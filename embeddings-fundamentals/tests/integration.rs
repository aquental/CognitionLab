//! Integration tests against real tools and providers (PLAN §5.3).
//!
//! These tests are `#[ignore]`d and also skip unless `RUN_INTEGRATION=1`, so the
//! default `cargo test` never depends on installed tools, downloads, or credentials.
//! A test that did not run is reported as outstanding, never as passed.

use embeddings_fundamentals::tools;

fn integration_enabled() -> bool {
    std::env::var("RUN_INTEGRATION").is_ok_and(|v| v == "1")
}

#[tokio::test]
#[ignore = "requires RUN_INTEGRATION=1 and installed poppler + tesseract"]
async fn required_external_tools_are_installed() {
    // An integration test that did not run is outstanding, never passed: fail loudly
    // when ignored tests are included without RUN_INTEGRATION=1.
    assert!(
        integration_enabled(),
        "outstanding: set RUN_INTEGRATION=1 to run integration tests (not reported as passed)"
    );
    let statuses = tools::probe(tools::REQUIRED_TOOLS).await;
    let missing: Vec<_> = statuses
        .iter()
        .filter(|s| s.version.is_none())
        .map(|s| s.name)
        .collect();
    assert!(missing.is_empty(), "missing external tools: {missing:?}");
}
