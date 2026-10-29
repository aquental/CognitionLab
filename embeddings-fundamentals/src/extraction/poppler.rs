//! Real extractors: poppler (`pdfinfo`, `pdftotext`, `pdftoppm`, `pdfimages`) and
//! Tesseract.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use tokio::process::Command;

use super::images::{ImageExtractor, ImageInfo, parse_list};
use super::{ExtractionConfig, ExtractionError, OcrEngine, PageText, TextExtractor, TextLine, tsv};
use crate::domain::ExtractionMethod;

const TOOL_TIMEOUT: Duration = Duration::from_secs(120);

async fn run(tool: &'static str, args: &[&std::ffi::OsStr]) -> Result<Output, ExtractionError> {
    let child = Command::new(tool).args(args).kill_on_drop(true).output();
    let output = tokio::time::timeout(TOOL_TIMEOUT, child)
        .await
        .map_err(|_| ExtractionError::Tool {
            tool,
            reason: "timed out".to_owned(),
        })?
        .map_err(|e| ExtractionError::Tool {
            tool,
            reason: e.to_string(),
        })?;
    if !output.status.success() {
        // stderr of poppler/tesseract describes the failure, never document text.
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr.lines().next().unwrap_or("non-zero exit").to_owned();
        return Err(ExtractionError::Tool { tool, reason });
    }
    Ok(output)
}

/// A scratch directory whose removal always runs on the blocking pool (PLAN §2.3):
/// `TempDir`'s recursive removal is synchronous file I/O and must not run on a runtime
/// worker, on success, on failure, or when the future that owns it is cancelled.
struct Scratch {
    path: PathBuf,
    dir: Option<tempfile::TempDir>,
}

fn blocking_failed(e: tokio::task::JoinError) -> ExtractionError {
    ExtractionError::Io(std::io::Error::other(e))
}

impl Scratch {
    /// Creates the directory (under `parent`, else in the system temporary directory)
    /// on the blocking pool. The guard itself is built there: if the awaiting future is
    /// cancelled after the directory exists but before the task's result is taken, the
    /// output dropped on the runtime thread is this guard, never a bare `TempDir`.
    async fn create(parent: Option<PathBuf>) -> Result<Self, ExtractionError> {
        tokio::task::spawn_blocking(move || -> Result<Self, ExtractionError> {
            let dir = match parent {
                Some(parent) => tempfile::tempdir_in(parent),
                None => tempfile::tempdir(),
            }?;
            Ok(Self {
                path: dir.path().to_owned(),
                dir: Some(dir),
            })
        })
        .await
        .map_err(blocking_failed)?
    }

    /// Removes the directory on the blocking pool and reports a removal failure.
    async fn close(mut self) -> Result<(), ExtractionError> {
        let Some(dir) = self.dir.take() else {
            return Ok(());
        };
        tokio::task::spawn_blocking(move || dir.close())
            .await
            .map_err(blocking_failed)?
            .map_err(ExtractionError::Io)
    }
}

impl Drop for Scratch {
    /// Reached with the directory still present only when the owning future was
    /// cancelled before `close`: the removal is handed to the blocking pool (outside a
    /// runtime there is no worker to protect, so it runs in place).
    fn drop(&mut self) {
        if let Some(dir) = self.dir.take() {
            match tokio::runtime::Handle::try_current() {
                Ok(runtime) => drop(runtime.spawn_blocking(move || drop(dir))),
                Err(_) => drop(dir),
            }
        }
    }
}

/// Runs `work` in a fresh [`Scratch`] directory and removes it afterwards.
async fn in_scratch<T>(
    work: impl AsyncFnOnce(&Path) -> Result<T, ExtractionError>,
) -> Result<T, ExtractionError> {
    in_scratch_under(None, work).await
}

