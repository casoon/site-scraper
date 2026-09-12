//! Terminal output through runemark: status lines, a progress bar, run
//! summaries and error blocks.
//!
//! Status lines and progress go to stderr (status lines are printed above an
//! active progress bar); the final summary goes to stdout.

use std::fmt;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use runemark::{
    ColorMode, Console, ErrorBlock, Metric, ProgressMode, ProgressSink, Report, TerminalProgress,
    Tone, Verdict,
};

struct Output {
    progress: TerminalProgress,
    console: Console,
    interactive: bool,
    started: AtomicBool,
}

static OUTPUT: OnceLock<Output> = OnceLock::new();

fn output() -> &'static Output {
    OUTPUT.get_or_init(|| {
        let is_terminal = std::io::stderr().is_terminal();
        let console = Console::stderr(ColorMode::Auto);
        Output {
            progress: TerminalProgress::stderr(ProgressMode::Auto, console, is_terminal),
            console,
            interactive: ProgressMode::Auto.is_interactive(is_terminal),
            started: AtomicBool::new(false),
        }
    })
}

fn line(verdict: Verdict, tone: Tone, message: &str) {
    let out = output();
    let text = format!("{} {}", verdict.symbol(out.console.symbol_theme()), message);
    if out.interactive && !out.started.load(Ordering::Relaxed) {
        // Printing through a bar that has not started would draw an empty bar
        eprintln!("{}", out.console.paint(tone, text));
    } else {
        out.progress.notice(tone, &text);
    }
}

/// A page or screenshot was saved.
pub fn success(message: &str) {
    line(Verdict::Passed, Tone::Muted, message);
}

pub fn info(message: &str) {
    line(Verdict::Info, Tone::Info, message);
}

pub fn warning(message: &str) {
    line(Verdict::Warning, Tone::Warning, message);
}

pub fn failure(message: &str) {
    line(Verdict::Failed, Tone::Error, message);
}

/// Something optional was left out (e.g. an asset that could not be downloaded).
pub fn skipped(message: &str) {
    line(Verdict::Skipped, Tone::Muted, message);
}

/// Start a progress bar with a known total.
// Only used by the headless screenshot runner.
#[cfg_attr(not(feature = "headless"), allow(dead_code))]
pub fn progress_start(total: usize, message: &str) {
    output().started.store(true, Ordering::Relaxed);
    output().progress.start(total as u64, message);
}

/// Grow the progress total while a crawl discovers pages. Only an interactive
/// bar is resized; plain logs would otherwise get a start line per batch.
pub fn progress_grow(total: usize, message: &str) {
    let out = output();
    if out.interactive {
        out.started.store(true, Ordering::Relaxed);
        out.progress.start(total as u64, message);
    }
}

pub fn progress_advance(done: usize, current: &str) {
    output().progress.advance(done as u64, current);
}

/// Close the progress bar and print a summary report to stdout.
fn finish(title: &str, verdict: Verdict, metrics: Vec<Metric>) {
    let out = output();
    // Only a live bar needs closing; plain logs get the summary below
    if out.interactive && out.started.load(Ordering::Relaxed) {
        out.progress.finish(verdict, "Done");
        // The finished bar leaves the cursor at the end of its line
        eprintln!();
    }
    let report = metrics
        .into_iter()
        .fold(Report::new(title, verdict), Report::add_metric);
    print!("{}", report.render(Console::stdout(ColorMode::Auto)));
}

/// Summarize a crawl; fails (already reported) if any page failed.
pub fn crawl_summary(
    start_url: &str,
    out_dir: &Path,
    pages: usize,
    failed: usize,
    warnings: usize,
    started: Instant,
) -> anyhow::Result<()> {
    finish(
        &format!("{} → {}", start_url, out_dir.display()),
        verdict(failed, warnings),
        vec![
            Metric::new("Pages", pages.to_string()),
            count_metric("Failed", failed, Tone::Error),
            count_metric("Warnings", warnings, Tone::Warning),
            Metric::new("Duration", format_duration(started)),
        ],
    );
    if failed > 0 {
        return Err(Reported.into());
    }
    Ok(())
}

/// Summarize a screenshot run; fails (already reported) if any screenshot failed.
// Only used by the headless screenshot runner.
#[cfg_attr(not(feature = "headless"), allow(dead_code))]
pub fn screenshot_summary(
    out_dir: &Path,
    total: usize,
    failed: usize,
    started: Instant,
) -> anyhow::Result<()> {
    finish(
        &format!("Screenshots → {}", out_dir.display()),
        verdict(failed, 0),
        vec![
            Metric::new("Saved", (total - failed).to_string()),
            count_metric("Failed", failed, Tone::Error),
            Metric::new("Duration", format_duration(started)),
        ],
    );
    if failed > 0 {
        return Err(Reported.into());
    }
    Ok(())
}

fn verdict(failed: usize, warnings: usize) -> Verdict {
    if failed > 0 {
        Verdict::Failed
    } else if warnings > 0 {
        Verdict::Warning
    } else {
        Verdict::Passed
    }
}

fn count_metric(key: &str, count: usize, tone: Tone) -> Metric {
    let metric = Metric::new(key, count.to_string());
    if count > 0 {
        metric.with_tone(tone)
    } else {
        metric
    }
}

fn format_duration(started: Instant) -> String {
    let secs = started.elapsed().as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else {
        format!("{}m {}s", secs / 60, secs % 60)
    }
}

/// An error whose details were already shown; `main` only sets the exit code.
#[derive(Debug)]
pub struct Reported;

impl fmt::Display for Reported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "already reported")
    }
}

impl std::error::Error for Reported {}

/// An error shown as a runemark error block with remedy and commands.
#[derive(Debug)]
pub struct BlockError(pub ErrorBlock);

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.heading)
    }
}

impl std::error::Error for BlockError {}

/// Print a fatal error as an error block on stderr.
pub fn print_error(err: &anyhow::Error) {
    if err.is::<Reported>() {
        return;
    }
    let block = match err.downcast_ref::<BlockError>() {
        Some(BlockError(block)) => block.clone(),
        None => {
            let block = ErrorBlock::new(err.to_string());
            let causes: Vec<String> = err.chain().skip(1).map(|c| c.to_string()).collect();
            if causes.is_empty() {
                block
            } else {
                // runemark indents only the first explanation line
                block.with_explanation(causes.join("\n").replace('\n', "\n  "))
            }
        }
    };
    eprint!("{}", block.render(Console::stderr(ColorMode::Auto)));
}
