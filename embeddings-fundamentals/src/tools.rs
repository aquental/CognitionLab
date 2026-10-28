//! Discovery of the external tools the pipeline shells out to.

use std::time::Duration;

use tokio::process::Command;

/// An external tool the pipeline depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
    /// Executable name.
    pub name: &'static str,
    /// Arguments that print a version and exit.
    pub version_args: &'static [&'static str],
}

/// Tools required by extraction and OCR (PLAN §0.1).
pub const REQUIRED_TOOLS: &[Tool] = &[
    Tool {
        name: "pdfinfo",
        version_args: &["-v"],
    },
    Tool {
        name: "pdftotext",
        version_args: &["-v"],
    },
    Tool {
        name: "pdftoppm",
        version_args: &["-v"],
    },
    Tool {
        name: "pdfimages",
        version_args: &["-v"],
    },
    Tool {
        name: "tesseract",
        version_args: &["--version"],
    },
];

/// Result of probing one tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolStatus {
    /// Executable name.
    pub name: &'static str,
    /// First version line, or `None` when the tool is missing or failed.
    pub version: Option<String>,
}

/// Runs each tool's version command with a timeout and reports the first output line.
pub async fn probe(tools: &[Tool]) -> Vec<ToolStatus> {
    let mut statuses = Vec::with_capacity(tools.len());
    for tool in tools {
        let output = tokio::time::timeout(
            Duration::from_secs(10),
            Command::new(tool.name)
                .args(tool.version_args)
                .kill_on_drop(true)
                .output(),
        )
        .await;
        let version = match output {
            // A tool that exits non-zero is broken, whatever it prints.
            Ok(Ok(out)) if out.status.success() => {
                // poppler prints its version on stderr, tesseract on stdout.
                let text = if out.stdout.is_empty() {
                    out.stderr
                } else {
                    out.stdout
                };
                String::from_utf8_lossy(&text)
                    .lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .map(str::to_owned)
            }
            Ok(Ok(_)) | Ok(Err(_)) | Err(_) => None,
        };
        statuses.push(ToolStatus {
            name: tool.name,
            version,
        });
    }
    statuses
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn failing_or_missing_tools_are_reported_missing() {
        let statuses = probe(&[
            // `sh -c` exits 1 after printing to stderr: a broken install.
            Tool {
                name: "sh",
                version_args: &["-c", "echo 'tool: error' >&2; exit 1"],
            },
            Tool {
                name: "definitely-not-an-installed-tool",
                version_args: &["--version"],
            },
            Tool {
                name: "sh",
                version_args: &["-c", "echo 'tool 1.2.3'"],
            },
        ])
        .await;
        assert_eq!(
            statuses[0].version, None,
            "non-zero exit must not count as installed"
        );
        assert_eq!(statuses[1].version, None);
        assert_eq!(statuses[2].version.as_deref(), Some("tool 1.2.3"));
    }
}