/// [`in_scratch`] with the directory created under `parent` (tests observe it there).
async fn in_scratch_under<T>(
    parent: Option<PathBuf>,
    work: impl AsyncFnOnce(&Path) -> Result<T, ExtractionError>,
) -> Result<T, ExtractionError> {
    let scratch = Scratch::create(parent).await?;
    let outcome = work(&scratch.path).await;
    let removed = scratch.close().await;
    let value = outcome?;
    removed?;
    Ok(value)
}

async fn version(tool: &'static str, flag: &str) -> String {
    match Command::new(tool)
        .arg(flag)
        .kill_on_drop(true)
        .output()
        .await
    {
        Ok(out) if out.status.success() => {
            let text = if out.stdout.is_empty() {
                out.stderr
            } else {
                out.stdout
            };
            String::from_utf8_lossy(&text)
                .lines()
                .next()
                .unwrap_or(tool)
                .trim()
                .to_owned()
        }
        _ => format!("{tool} (version unknown)"),
    }
}

/// `pdftotext -layout` text layers; page count from `pdfinfo`, cross-checked with lopdf.
#[derive(Debug, Clone)]
pub struct PopplerTextExtractor {
    engine: String,
}

impl PopplerTextExtractor {
    /// Probes the installed poppler version.
    pub async fn detect() -> Self {
        Self {
            engine: version("pdftotext", "-v").await,
        }
    }
}

fn lopdf_page_count(pdf: &Path) -> Result<u32, ExtractionError> {
    let doc = lopdf::Document::load(pdf).map_err(|e| ExtractionError::Parse {
        what: "pdf structure",
        reason: e.to_string(),
    })?;
    u32::try_from(doc.get_pages().len()).map_err(|_| ExtractionError::Parse {
        what: "pdf structure",
        reason: "page count overflow".to_owned(),
    })
}

impl TextExtractor for PopplerTextExtractor {
    fn engine_id(&self) -> String {
        self.engine.clone()
    }

    async fn page_count(&self, pdf: &Path) -> Result<NonZeroU32, ExtractionError> {
        let output = run("pdfinfo", &[pdf.as_os_str()]).await?;
        let info = String::from_utf8_lossy(&output.stdout);
        let pdfinfo = info
            .lines()
            .find_map(|l| l.strip_prefix("Pages:"))
            .and_then(|n| n.trim().parse::<u32>().ok())
            .ok_or_else(|| ExtractionError::Parse {
                what: "pdfinfo output",
                reason: "no Pages: line".to_owned(),
            })?;
        let path = pdf.to_owned();
        let lopdf = tokio::task::spawn_blocking(move || lopdf_page_count(&path))
            .await
            .map_err(|e| ExtractionError::Tool {
                tool: "lopdf",
                reason: e.to_string(),
            })??;
        if lopdf != pdfinfo {
            return Err(ExtractionError::PageCountMismatch { pdfinfo, lopdf });
        }
        NonZeroU32::new(pdfinfo).ok_or_else(|| ExtractionError::Parse {
            what: "pdfinfo output",
            reason: "document has no pages".to_owned(),
        })
    }

    async fn extract_page(
        &self,
        pdf: &Path,
        page: NonZeroU32,
    ) -> Result<PageText, ExtractionError> {
        let n = page.to_string();
        let args: [&std::ffi::OsStr; 7] = [
            "-layout".as_ref(),
            "-f".as_ref(),
            n.as_ref(),
            "-l".as_ref(),
            n.as_ref(),
            pdf.as_os_str(),
            "-".as_ref(),
        ];
        let output = run("pdftotext", &args).await?;
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(PageText {
            page,
            method: ExtractionMethod::Native,
            engine: self.engine.clone(),
            lines: native_lines(&text),
            confidence: None,
        })
    }
}

/// Splits `pdftotext` output into lines, dropping form feeds and trailing whitespace.
#[must_use]
pub fn native_lines(text: &str) -> Vec<TextLine> {
    let mut lines: Vec<TextLine> = text
        .replace('\u{c}', "")
        .lines()
        .map(|l| TextLine {
            text: l.trim_end().to_owned(),
            bounding_box: None,
        })
        .collect();
    while lines.last().is_some_and(|l| l.text.is_empty()) {
        lines.pop();
    }
    lines
}

