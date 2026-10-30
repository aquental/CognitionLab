"""Redaction by rasterizing (R5 fallback, R7).

Each page keeps only its rendered pixels, with a black box painted over every
detection. The output is rebuilt from those images alone, so no text layer, covered
text, metadata, incremental update, or embedded file of the original survives: what is
left is what the page shows, minus the boxes. A PDF becomes a scanned PDF; an image
file becomes a PNG.

The PDF is written here, not by Pillow: Pillow stores grayscale and color pages as JPEG,
whose artifacts around letters change what OCR reads (on a real exam, a page with no
box went from 1709 to 3158 OCR characters). Pages are stored losslessly (Flate), with
no information dictionary, and the same pages always give the same bytes.
"""

from __future__ import annotations

import io
import zlib
from collections.abc import Iterable
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw

from anonimizador.locate import Box, Page

# Pixels added on every side of a box, per 100 dpi: OCR boxes are tight, and accents
# and descenders reach past them.
PADDING_PER_100_DPI = 2
POINTS_PER_INCH = 72
COLOR_SPACES = {"L": b"/DeviceGray", "RGB": b"/DeviceRGB"}


def padded(box: Box, padding: int, size: tuple[int, int]) -> Box:
    left, top, right, bottom = box
    return (
        max(0, left - padding),
        max(0, top - padding),
        min(size[0], right + padding),
        min(size[1], bottom + padding),
    )


def gray_if_neutral(im: Image.Image) -> Image.Image:
    """A color image whose channels are all equal, as `pdftoppm` renders a black-and-white
    page, stored as gray: a third of the size, the same pixels."""
    if im.mode != "RGB":
        return im
    red, green, blue = im.split()
    neutral = ImageChops.difference(red, green).getbbox() is None
    return red if neutral and ImageChops.difference(red, blue).getbbox() is None else im


def paint(page: Page, boxes: Iterable[Box], dpi: int) -> Image.Image:
    """The page's pixels with every box painted black."""
    padding = max(1, dpi * PADDING_PER_100_DPI // 100)
    with Image.open(page.image) as im:
        mode = "L" if im.mode in ("1", "L") else "RGB"
        # Bare pixels: nothing of the rendered file comes along.
        pixels = Image.frombytes(mode, im.size, im.convert(mode).tobytes())
    draw = ImageDraw.Draw(pixels)
    for box in boxes:
        draw.rectangle(padded(box, padding, pixels.size), fill=0)
    return gray_if_neutral(pixels)


def pdf_bytes(images: list[Image.Image], dpi: int) -> bytes:
    """A PDF with one full-page image per page and nothing else: a catalog, a page tree,
    and per page its dictionary, content stream, and image."""
    count = len(images)
    kids = b" ".join(b"%d 0 R" % (3 + 3 * n) for n in range(count))
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [%s] /Count %d >>" % (kids, count),
    ]
    for n, im in enumerate(images):
        # Objects 3 + 3n (the page, as `kids` lists it), then its content and image.
        content, image = 4 + 3 * n, 5 + 3 * n
        width = im.size[0] * POINTS_PER_INCH / dpi
        height = im.size[1] * POINTS_PER_INCH / dpi
        draw = b"q %.4f 0 0 %.4f 0 0 cm /Im0 Do Q" % (width, height)
        objects.append(
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 %.4f %.4f] "
            b"/Resources << /XObject << /Im0 %d 0 R >> >> /Contents %d 0 R >>"
            % (width, height, image, content)
        )
        objects.append(b"<< /Length %d >>\nstream\n%s\nendstream" % (len(draw), draw))
        data = zlib.compress(im.tobytes())
        objects.append(
            b"<< /Type /XObject /Subtype /Image /Width %d /Height %d /ColorSpace %s "
            b"/BitsPerComponent 8 /Filter /FlateDecode /Length %d >>\nstream\n%s\nendstream"
            % (im.size[0], im.size[1], COLOR_SPACES[im.mode], len(data), data)
        )
    out = io.BytesIO()
    out.write(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for number, body in enumerate(objects, start=1):
        offsets.append(out.tell())
        out.write(b"%d 0 obj\n%s\nendobj\n" % (number, body))
    xref = out.tell()
    out.write(b"xref\n0 %d\n0000000000 65535 f \n" % (len(objects) + 1))
    out.writelines(b"%010d 00000 n \n" % offset for offset in offsets)
    out.write(
        b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objects) + 1, xref)
    )
    return out.getvalue()


def redact(pages: list[Page], boxes: dict[int, list[Box]], output: Path, dpi: int) -> None:
    """Writes the redacted copy, a PDF of the page images or a PNG for one image file,
    to `output`, which must not exist yet: an existing file, or a link to one, is never
    written through."""
    images = [paint(page, boxes.get(page.number, []), dpi) for page in pages]
    if output.suffix == ".pdf":
        data = pdf_bytes(images, dpi)
    else:
        buffer = io.BytesIO()
        images[0].save(buffer, "PNG")
        data = buffer.getvalue()
    with output.open("xb") as f:
        f.write(data)
