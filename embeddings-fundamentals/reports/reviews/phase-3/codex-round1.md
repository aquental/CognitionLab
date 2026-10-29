ID: C3-1
Severity: medium
File:line: src/pipeline.rs:705
Description: Image re-ingestion treats an intact records file as an intact revision without checking its referenced assets.
Impact: Missing or corrupted PNGs survive subsequent ingestion indefinitely while the source is reported as succeeded and unchanged.
Reproduction scenario or verifiable rationale: Ingest an image, delete or corrupt its stored PNG, then ingest the unchanged manifest with a fresh experiment ID. `begin_source` returns before `source_images` and `put_asset`, so the asset-repair logic never runs. The unchanged revision still contains the broken URI.
Validation suggestion: Add deletion and corruption regression cases. Verify referenced assets before accepting an image revision as unchanged; repair them or report failure without replacing snapshot-pinned records.

ID: C3-2
Severity: medium
File:line: src/assets.rs:78
Description: Converting directly to RGB8 discards PNG transparency without compositing it.
Impact: Stored assets can display and embed pixels that were invisible in the original image, changing its content and potentially exposing information hidden beneath transparent regions.
Reproduction scenario or verifiable rationale: Supply an RGBA PNG containing distinctive RGB pixels with alpha zero. `to_rgb8()` retains those RGB values and removes alpha, making the previously transparent region opaque in the stored PNG. Existing tests cover opaque RGB fixtures only.
Validation suggestion: Add a transparent PNG fixture with contrasting hidden pixels. Preserve transparency in stored assets or explicitly composite against a defined background before encoding and embedding.

ID: C3-3
Severity: medium
File:line: src/assets.rs:75
Description: The scrubber removes EXIF orientation without applying that orientation to the decoded pixels.
Impact: A JPEG displayed upright by an orientation-aware viewer becomes rotated or mirrored in its stored asset and subsequent embedding.
Reproduction scenario or verifiable rationale: Ingest an asymmetric JPEG with EXIF Orientation 6. The code decodes its raw raster, converts it to RGB8, and writes a metadata-free PNG without any orientation transformation. The output loses the instruction that rotates the original for display.
Validation suggestion: Test rotated and mirrored EXIF orientations with independently expected pixel positions. Apply the orientation before removing metadata and record the resulting dimensions.

ID: C3-4
Severity: medium
File:line: src/extraction/poppler.rs:284
Description: Dropping `scratch` synchronously removes the extraction directory on the async runtime worker. This violates PLAN §2.3 and `async-spawn-blocking`.
Impact: Image extraction can stall unrelated tasks and timers while recursively deleting all images extracted from a page.
Reproduction scenario or verifiable rationale: `pdfimages` writes every image on the selected page into the temporary directory. After asynchronously reading one file, `extract_image` returns and runs `TempDir`’s synchronous filesystem cleanup. The same cleanup also occurs on error paths.
Validation suggestion: Move temporary-directory cleanup onto the blocking pool, including failure paths. Add a runtime-responsiveness regression covering a page with many extracted files.

ID: C3-5
Severity: medium
File:line: src/evaluation.rs:616
Description: Image-label applicability checks only the original filename and PDF image number, which do not identify image content across source revisions.
Impact: Evaluation can silently apply frozen relevance judgments to replacement images and report invalid retrieval metrics.
Reproduction scenario or verifiable rationale: Replace an image file’s contents while retaining its filename and source ID, update the manifest checksum, and re-ingest. Its chunk ID and `OriginalAsset::reference()` remain unchanged, so the old label passes. Replacing a PDF image at the same page and ordinal has the same effect.
Validation suggestion: Bind image labels to a content checksum or immutable source revision plus image locator. Add a regression that replaces pixels under an unchanged original reference and requires evaluation to reject the stale judgment.