/// `pdftoppm -png -singlefile` rendering followed by `tesseract ... tsv`.
#[derive(Debug, Clone)]
pub struct TesseractOcr {
    engine: String,
    config: ExtractionConfig,
}

impl TesseractOcr {
    /// Probes the installed Tesseract version.
    pub async fn detect(config: ExtractionConfig) -> Self {
        Self {
            engine: version("tesseract", "--version").await,
            config,
        }
    }
}

impl OcrEngine for TesseractOcr {
    fn engine_id(&self) -> String {
        self.engine.clone()
    }

    async fn ocr_page(&self, pdf: &Path, page: NonZeroU32) -> Result<PageText, ExtractionError> {
        in_scratch(async |scratch| {
            let prefix = scratch.join("page");
            let (n, dpi) = (page.to_string(), self.config.ocr_dpi.to_string());
            let render: [&std::ffi::OsStr; 11] = [
                "-q".as_ref(),
                "-png".as_ref(),
                "-singlefile".as_ref(),
                "-r".as_ref(),
                dpi.as_ref(),
                "-f".as_ref(),
                n.as_ref(),
                "-l".as_ref(),
                n.as_ref(),
                pdf.as_os_str(),
                prefix.as_os_str(),
            ];
            run("pdftoppm", &render).await?;
            let png = prefix.with_extension("png");
            let psm = self.config.ocr_psm.to_string();
            let ocr: [&std::ffi::OsStr; 7] = [
                png.as_os_str(),
                "-".as_ref(),
                "-l".as_ref(),
                self.config.ocr_lang.as_ref(),
                "--psm".as_ref(),
                psm.as_ref(),
                "tsv".as_ref(),
            ];
            let output = run("tesseract", &ocr).await?;
            tsv::parse(
                &String::from_utf8_lossy(&output.stdout),
                page,
                self.config.ocr_dpi,
                &self.engine,
            )
        })
        .await
    }
}

/// `pdfimages -list` and `pdfimages -png -p` (poppler).
#[derive(Debug, Clone)]
pub struct PopplerImageExtractor {
    engine: String,
}

impl PopplerImageExtractor {
    /// Probes the installed poppler version.
    pub async fn detect() -> Self {
        Self {
            engine: version("pdfimages", "-v").await,
        }
    }
}

impl ImageExtractor for PopplerImageExtractor {
    fn engine_id(&self) -> String {
        self.engine.clone()
    }

    async fn list_images(&self, pdf: &Path) -> Result<Vec<ImageInfo>, ExtractionError> {
        let output = run("pdfimages", &["-list".as_ref(), pdf.as_os_str()]).await?;
        parse_list(&String::from_utf8_lossy(&output.stdout))
    }

