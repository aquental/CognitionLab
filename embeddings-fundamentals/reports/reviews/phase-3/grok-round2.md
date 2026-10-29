The review package was truncated in the message. I am reading the rest of the inlined diff before judging Phase 3.The report JSON is repeated. I am locating the source diffs so the review covers the code, not the evidence dumps.ID: G3-2
Severity: low
File:line: src/adapters.rs:4175
Description: `build_vision` rejects `prefix_policy != None` as `BuildError::Load`. `exit_code` maps `UnsupportedModelKind` to validation exit 3 and leaves `Load` at exit 1. FastEmbed and OpenAI `text_to_image` already use the validation variant. This check runs before any download.
Impact: A cross-modal config that sets a prefix is reported as an adapter load failure (exit 1). The same class of engine/config mismatch exits 3. Shipped image configs use `none`, so the happy path is unchanged.
Reproduction scenario or verifiable rationale: `adapters::build` with `engine = embed_anything`, `model_kind = text_to_image`, and `prefix_policy = query` returns `BuildError::Load`, whose `exit_code()` is 1. The `UnsupportedModelKind` arm in the same `exit_code` match is 3.
Validation suggestion: Assert `exit_code() == 3` for that config and that no model download starts. Return `UnsupportedModelKind` or another validation variant.

ID: G3-3
Severity: low
File:line: tests/pipeline.rs:9689
Description: In `an_image_run_embeds_image_files_and_qualifying_pdf_images`, the image-file branch sets the expected pixel size to `(original.width, original.height)` taken from the record under test. `ExtractionMethod::ImageFile` and `pdf_image_num == None` are real checks. The embedded image's `(EmbeddedImage, Some(0), (256, 192))` is an independent constant.
Impact: A regression that stores the wrong width or height for `chest-001` or `abd-us-001` still passes this assertion. BDD scrubbed-byte checks cover dimensions elsewhere, so the hole is this assertion.
Reproduction scenario or verifiable rationale: After scrub, write a different `width`/`height` onto image-file `OriginalAsset` values. The image-file arm still compares the record to itself and passes.
Validation suggestion: Hard-code the fixture PNG and JPEG dimensions, or decode the corpus files, and compare those constants to the record.
