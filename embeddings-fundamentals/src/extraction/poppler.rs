//! Real extractors: poppler (`pdfinfo`, `pdftotext`, `pdftoppm`) and Tesseract.

use std::num::NonZeroU32;
use std::path::Path;
use std::process::Output;
use std::time::Duration;

use tokio::process::Command;

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
        let scratch = tempfile::tempdir()?;
        let prefix = scratch.path().join("page");
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
    }
}
