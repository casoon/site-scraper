use std::path::Path;

use anyhow::Result;
use image::{ImageBuffer, Rgba};
use url::Url;

use crate::network::fetch::fetch_with_retry;
use crate::utils::filesystem::ensure_dir;
use crate::utils::url::url_to_local_path;

pub struct PlaceholderResult {
    pub src: String,
    pub width: u32,
    pub height: u32,
}

/// Decide placeholder URL or create local placeholder for an image.
pub async fn get_placeholder_for_image(
    img_url: &Url,
    strategy: &str,
    out_dir: &Path,
    root: &Url,
) -> PlaceholderResult {
    if strategy == "real" {
        let local_path = url_to_local_path(root, img_url, out_dir, None);
        if let Some(parent) = local_path.parent() {
            let _ = ensure_dir(parent).await;
        }
        if let Some((w, h)) = download_and_save(img_url, &local_path).await {
            return PlaceholderResult {
                src: local_path.to_string_lossy().to_string(),
                width: w,
                height: h,
            };
        }
        return PlaceholderResult {
            src: format!("https://placehold.co/800x450"),
            width: 800,
            height: 450,
        };
    }

    // Try to probe dimensions from the first bytes
    let (width, height) = probe_dimensions(img_url).await;

    if strategy == "local" {
        let local_path = url_to_local_path(root, img_url, out_dir, Some(".png"));
        if let Some(parent) = local_path.parent() {
            let _ = ensure_dir(parent).await;
        }

        let w = width.clamp(1, 4096);
        let h = height.clamp(1, 4096);

        // Generate a simple gray placeholder PNG
        if generate_placeholder_png(&local_path, w, h).is_ok() {
            return PlaceholderResult {
                src: local_path.to_string_lossy().to_string(),
                width: w,
                height: h,
            };
        }
    }

    // External placeholder via placehold.co
    PlaceholderResult {
        src: format!("https://placehold.co/{}x{}", width, height),
        width,
        height,
    }
}

/// Fetch image bytes, probe dimensions, and save to disk in a single request.
async fn download_and_save(img_url: &Url, dest: &Path) -> Option<(u32, u32)> {
    match fetch_with_retry(img_url.as_str(), 3, 400).await {
        Ok(resp) => match resp.bytes().await {
            Ok(bytes) => {
                let (w, h) = match imagesize::blob_size(&bytes) {
                    Ok(size) => (size.width as u32, size.height as u32),
                    Err(_) => (800, 450),
                };
                let _ = tokio::fs::write(dest, &bytes).await;
                Some((w, h))
            }
            Err(_) => None,
        },
        Err(_) => None,
    }
}

async fn probe_dimensions(img_url: &Url) -> (u32, u32) {
    match fetch_with_retry(img_url.as_str(), 3, 400).await {
        Ok(resp) => match resp.bytes().await {
            Ok(bytes) => match imagesize::blob_size(&bytes) {
                Ok(size) => (size.width as u32, size.height as u32),
                Err(_) => (800, 450),
            },
            Err(_) => (800, 450),
        },
        Err(_) => (800, 450),
    }
}

fn generate_placeholder_png(path: &Path, w: u32, h: u32) -> Result<()> {
    // Create a solid gray (#e5e7eb) placeholder
    let gray = Rgba([0xe5u8, 0xe7, 0xeb, 0xff]);
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_pixel(w, h, gray);
    img.save(path)?;
    Ok(())
}
