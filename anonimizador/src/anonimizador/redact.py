"""Redaction by rasterizing (R5 fallback, R7).

Each page keeps only its rendered pixels, with a black box painted over every
detection. The output is rebuilt from those images alone, so no text layer, covered
text, metadata, incremental update, or embedded file of the original survives: what is
left is what the page shows, minus the boxes. A PDF becomes a scanned PDF; an image
file becomes a PNG.
"""

from __future__ import annotations

import io
import time
from collections.abc import Iterable
from pathlib import Path

from PIL import Image, ImageDraw

from anonimizador.locate import Box, Page

# Pixels added on every side of a box, per 100 dpi: OCR boxes are tight, and accents
# and descenders reach past them.
PADDING_PER_100_DPI = 2
# The dates Pillow writes into a PDF: fixed, so a run's output does not record when it
# ran, and the same original gives the same bytes.
FIXED_DATE = time.gmtime(946_684_800)  # 2000-01-01T00:00:00Z


def padded(box: Box, padding: int, size: tuple[int, int]) -> Box:
    left, top, right, bottom = box
    return (
        max(0, left - padding),
        max(0, top - padding),
        min(size[0], right + padding),
        min(size[1], bottom + padding),
    )


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
    return pixels


def write_pdf(images: list[Image.Image], output: Path, dpi: int) -> None:
    """Saved through a buffer: given a file name, Pillow writes it as the PDF's title."""
    first, *rest = images
    buffer = io.BytesIO()
    first.save(
        buffer,
        "PDF",
        resolution=float(dpi),
        save_all=True,
        append_images=rest,
        creationDate=FIXED_DATE,
        modDate=FIXED_DATE,
    )
    output.write_bytes(buffer.getvalue())


def redact(pages: list[Page], boxes: dict[int, list[Box]], output: Path, dpi: int) -> None:
    """Writes the redacted copy: a PDF of the page images, or a PNG for one image file."""
    images = [paint(page, boxes.get(page.number, []), dpi) for page in pages]
    if output.suffix == ".pdf":
        write_pdf(images, output, dpi)
    else:
        images[0].save(output, "PNG")
