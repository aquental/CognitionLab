# Phase 3 — adversarial review findings

Reviewer reports are stored verbatim beside this file. Review diff base: `3294dbd` (approved Phase 2 head). Branch `phase-3/image-retrieval`.

Pre-review fix found by the implementer after packaging Grok round 1 (not a reviewer finding): S3-1. `vision::load` now refuses a checkpoint whose image preprocessing has no fixed input size (a shortest-edge resize without a crop), because such images cannot be stacked into one batch tensor. `ImagePreprocessing::fixed_output_size` exists for this check; both pinned models pass it (224×224, 256×256). Covered by `the_pinned_clip_and_siglip_configs_are_read_exactly`.

## Grok round 1 (`68a0585`)

| ID | Reviewer | Severity | Description | Resolution / refutation | Files | Validation evidence | Reviewer confirmation | Status |
|---|---|---|---|---|---|---|---|---|
| G3-1 | Grok | high | An image reprocess that keeps no image (changed bytes, filter, or preparation) returned `not_applicable` before `commit`, so the previous active pointer kept answering searches while the report named an unpublished revision | Valid. When nothing qualifies, the source's active pointer in that space is removed (`FsRecordStore::deactivate`: unlink + fsync of `active/`, writer only). Revisions are kept and stay readable through the snapshots that pin them. The report carries no revision id, and its reason says when a previously active revision was deactivated | `src/storage.rs`, `src/pipeline.rs` | `tests/pipeline.rs::an_image_reprocess_that_keeps_nothing_deactivates_the_old_revision` (publish, re-ingest with `min_side_px = 512`: pointer gone, no active record, earlier snapshot still reads its pinned record, a third run stays `not_applicable` with nothing to deactivate); a mutant that skips the deactivation fails it | Grok round 2: no further finding on it (only G3-2, G3-3 raised) | fixed |

## Grok round 2 (`effd663`)

| ID | Reviewer | Severity | Description | Resolution / refutation | Files | Validation evidence | Reviewer confirmation | Status |
|---|---|---|---|---|---|---|---|---|
| G3-2 | Grok | low | A cross-modal config with a prefix was refused as `BuildError::Load` (exit 1), while the same class of configuration mismatch exits 3 | Valid. New `BuildError::PrefixNotSupported` (exit 3), checked in `adapters::build` before any revision check, download, or credential read, for cross-modal `embed_anything` models and for OpenAI (whose prefix refusal had the same defect). The per-adapter `Load` checks are removed | `src/adapters.rs` | `a_prefix_on_a_model_without_one_is_a_validation_refusal` (both engines, exit 3); compiles and passes without engine features, so the refusal happens before any adapter code | pending | fixed |
| G3-3 | Grok | low | The image-file branch of `an_image_run_embeds_image_files_and_qualifying_pdf_images` compared the record's pixel size with itself | Valid. Expected sizes are the fixture files' own sizes (chest 256×256, abd-us 256×192, embedded 256×192), and an unexpected source fails the test | `tests/pipeline.rs` | test passes; a record with another size now fails it | pending | fixed |
