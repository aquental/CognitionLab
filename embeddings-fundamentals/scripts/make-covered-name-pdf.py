#!/usr/bin/env python3
"""Writes the synthetic PDF that the anonymization verifier tests use (TESTS.md).

One page of invented content that shows the classic failed redaction:

- the name "Fulano Ficticio" is in the text layer, but a black box drawn over it hides
  it from view, so text extraction finds it and the rendered page does not;
- the same name is in the document information dictionary (/Author);
- an invented CPF ("123.456.789-09") is visible on the page.

No real person or medical record is involved.

    python3 scripts/make-covered-name-pdf.py tests/fixtures/anonymization/covered-name.pdf
"""
import sys

# Helvetica 12 pt: "Paciente: " is 53.4 pt wide and "Fulano Ficticio" 77.4 pt, so the
# name spans x = 125.4..202.8 on the baseline y = 730.
CONTENT = b"""BT /F1 14 Tf 72 770 Td (LAUDO SINTETICO DE TESTE) Tj ET
BT /F1 12 Tf 72 730 Td (Paciente: Fulano Ficticio) Tj ET
0 0 0 rg 122 724 86 18 re f
BT /F1 12 Tf 72 700 Td (CPF: 123.456.789-09) Tj ET
BT /F1 12 Tf 72 670 Td (Hemoglobina 13,5 g/dL) Tj ET
"""


def main():
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842]"
        b" /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        b"<< /Length %d >>\nstream\n" % len(CONTENT) + CONTENT + b"endstream",
        b"<< /Title (Laudo sintetico) /Author (Fulano Ficticio)"
        b" /Producer (make-covered-name-pdf.py) >>",
    ]
    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for number, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % number + body + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objects) + 1)
    for offset in offsets:
        out += b"%010d 00000 n \n" % offset
    out += b"trailer\n<< /Size %d /Root 1 0 R /Info 6 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (
        len(objects) + 1,
        xref,
    )
    with open(sys.argv[1], "wb") as f:
        f.write(out)


if __name__ == "__main__":
    main()
