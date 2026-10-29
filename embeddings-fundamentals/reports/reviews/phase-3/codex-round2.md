C3-1: closure confirmed.
C3-2: closure confirmed.
C3-3: closure confirmed.
C3-4: closure rejected.
C3-5: closure confirmed.

ID: C3-4  
Severity: medium  
File:line: src/extraction/poppler.rs:48  
Description: `in_scratch` still owns a `TempDir` across `work(...).await`. Cancelling the enclosing future drops that directory synchronously on the runtime worker, bypassing the blocking-pool cleanup. This violates `async-spawn-blocking` and PLAN §2.3.  
Impact: Cancelling OCR or image extraction can block an executor thread while recursively deleting generated files.  
Reproduction scenario or verifiable rationale: Cancel an extraction future after scratch files exist but before `work` completes. Execution never reaches the subsequent `spawn_blocking`; `TempDir::drop` performs cleanup on the cancelling thread. The regression test covers returned success and errors only.  
Validation suggestion: Add cancellation coverage and verify the cleanup thread. Make scratch ownership schedule blocking cleanup when dropped, including during cancellation.

ID: C3-6  
Severity: medium  
File:line: src/extraction/poppler.rs:292  
Description: PDF image extraction reads only the base image PNG and discards its separately extracted soft mask. The scrubber’s alpha compositing therefore cannot preserve transparency for qualifying PDF images.  
Impact: Stored and embedded images can reveal pixels hidden in the original PDF and differ visibly from the original image.  
Reproduction scenario or verifiable rationale: Use a PDF page with usable native text and a qualifying RGB image whose `/SMask` hides colored pixels. `pdfimages` emits separate image and mask files; this method returns only the base image, while `ImageFilter::keeps` excludes the mask. The returned RGB PNG has no alpha for `flatten_on_white` to apply. Existing fixtures mask only a discarded 48-pixel logo.  
Validation suggestion: Add a qualifying masked PDF image fixture with known hidden pixels. Apply its associated mask before compositing over white, and assert that the stored asset preserves the visible image without exposing hidden colors.