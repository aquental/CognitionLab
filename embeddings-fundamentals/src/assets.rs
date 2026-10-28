//! Image asset preparation (PLAN Phase 3 step 2, PRD §8).
//!
//! Every image that becomes a record is decoded and re-encoded as an RGB8 PNG before it
//! is stored or embedded. Re-encoding from decoded pixels drops every metadata block of
//! the input (EXIF, XMP, IPTC, ICC, PNG text chunks) by construction: the encoder only
//! writes the image header, the pixel data, and the end marker. The stored asset is
//! content-addressed by the SHA-256 of those bytes.

use std::io::Cursor;

use image::{ImageFormat, ImageReader};

use crate::ids::Checksum;

/// Identity of the preparation step; part of the image-preparation fingerprint.
pub const SCRUBBER_ID: &str = "decode-rgb8-png-v1";

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
        let reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| AssetError::Decode(e.to_string()))?;
        match reader.format() {
            Some(ImageFormat::Png | ImageFormat::Jpeg) => {}
            other => {
                return Err(AssetError::Decode(format!(
                    "unsupported format {other:?} (PNG or JPEG expected)"
                )));
            }
        }
        let decoded = reader
            .decode()
            .map_err(|e| AssetError::Decode(e.to_string()))?;
        let rgb = decoded.to_rgb8();
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
