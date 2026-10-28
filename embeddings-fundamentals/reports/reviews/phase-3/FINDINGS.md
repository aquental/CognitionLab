# Phase 3 — adversarial review findings

Reviewer reports are stored verbatim beside this file. Review diff base: `3294dbd` (approved Phase 2 head). Branch `phase-3/image-retrieval`.

Pre-review fix found by the implementer after packaging Grok round 1 (not a reviewer finding): S3-1. `vision::load` now refuses a checkpoint whose image preprocessing has no fixed input size (a shortest-edge resize without a crop), because such images cannot be stacked into one batch tensor. `ImagePreprocessing::fixed_output_size` exists for this check; both pinned models pass it (224×224, 256×256). Covered by `the_pinned_clip_and_siglip_configs_are_read_exactly`.

## Grok round 1 (`68a0585`)

| ID | Reviewer | Severity | Description | Resolution / refutation | Files | Validation evidence | Reviewer confirmation | Status |
|---|---|---|---|---|---|---|---|---|
| G3-1 | Grok | high | An image reprocess that keeps no image (changed bytes, filter, or preparation) returned `not_applicable` before `commit`, so the previous active pointer kept answering searches while the report named an unpublished revision | Valid. When nothing qualifies, the source's active pointer in that space is removed (`FsRecordStore::deactivate`: unlink + fsync of `active/`, writer only). Revisions are kept and stay readable through the snapshots that pin them. The report carries no revision id, and its reason says when a previously active revision was deactivated | `src/storage.rs`, `src/pipeline.rs` | `tests/pipeline.rs::an_image_reprocess_that_keeps_nothing_deactivates_the_old_revision` (publish, re-ingest with `min_side_px = 512`: pointer gone, no active record, earlier snapshot still reads its pinned record, a third run stays `not_applicable` with nothing to deactivate); a mutant that skips the deactivation fails it | pending | fixed |