    async fn extract_image(
        &self,
        pdf: &Path,
        image: &ImageInfo,
    ) -> Result<Vec<u8>, ExtractionError> {
        in_scratch(async |scratch| {
            let root = scratch.join("img");
            let n = image.page.to_string();
            let args: [&std::ffi::OsStr; 8] = [
                "-png".as_ref(),
                "-p".as_ref(),
                "-f".as_ref(),
                n.as_ref(),
                "-l".as_ref(),
                n.as_ref(),
                pdf.as_os_str(),
                root.as_os_str(),
            ];
            run("pdfimages", &args).await?;
            // With -p and one page, files are named <root>-<page:03>-<position:03>.png.
            let file = scratch.join(format!(
                "img-{:03}-{:03}.png",
                image.page.get(),
                image.position_on_page
            ));
            tokio::fs::read(&file)
                .await
                .map_err(|_| ExtractionError::Tool {
                    tool: "pdfimages",
                    reason: format!(
                        "image {} of page {} was not written",
                        image.position_on_page, image.page
                    ),
                })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cancelled extraction drops its scratch directory without removing it on the
    /// runtime thread: with the only blocking thread held, the directory must still exist
    /// right after the cancellation, and it is gone once the blocking pool runs.
    #[test]
    fn a_cancelled_extraction_removes_its_scratch_directory_on_the_blocking_pool() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(1)
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let (sent, entered) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(in_scratch(async move |dir: &Path| {
                tokio::fs::write(dir.join("img-001-000.png"), b"x").await?;
                sent.send(dir.to_owned()).expect("send");
                std::future::pending::<Result<(), ExtractionError>>().await
            }));
            let dir = entered.await.expect("scratch created");
            let (release, held) = std::sync::mpsc::channel::<()>();
            let blocker = tokio::task::spawn_blocking(move || held.recv());
            task.abort();
            assert!(task.await.expect_err("cancelled").is_cancelled());
            assert!(
                dir.exists(),
                "the scratch directory was removed on the runtime thread"
            );
            release.send(()).expect("release");
            blocker.await.expect("blocker").expect("recv");
            // The blocking pool runs its queue in order on its single thread.
            tokio::task::spawn_blocking(|| ()).await.expect("flush");
            assert!(!dir.exists(), "{} was not removed", dir.display());
        });
    }

    /// Regression (C3-4, Codex round 3): cancelled after the directory was created on
    /// the blocking pool but before the waiting task took the result. The dropped task
    /// output must be a guard, so nothing is removed on the runtime thread.
    #[test]
    fn a_scratch_directory_cancelled_before_its_creation_is_taken_is_removed_off_the_runtime() {
        let parent = tempfile::tempdir().expect("parent");
        let entries = || std::fs::read_dir(parent.path()).expect("parent").count();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(1)
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let task = tokio::spawn(in_scratch_under(
                Some(parent.path().to_owned()),
                async |_dir: &Path| -> Result<(), ExtractionError> {
                    Err(ExtractionError::Tool {
                        tool: "pdfimages",
                        reason: "work must not start".into(),
                    })
                },
            ));
            // The task starts, queues the directory creation, and waits for it.
            tokio::task::yield_now().await;
            // Queued behind the creation on the only blocking thread; it then holds the
            // pool. Waiting for it blocks the runtime thread, so the task cannot resume.
            let (ran, created) = std::sync::mpsc::channel();
            let (release, held) = std::sync::mpsc::channel::<()>();
            let blocker = tokio::task::spawn_blocking(move || {
                ran.send(()).expect("signal");
                held.recv()
            });
            created.recv().expect("creation finished");
            assert_eq!(entries(), 1, "the scratch directory was not created");
            task.abort();
            assert!(task.await.expect_err("cancelled").is_cancelled());
            assert_eq!(
                entries(),
                1,
                "the scratch directory was removed on the runtime thread"
            );
            release.send(()).expect("release");
            blocker.await.expect("blocker").expect("recv");
            tokio::task::spawn_blocking(|| ()).await.expect("flush");
            assert_eq!(entries(), 0, "the scratch directory was not removed");
        });
    }

    #[tokio::test(flavor = "current_thread")]
    async fn scratch_directories_are_removed_on_success_and_failure() {
        let mut seen = Vec::new();
        let value = in_scratch(async |dir| {
            tokio::fs::write(dir.join("img-001-000.png"), b"x").await?;
            seen.push(dir.to_owned());
            Ok(7)
        })
        .await
        .expect("success");
        assert_eq!(value, 7);
        let failed: Result<(), _> = in_scratch(async |dir| {
            tokio::fs::write(dir.join("partial"), b"x").await?;
            seen.push(dir.to_owned());
            Err(ExtractionError::Tool {
                tool: "pdfimages",
                reason: "injected".into(),
            })
        })
        .await;
        assert!(matches!(failed, Err(ExtractionError::Tool { .. })));
        assert_eq!(seen.len(), 2);
        for dir in &seen {
            assert!(!dir.exists(), "{} was not removed", dir.display());
        }
    }
}
