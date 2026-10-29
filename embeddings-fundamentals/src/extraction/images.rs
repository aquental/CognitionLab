//! Embedded PDF images (PLAN §0.1 "Embedded images").
//!
//! `pdfimages -list` is read first. Only rows of type `image` whose sides are both at
//! least [`ImageFilter::min_side_px`] become candidates; soft masks, stencils, and
//! small images (logos, icons) are dropped. The pipeline additionally drops every image
//! on a page that needs OCR: there the raster is the page scan, the OCR source, never an
//! image record. Kept images are then extracted one by one (`pdfimages -png -p`).

use std::future::Future;
use std::num::NonZeroU32;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::ExtractionError;

/// One row of `pdfimages -list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageInfo {
    /// 1-based page number.
    pub page: NonZeroU32,
    /// Image number in the whole document (`num` column).
    pub num: u32,
    /// Position of this row among the rows of its page (0-based, every type counted);
    /// `pdfimages -png -p -f N -l N` numbers its output files by this position.
    pub position_on_page: u32,
    /// `image`, `smask`, `stencil`, ...
    pub kind: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Which listed images may become image records. Its canonical JSON is part of the
/// image-preparation fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageFilter {
    /// Minimum width and height in pixels.
    pub min_side_px: u32,
}

impl Default for ImageFilter {
    fn default() -> Self {
        Self { min_side_px: 64 }
    }
}

impl ImageFilter {
    /// True for a real image (not a mask) with both sides at least the minimum.
    #[must_use]
    pub fn keeps(&self, info: &ImageInfo) -> bool {
        info.kind == "image" && info.width >= self.min_side_px && info.height >= self.min_side_px
    }
}

/// The mask that `pdfimages -list` lists right after `image`: poppler emits a soft mask
/// (`smask`, `/SMask`) or an explicit mask (`mask`, `/Mask` image) immediately after the
/// image it belongs to, on the same page. Hidden pixels of the image are only hidden
/// through it, so a kept image is prepared with its mask (C3-6).
#[must_use]
pub fn mask_of<'a>(listed: &'a [ImageInfo], image: &ImageInfo) -> Option<&'a ImageInfo> {
    let next = image.position_on_page.checked_add(1)?;
    listed
        .iter()
        .find(|m| m.page == image.page && m.position_on_page == next)
        .filter(|m| matches!(m.kind.as_str(), "smask" | "mask"))
}

fn parse_error(reason: impl Into<String>) -> ExtractionError {
    ExtractionError::Parse {
        what: "pdfimages -list output",
        reason: reason.into(),
    }
}

/// Parses `pdfimages -list` output: two header lines, then one row per image with
/// `page num type width height ...` as the first five columns.
///
/// # Errors
///
/// Returns [`ExtractionError::Parse`] when the header is missing or a row is malformed.
pub fn parse_list(output: &str) -> Result<Vec<ImageInfo>, ExtractionError> {
    let mut lines = output.lines();
    let header = lines.next().ok_or_else(|| parse_error("empty output"))?;
    if !header
        .split_whitespace()
        .take(5)
        .eq(["page", "num", "type", "width", "height"])
    {
        return Err(parse_error("unexpected header"));
    }
    if !lines.next().is_some_and(|l| l.starts_with("---")) {
        return Err(parse_error("missing header rule"));
    }
    let mut images: Vec<ImageInfo> = Vec::new();
    for line in lines.filter(|l| !l.trim().is_empty()) {
        let cells: Vec<&str> = line.split_whitespace().collect();
        let [page, num, kind, width, height, ..] = cells.as_slice() else {
            return Err(parse_error("short row"));
        };
        let number = |cell: &str, what: &str| {
            cell.parse::<u32>()
                .map_err(|_| parse_error(format!("{what} is not a number")))
        };
        let page = NonZeroU32::new(number(page, "page")?).ok_or_else(|| parse_error("page 0"))?;
        let position_on_page = u32::try_from(images.iter().filter(|i| i.page == page).count())
            .map_err(|_| parse_error("too many images on one page"))?;
        images.push(ImageInfo {
            page,
            num: number(num, "num")?,
            position_on_page,
            kind: (*kind).to_owned(),
            width: number(width, "width")?,
            height: number(height, "height")?,
        });
    }
    Ok(images)
}

