//! Image asset preparation (PLAN Phase 3 step 2, PRD §8).
//!
//! Every image that becomes a record is decoded and re-encoded as an RGB8 PNG before it
//! is stored or embedded. Re-encoding from decoded pixels drops every metadata block of
//! the input (EXIF, XMP, IPTC, ICC, PNG text chunks) by construction: the encoder only
//! writes the image header, the pixel data, and the end marker. The stored asset is
//! content-addressed by the SHA-256 of those bytes.
//!
//! Three pieces of information change what the image shows, so they are applied to the
//! pixels before they are dropped: a PDF image's mask (extracted by `pdfimages` as a
//! separate gray image; it becomes the alpha channel), the EXIF orientation (the asset is
//! stored upright, as a viewer displays the original), and transparency (the image is
//! composited over opaque white, so pixels hidden under alpha 0 never become visible).

use std::io::Cursor;

use image::imageops::FilterType;
use image::{DynamicImage, GrayImage, ImageDecoder, ImageFormat, ImageReader, RgbImage};

use crate::ids::Checksum;

/// Identity of the preparation step; part of the image-preparation fingerprint.
pub const SCRUBBER_ID: &str = "decode-mask-orient-flatten-white-rgb8-png-v3";

/// A decoded and re-encoded image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedImage {
    /// PNG bytes without metadata.
    pub png: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// SHA-256 of `png`.
    pub checksum: Checksum,
}

/// Errors raised while preparing an image.
#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    /// The input is not a decodable PNG or JPEG.
    #[error("cannot decode image: {0}")]
    Decode(String),
    /// The image has no pixels.
    #[error("image has no pixels")]
    Empty,
    /// The re-encoded PNG could not be written.
    #[error("cannot encode image: {0}")]
    Encode(String),
}

/// Re-encodes images without metadata (synchronous, CPU-bound: call it on the blocking
/// pool).
#[derive(Debug, Clone, Copy, Default)]
pub struct MetadataScrubber;

impl MetadataScrubber {
    /// Identity of this preparation step.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        SCRUBBER_ID
    }

    /// Decodes a PNG or JPEG and re-encodes its pixels as an RGB8 PNG.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when the input cannot be decoded, has no pixels, or the
    /// output cannot be encoded.
    pub fn scrub(&self, bytes: &[u8]) -> Result<PreparedImage, AssetError> {
        self.scrub_masked(bytes, None)
    }

    /// Like [`Self::scrub`], with the image's PDF mask as `pdfimages` writes it: a gray
    /// image whose value is the opacity (soft mask: the alpha; explicit mask: 0 hidden,
    /// 255 painted), scaled to the image size as a PDF viewer does. Masked-out pixels are
    /// therefore composited away instead of being revealed.
    ///
    /// # Errors
    ///
    /// As [`Self::scrub`]; a mask that cannot be decoded is a [`AssetError::Decode`].
    pub fn scrub_masked(
        &self,
        bytes: &[u8],
        mask: Option<&[u8]>,
    ) -> Result<PreparedImage, AssetError> {
        let reader = reader_of(bytes)?;
        let decode = |e: image::ImageError| AssetError::Decode(e.to_string());
        let mut decoder = reader.into_decoder().map_err(decode)?;
        let orientation = decoder.orientation().map_err(decode)?;
        let mut decoded = DynamicImage::from_decoder(decoder).map_err(decode)?;
        if let Some(mask) = mask {
            // The mask belongs to the stored raster, before any display orientation.
            decoded = with_mask(&decoded, &opacity_of(mask)?);
        }
        decoded.apply_orientation(orientation);
        let rgb = if decoded.color().has_alpha() {
            flatten_on_white(&decoded)
        } else {
            decoded.to_rgb8()
        };
        let (width, height) = rgb.dimensions();
        if width == 0 || height == 0 {
            return Err(AssetError::Empty);
        }
        let mut png = Vec::new();
        rgb.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .map_err(|e| AssetError::Encode(e.to_string()))?;
        Ok(PreparedImage {
            checksum: Checksum::of_bytes(&png),
            png,
            width,
            height,
        })
    }
}

