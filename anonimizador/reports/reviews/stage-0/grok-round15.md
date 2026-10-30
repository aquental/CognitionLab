I'll review the current stage against G0-44, G0-45, and the carried-over verifier fixes, then look for any remaining miss, crash, wrong exit, or leak.The drawn-text and join logic is where G0-44 and G0-45 landed. I'll read those tests and the round 14 report next, then check the layouts they still miss.G0-44 and G0-45 are in the drawn-text path. I'll check those layouts against the current matcher and look for cases that still drop a name or a number.G0-44 and G0-45 are fixed. G4-5 and the verifier part of G4-6 still hold. Two misses remain.

**G0-44 (confirmed).** `drawn_spans` keeps a string only when it is outside a comment and outside a dictionary (`channels.py:299`). The three reported streams, the hex comment, the nested dictionary, a 6-letter word, and an octal or hex name that exists only in a comment all count 1 for the planted name and 0 for a different final letter. A comment with no string, and `/MCID 1` with no string, stay 1.

**G0-45 (confirmed).** A join keeps one space at a boundary (`join_starts`, `MIXED_HEAD` at `channels.py:323` and `channels.py:381`). The reported phone and CPF streams, the space-only piece, and the kerned CPF each hint once. A space on both sides of the move, and two numbers with a word between them, hint nothing.

**G4-5 (confirmed).** A non-zero exit, a timeout, or a tool that cannot start raises `ToolError` with the tool name only. The file is `ERROR` and the run exits 2.

**G4-6 (confirmed).** A file found under a directory is named by its relative path, directories included, and that path is the `name` channel.

## G0-46

**Severity:** high

**File:line:** `src/anonimizador/channels.py:306` (`drawn_spans`)

**Description:** Every `<<` raises a dictionary depth, and every later string is left out of the drawn, split, and joined readings until a `>>` brings the depth back to 0 or a `stream` keyword resets it. Bytes inside an inline image are not a dictionary. An image whose samples contain `<<` without a later `>>` leaves the depth above 0 for the rest of that stream, so a short name drawn in more than one string after the image is in none of those readings. Each piece is still its own line, and a term under 6 letters must be a whole word, so the lines do not save it. In 200 random 256-byte prefixes before `(Zu) Tj (leo) Tj`, 59 left the name at 0, and in every one of those the strings had been dropped because the depth never returned to 0.

**Impact:** `pdftotext -layout` prints the name and the verifier counts 0. On a live page the file is `ok` and the run exits 0. A name that is one whole string still counts 1. A word of 6 letters or more, drawn in pieces, still counts 1 through its compact form. A listed number still matches by its digits. An unlisted CPF drawn group by group gives no hint: the same groups hint `cpf=1` with no image and nothing with this image. Image bytes in which every `<<` is closed by a later `>>` leave the name at 1.

**Reproduction:** This page content counts `Zuleo` 0 and `Zulea` 0. `pdftotext -layout` prints `Hemoglobina 13,5 g/dL` and `Zuleo`.

```text
BT /F1 12 Tf 72 740 Td (Hemoglobina 13,5 g/dL) Tj ET
BI /W 8 /H 1 /CS /G /BPC 8 ID \x10\x20<<\x30\x40\x50\x60 EI
BT /F1 12 Tf 72 700 Td (Zu) Tj (leo) Tj ET
```

The same miss holds for `[(Z) -2 (u) -2 (l) -2 (e) -2 (o)] TJ` after those samples. Replacing the samples with `\x10\x20<<>>\x50\x60` brings the count back to 1.

**Validation suggestion:** Both streams must count 1 for `Zuleo` and 0 for `Zulea`, including a Flate page stream. The grouped CPF above must hint `cpf=1` again. A 6-letter word in pieces, a name in one string, and the G0-44 comment and dictionary streams must stay 1 when the same image samples precede them. Skipping every `<<` brings the G0-44 dictionary stream back to 0.

## G0-47

**Severity:** medium

**File:line:** `src/anonimizador/channels.py:195` (`PDF_LITERAL`)

**Description:** A literal is read one parenthesis level deep. When the outer literal also contains a parenthesis nested one level further, the match starts at the inner string and the escaped text in the outer string is never decoded. The octal bytes are not the letters of the name, so the raw byte views do not contain it either.

**Impact:** `pdftotext -layout` prints `Zuléo (obs (2))` and the verifier counts `Zuléo` 0. If that string is the only copy of the name, the file is `ok` and the run exits 0. These still count 1: `(Zul\351o (obs))`, `(obs (2) Zul\351o)`, `((Zul\351o))`, `(Zul\351o (a) (b))`, and `(A (B (Zul\351o)))`, where the escaped name sits inside the one-level match. An ASCII name in the outer string still counts 1, because those letters are in the raw bytes.

**Reproduction:** `BT /F1 12 Tf 72 700 Td (Zul\351o (obs (2))) Tj ET` in a one-page PDF. `pdftotext -layout` prints `Zuléo (obs (2))`. `raw_readings` of that file counts `Zuléo` 0.

**Validation suggestion:** That stream must count 1 for `Zuléo` and 0 for a different final letter, and the one-level forms above must stay 1. A scan that accepts any depth, and still finishes a long binary stream quickly, keeps a third level from reopening the hole.
