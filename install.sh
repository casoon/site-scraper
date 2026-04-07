#!/usr/bin/env bash
set -euo pipefail

REPO="casoon/site-scraper"
BINARY="site-scraper"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  OS_TAG="linux" ;;
    Darwin) OS_TAG="macos" ;;
    *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
    x86_64|amd64)  ARCH_TAG="x86_64" ;;
    aarch64|arm64) ARCH_TAG="aarch64" ;;
    *)             echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

ASSET_NAME="${BINARY}-${OS_TAG}-${ARCH_TAG}.tar.gz"

# Get latest release tag
echo "Fetching latest release..."
TAG=$(curl -sL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -1 | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')

if [ -z "$TAG" ]; then
    echo "Error: Could not determine latest release."
    exit 1
fi

echo "Installing ${BINARY} ${TAG}..."

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${TAG}/${ASSET_NAME}"
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Downloading ${DOWNLOAD_URL}..."
curl -sL "$DOWNLOAD_URL" -o "${TMP_DIR}/${ASSET_NAME}"

echo "Extracting..."
tar -xzf "${TMP_DIR}/${ASSET_NAME}" -C "$TMP_DIR"

echo "Installing to ${INSTALL_DIR}/${BINARY}..."
if [ -w "$INSTALL_DIR" ]; then
    mv "${TMP_DIR}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
else
    sudo mv "${TMP_DIR}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
fi
chmod +x "${INSTALL_DIR}/${BINARY}"

echo "Done! ${BINARY} ${TAG} installed to ${INSTALL_DIR}/${BINARY}"