fn reader_of(bytes: &[u8]) -> Result<ImageReader<Cursor<&[u8]>>, AssetError> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| AssetError::Decode(e.to_string()))?;
    match reader.format() {
        Some(ImageFormat::Png | ImageFormat::Jpeg) => Ok(reader),
        other => Err(AssetError::Decode(format!(
            "unsupported format {other:?} (PNG or JPEG expected)"
        ))),
    }
}

/// Decodes a mask as 8-bit opacity (a 1-bit mask expands to 0 and 255).
fn opacity_of(mask: &[u8]) -> Result<GrayImage, AssetError> {
    let decoded = reader_of(mask)?
        .decode()
        .map_err(|e| AssetError::Decode(format!("mask: {e}")))?;
    Ok(decoded.to_luma8())
}

/// The image with `opacity` (scaled to its size) multiplied into its alpha channel.
fn with_mask(image: &DynamicImage, opacity: &GrayImage) -> DynamicImage {
    let mut rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let scaled;
    let opacity = if opacity.dimensions() == (width, height) {
        opacity
    } else {
        scaled = image::imageops::resize(opacity, width, height, FilterType::Triangle);
        &scaled
    };
    for (pixel, mask) in rgba.pixels_mut().zip(opacity.pixels()) {
        let alpha = (u16::from(pixel.0[3]) * u16::from(mask.0[0]) + 127) / 255;
        pixel.0[3] = u8::try_from(alpha).unwrap_or(u8::MAX);
    }
    DynamicImage::ImageRgba8(rgba)
}

