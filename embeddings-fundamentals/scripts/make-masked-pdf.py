#!/usr/bin/env python3
"""Writes the synthetic masked-image PDF used by the C3-6 regression tests.

One page with a usable native text layer and two 96x96 RGB images, red on the left half
and blue on the right half:

- Im1 has a soft mask (/SMask, 48x48 DeviceGray): alpha 0 on the left half, 255 on the
  right half.
- Im2 has an explicit mask (/Mask, 96x96 1-bit image mask): sample 1 (masked out) on
  the left half.

A viewer shows only the blue halves; the red halves are hidden. No medical content.

    python3 scripts/make-masked-pdf.py tests/fixtures/pdf-masks/masked-images.pdf
    cd tests/fixtures/pdf-masks && pdfimages -list masked-images.pdf > images.list \
        && pdfimages -png -p -f 1 -l 1 masked-images.pdf img
"""
import sys

SIDE = 96
SOFT_MASK_SIDE = 48


def rgb():
    red, blue = [255, 0, 0], [0, 0, 255]
    return bytes(
        c for _ in range(SIDE) for x in range(SIDE) for c in (red if x < SIDE // 2 else blue)
    )


def soft_mask(side):
    return bytes(0 if x < side // 2 else 255 for _ in range(side) for x in range(side))


def explicit_mask(side):
    rows = bytearray()
    for _ in range(side):
        row = bytearray((side + 7) // 8)
        for x in range(side // 2):
            row[x // 8] |= 0x80 >> (x % 8)
        rows += row
    return bytes(rows)


def stream(dictionary, data):
    return dictionary + b"\n/Length %d >>\nstream\n" % len(data) + data + b"\nendstream"


def main():
    # Enough text (>= 200 visible characters) for the native text layer to be usable,
    # so an image run keeps the page's images.
    lines = [
        b"Laudo sintetico de ultrassonografia com camada de texto nativa usavel.",
        b"Este documento ficticio existe apenas para testar mascaras de imagem em PDF.",
        b"A metade esquerda de cada imagem e vermelha e fica oculta pela mascara.",
        b"A metade direita de cada imagem e azul e permanece visivel ao leitor.",
    ]
    text = b"".join(b"(%s) Tj T*\n" % line for line in lines)
    content = (
        b"BT /F1 11 Tf 14 TL 20 760 Td\n" + text + b"ET\n"
        b"q 200 0 0 200 50 450 cm /Im1 Do Q\nq 200 0 0 200 300 450 cm /Im2 Do Q\n"
    )
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font "
        b"<< /F1 4 0 R >> /XObject << /Im1 6 0 R /Im2 8 0 R >> >> /Contents 5 0 R >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica",
        stream(b"<<", content),
        stream(
            b"<< /Type /XObject /Subtype /Image /Width 96 /Height 96 /ColorSpace /DeviceRGB "
            b"/BitsPerComponent 8 /SMask 7 0 R",
            rgb(),
        ),
        stream(
            b"<< /Type /XObject /Subtype /Image /Width %d /Height %d /ColorSpace /DeviceGray "
            b"/BitsPerComponent 8" % (SOFT_MASK_SIDE, SOFT_MASK_SIDE),
            soft_mask(SOFT_MASK_SIDE),
        ),
        stream(
            b"<< /Type /XObject /Subtype /Image /Width 96 /Height 96 /ColorSpace /DeviceRGB "
            b"/BitsPerComponent 8 /Mask 9 0 R",
            rgb(),
        ),
        stream(
            b"<< /Type /XObject /Subtype /Image /Width 96 /Height 96 /ImageMask true "
            b"/BitsPerComponent 1",
            explicit_mask(SIDE),
        ),
    ]
    objects[3] += b" >>"
    out = bytearray(b"%PDF-1.7\n")
    offsets = []
    for number, body in enumerate(objects, 1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % number + body + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objects) + 1)
    out += b"".join(b"%010d 00000 n \n" % o for o in offsets)
    out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (
        len(objects) + 1,
        xref,
    )
    with open(sys.argv[1], "wb") as f:
        f.write(out)


if __name__ == "__main__":
    main()