/// Lists and extracts the images embedded in a PDF.
pub trait ImageExtractor: Send + Sync {
    /// Tool identity and version, included in the extraction fingerprint of image runs.
    fn engine_id(&self) -> String;

    /// Every image of the PDF, in `pdfimages -list` order.
    fn list_images(
        &self,
        pdf: &Path,
    ) -> impl Future<Output = Result<Vec<ImageInfo>, ExtractionError>> + Send;

    /// The encoded bytes (PNG) of one listed image.
    fn extract_image(
        &self,
        pdf: &Path,
        image: &ImageInfo,
    ) -> impl Future<Output = Result<Vec<u8>, ExtractionError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIXED: &str = "\
page   num  type   width height color comp bpc  enc interp  object ID x-ppi y-ppi size ratio
--------------------------------------------------------------------------------------------
   1     0 image     256   192  rgb     3   8  image  no         3  0    77    77 29.4K  20%
   1     1 image      48    48  rgb     3   8  image  no         5  0    72    72   61B 0.9%
   1     2 smask      48    48  gray    1   8  image  no         5  0    72    72   89B 3.9%
   2     3 image    1240  1755  rgb     3   8  image  no         8  0   150   150 52.3K 0.8%
";

    #[test]
    fn a_mask_belongs_to_the_image_listed_right_before_it() {
        let masked = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pdf-masks/images.list"),
        )
        .expect("captured pdfimages list");
        let listed = parse_list(&masked).expect("list");
        let mask =
            |position: usize| mask_of(&listed, &listed[position]).map(|m| (m.num, m.kind.as_str()));
        assert_eq!(mask(0), Some((1, "smask")));
        assert_eq!(mask(2), Some((3, "mask")));
        assert_eq!(mask(1), None);
        assert_eq!(mask(3), None);
        // An image followed by another image, or last on its page, has no mask.
        let mixed = parse_list(MIXED).expect("list");
        assert_eq!(mask_of(&mixed, &mixed[0]), None);
        assert_eq!(mask_of(&mixed, &mixed[1]).map(|m| m.num), Some(2));
        assert_eq!(mask_of(&mixed, &mixed[3]), None);
    }

    #[test]
    fn the_list_is_parsed_with_per_page_positions() {
        let images = parse_list(MIXED).expect("parse");
        let rows: Vec<(u32, u32, u32, &str)> = images
            .iter()
            .map(|i| (i.page.get(), i.num, i.position_on_page, i.kind.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                (1, 0, 0, "image"),
                (1, 1, 1, "image"),
                (1, 2, 2, "smask"),
                (2, 3, 0, "image")
            ]
        );
        assert_eq!((images[0].width, images[0].height), (256, 192));
    }

    #[test]
    fn masks_and_small_images_are_dropped() {
        let images = parse_list(MIXED).expect("parse");
        let filter = ImageFilter::default();
        let kept: Vec<u32> = images
            .iter()
            .filter(|i| filter.keeps(i))
            .map(|i| i.num)
            .collect();
        // The 48 px logo and its soft mask are dropped; the page-2 raster passes the list
        // filter (the pipeline drops it because page 2 needs OCR).
        assert_eq!(kept, [0, 3]);
        let edge = ImageInfo {
            width: 64,
            height: 63,
            ..images[0].clone()
        };
        assert!(!filter.keeps(&edge), "both sides must reach the minimum");
    }

    #[test]
    fn an_empty_list_has_no_images_and_garbage_is_refused() {
        let header = MIXED.lines().take(2).collect::<Vec<_>>().join("\n");
        assert_eq!(parse_list(&header).expect("parse"), Vec::new());
        assert!(parse_list("").is_err());
        assert!(parse_list("not pdfimages output\n---\n").is_err());
        let bad = format!("{header}\n   x     0 image 1 1\n");
        assert!(parse_list(&bad).is_err());
    }
}