/// Composites an image with alpha over opaque white (8-bit, rounded).
fn flatten_on_white(image: &DynamicImage) -> RgbImage {
    let rgba = image.to_rgba8();
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, a] = rgba.get_pixel(x, y).0;
        let (a, white) = (u16::from(a), 255 - u16::from(a));
        let blend = |c: u8| {
            let mixed = (u16::from(c) * a + 255 * white + 127) / 255;
            u8::try_from(mixed).unwrap_or(u8::MAX)
        };
        image::Rgb([blend(r), blend(g), blend(b)])
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::anonymization::image_findings;

    fn corpus(file: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/corpus")
            .join(file)
    }

    /// Chunk types of a PNG, in order.
    fn png_chunks(png: &[u8]) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut at = 8;
        while let Some(header) = png.get(at..at + 8) {
            let len = u32::from_be_bytes(header[..4].try_into().expect("len"));
            chunks.push(String::from_utf8_lossy(&header[4..8]).into_owned());
            at += 12 + usize::try_from(len).expect("len");
        }
        chunks
    }

    fn findings_of(png: &[u8]) -> Vec<crate::anonymization::Finding> {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("out.png");
        std::fs::write(&path, png).expect("write");
        image_findings(&path)
    }

    #[test]
    fn jpeg_metadata_blocks_do_not_survive() {
        let original = std::fs::read(corpus("abd-us-001.jpg")).expect("jpeg");
        let segment = |marker: u8, payload: &[u8]| -> Vec<u8> {
            let len = u16::try_from(payload.len() + 2).expect("len").to_be_bytes();
            [&[0xFF, marker][..], &len, payload].concat()
        };
        let xmp = segment(
            0xE1,
            b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta><dc:creator>Fulano</dc:creator></x:xmpmeta>",
        );
        let iptc = segment(
            0xED,
            b"Photoshop 3.0\08BIM\x04\x04\0\0\0\0\0\x0b\x1c\x02\x50\x00\x06Fulano",
        );
        let tagged = [&original[..2], &xmp, &iptc, &original[2..]].concat();
        let dir = tempfile::tempdir().expect("tmp");
        let input = dir.path().join("tagged.jpg");
        std::fs::write(&input, &tagged).expect("write");
        assert!(
            !image_findings(&input).is_empty(),
            "the input carries metadata"
        );

        let prepared = MetadataScrubber.scrub(&tagged).expect("scrub");
        assert_eq!(png_chunks(&prepared.png), ["IHDR", "IDAT", "IEND"]);
        assert_eq!(findings_of(&prepared.png), vec![]);
        assert!(
            !prepared.png.windows(6).any(|w| w == b"Fulano"),
            "no metadata value survives"
        );
        let plain = MetadataScrubber.scrub(&original).expect("scrub");
        assert_eq!(
            plain, prepared,
            "pixels are unchanged by the metadata blocks"
        );
    }

    #[test]
    fn png_text_chunks_do_not_survive_and_pixels_are_kept() {
        let original = std::fs::read(corpus("chest-001.png")).expect("png");
        let data = b"Author\0Fulano";
        let mut chunk = u32::try_from(data.len())
            .expect("len")
            .to_be_bytes()
            .to_vec();
        chunk.extend_from_slice(b"tEXt");
        chunk.extend_from_slice(data);
        let crc = crc32(&chunk[4..]);
        chunk.extend_from_slice(&crc.to_be_bytes());
        let tagged = [&original[..33], &chunk, &original[33..]].concat();
        let prepared = MetadataScrubber.scrub(&tagged).expect("scrub");
        assert_eq!(png_chunks(&prepared.png), ["IHDR", "IDAT", "IEND"]);
        assert_eq!(findings_of(&prepared.png), vec![]);
        let pixels = |png: &[u8]| {
            image::load_from_memory(png)
                .expect("decode")
                .to_rgb8()
                .into_raw()
        };
        assert_eq!(pixels(&prepared.png), pixels(&original));
        assert_eq!(prepared.checksum, Checksum::of_bytes(&prepared.png));
    }

    fn decoded(png: &[u8]) -> RgbImage {
        image::load_from_memory(png).expect("decode").to_rgb8()
    }

    #[test]
    fn transparent_pixels_are_composited_over_white_not_revealed() {
        // Regression (C3-2): a red pixel under alpha 0 must not become visible.
        let rgba = image::RgbaImage::from_raw(
            4,
            1,
            vec![
                255, 0, 0, 0, // hidden red
                0, 0, 255, 255, // opaque blue
                0, 0, 0, 128, // half-transparent black
                0, 255, 0, 0, // hidden green
            ],
        )
        .expect("rgba");
        let mut png = Vec::new();
        DynamicImage::ImageRgba8(rgba)
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .expect("encode");
        let prepared = MetadataScrubber.scrub(&png).expect("scrub");
        let pixels: Vec<[u8; 3]> = decoded(&prepared.png).pixels().map(|p| p.0).collect();
        // 0 * 128/255 + 255 * 127/255 = 127.
        assert_eq!(
            pixels,
            [
                [255, 255, 255],
                [0, 0, 255],
                [127, 127, 127],
                [255, 255, 255]
            ]
        );
    }

    /// A JPEG of `width`×`height`, left half red and right half blue, with an EXIF
    /// orientation tag inserted after the SOI marker.
    fn oriented_jpeg(width: u32, height: u32, orientation: u16) -> Vec<u8> {
        let image = RgbImage::from_fn(width, height, |x, _| {
            if x < width / 2 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 0, 255])
            }
        });
        let mut jpeg = Vec::new();
        DynamicImage::ImageRgb8(image)
            .write_to(&mut Cursor::new(&mut jpeg), ImageFormat::Jpeg)
            .expect("encode");
        // TIFF little endian, one IFD entry: Orientation (0x0112), SHORT, count 1.
        let mut exif = b"Exif\0\0II*\0".to_vec();
        exif.extend_from_slice(&8_u32.to_le_bytes());
        exif.extend_from_slice(&1_u16.to_le_bytes());
        exif.extend_from_slice(&0x0112_u16.to_le_bytes());
        exif.extend_from_slice(&3_u16.to_le_bytes());
        exif.extend_from_slice(&1_u32.to_le_bytes());
        exif.extend_from_slice(&orientation.to_le_bytes());
        exif.extend_from_slice(&[0, 0]);
        exif.extend_from_slice(&0_u32.to_le_bytes());
        let len = u16::try_from(exif.len() + 2).expect("len").to_be_bytes();
        [&jpeg[..2], &[0xFF, 0xE1], &len, &exif, &jpeg[2..]].concat()
    }

    fn is_red(p: image::Rgb<u8>) -> bool {
        p.0[0] > 200 && p.0[2] < 60
    }

    fn is_blue(p: image::Rgb<u8>) -> bool {
        p.0[2] > 200 && p.0[0] < 60
    }

    #[test]
    fn the_exif_orientation_is_applied_before_it_is_dropped() {
        // Regression (C3-3): stored upright, as an orientation-aware viewer shows it.
        // Orientation 6 (rotate 90° clockwise): the left (red) column becomes the top.
        let rotated = MetadataScrubber
            .scrub(&oriented_jpeg(32, 16, 6))
            .expect("scrub");
        assert_eq!((rotated.width, rotated.height), (16, 32));
        let pixels = decoded(&rotated.png);
        assert!(
            is_red(*pixels.get_pixel(8, 4)),
            "{:?}",
            pixels.get_pixel(8, 4)
        );
        assert!(
            is_blue(*pixels.get_pixel(8, 28)),
            "{:?}",
            pixels.get_pixel(8, 28)
        );
        // Orientation 2 (mirror horizontally): blue on the left, red on the right.
        let mirrored = MetadataScrubber
            .scrub(&oriented_jpeg(32, 16, 2))
            .expect("scrub");
        assert_eq!((mirrored.width, mirrored.height), (32, 16));
        let pixels = decoded(&mirrored.png);
        assert!(
            is_blue(*pixels.get_pixel(4, 8)),
            "{:?}",
            pixels.get_pixel(4, 8)
        );
        assert!(
            is_red(*pixels.get_pixel(28, 8)),
            "{:?}",
            pixels.get_pixel(28, 8)
        );
        // Orientation 1 keeps the raster; the tag never survives.
        let upright = MetadataScrubber
            .scrub(&oriented_jpeg(32, 16, 1))
            .expect("scrub");
        assert!(is_red(*decoded(&upright.png).get_pixel(4, 8)));
        assert_eq!(png_chunks(&rotated.png), ["IHDR", "IDAT", "IEND"]);
    }

    fn pdf_mask_fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/pdf-masks")
                .join(name),
        )
        .expect("captured pdfimages output")
    }

    /// Regression (C3-6): the captured `pdfimages` output of `masked-images.pdf` (red
    /// left halves hidden by a 48×48 soft mask and by a 1-bit explicit mask). Prepared
    /// with their masks, the hidden halves are white and the visible halves stay blue;
    /// without them the hidden red would be revealed.
    #[test]
    fn pdf_masks_hide_what_the_viewer_hides() {
        let red_left = pdf_mask_fixture("img-001-000.png");
        let unmasked = decoded(&MetadataScrubber.scrub(&red_left).expect("scrub").png);
        assert!(
            is_red(*unmasked.get_pixel(10, 48)),
            "fixture shows red unmasked"
        );
        for (image, mask) in [
            ("img-001-000.png", "img-001-001.png"),
            ("img-001-002.png", "img-001-003.png"),
        ] {
            let prepared = MetadataScrubber
                .scrub_masked(&pdf_mask_fixture(image), Some(&pdf_mask_fixture(mask)))
                .expect("scrub");
            assert_eq!((prepared.width, prepared.height), (96, 96));
            let rgb = decoded(&prepared.png);
            for y in [0, 48, 95] {
                for x in [0, 20, 45] {
                    assert_eq!(
                        rgb.get_pixel(x, y).0,
                        [255, 255, 255],
                        "{mask} at ({x},{y})"
                    );
                }
                for x in [50, 75, 95] {
                    assert!(is_blue(*rgb.get_pixel(x, y)), "{mask} at ({x},{y})");
                }
            }
            assert!(
                !rgb.pixels().any(|p| p.0[0] > 128 && p.0[1] < 128),
                "{mask}: a hidden red pixel was revealed"
            );
        }
    }

    #[test]
    fn non_images_are_refused() {
        assert!(matches!(
            MetadataScrubber.scrub(b"%PDF-1.5"),
            Err(AssetError::Decode(_))
        ));
        assert!(matches!(
            MetadataScrubber.scrub(&[0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0]),
            Err(AssetError::Decode(_))
        ));
    }

    /// CRC-32 (IEEE) of a PNG chunk type and data, so the tagged input stays valid.
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFF_u32;
        for &b in bytes {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
}
