# Antigravity Adversarial Review — Stage 0 (Round 3)

## Part 1: Status of Round 2 Findings (A0-1..A0-6)

- **A0-1: CONFIRMED**
  - **Fix:** [`Document.stream_dictionary`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L171-L185) (`GAP.fullmatch(self.data, end, keyword)` at [`src/anonimizador/pdfstreams.py:183`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L183), which rejects `end > keyword` and requires only whitespace/comments before `stream`) and [`decoded_streams`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L397-L440) (`pos = start` when `entries is None and not unknown` at [`src/anonimizador/pdfstreams.py:431-432`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L431-L432)).
  - **Test:** [`test_a_stream_keyword_in_a_comment_or_a_string_hides_no_stream`](file://<review-worktree>/anonimizador/tests/test_channels.py#L971-L981) (`tests/test_channels.py:971-981`).

- **A0-2: CONFIRMED**
  - **Fix:** [`GAP`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L67) and [`DICTIONARY_BEFORE`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L70) (`src/anonimizador/pdfstreams.py:67,70`), [`Document.stream_dictionary`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L171-L185) (`src/anonimizador/pdfstreams.py:183`), and [`decoded_streams`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L419-L424) (`unread += unknown` before `flate_decode` at [`src/anonimizador/pdfstreams.py:419-424`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L419-L424)).
  - **Test:** [`test_a_stream_dictionary_that_cannot_be_read_is_unread_even_when_it_inflates`](file://<review-worktree>/anonimizador/tests/test_channels.py#L984-L998) (`tests/test_channels.py:984-998`).

- **A0-3: CONFIRMED**
  - **Fix:** [`pdf_string_codec`](file://<review-worktree>/anonimizador/src/anonimizador/channels.py#L369-L381) (`src/anonimizador/channels.py:369-381`) and [`outer_text`](file://<review-worktree>/anonimizador/src/anonimizador/channels.py#L454-L465) (`src/anonimizador/channels.py:454-465`).
  - **Test:** [`test_a_deeper_literal_keeps_its_utf16_text_aligned`](file://<review-worktree>/anonimizador/tests/test_channels.py#L1001-L1014) (`tests/test_channels.py:1001-1014`).

- **A0-4: CONFIRMED**
  - **Fix (part 2) & Refutation (part 1):** [`INLINE_IMAGE_HEAD`](file://<review-worktree>/anonimizador/src/anonimizador/channels.py#L259) / [`INLINE_IMAGE_START`](file://<review-worktree>/anonimizador/src/anonimizador/channels.py#L276) (`src/anonimizador/channels.py:259,276`) and [`inline_image_end`](file://<review-worktree>/anonimizador/src/anonimizador/channels.py#L520-L538) (`src/anonimizador/channels.py:520-538`).
  - **Test (fix and refutation):** [`test_a_bi_in_an_image_with_no_id_after_it_starts_no_image`](file://<review-worktree>/anonimizador/tests/test_channels.py#L1017-L1026) (`tests/test_channels.py:1017-1026`).

- **A0-5: CONFIRMED**
  - **Fix:** [`Document.__init__`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L139-L147) and [`Document.definition`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L149-L160) (`src/anonimizador/pdfstreams.py:139-160`), [`unpredict`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L303-L304) (`src/anonimizador/pdfstreams.py:303-304`), and [`decode_stream`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L350-L378) (`src/anonimizador/pdfstreams.py:361-370`).
  - **Test:** [`test_object_definitions_filter_chains_and_predictors_are_read_as_a_reader_does`](file://<review-worktree>/anonimizador/tests/test_channels.py#L1029-L1058) (`tests/test_channels.py:1029-1058`).

- **A0-6: CONFIRMED**
  - **Refutation test:** [`test_a_file_named_directly_is_checked_whatever_its_suffix`](file://<review-worktree>/anonimizador/tests/test_cli.py#L85-L91) (`tests/test_cli.py:85-91`), verifying that a non-corpus file passed directly on the command line is checked and reported as `ERROR` (`unreadable: file: not a whole PNG or JPEG image`) with exit status `2`.

---

## Part 2: New Defects

None.

NO FINDINGS
