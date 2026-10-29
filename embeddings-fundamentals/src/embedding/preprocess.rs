//! Image preprocessing of cross-modal models, read from the model's pinned
//! `preprocessor_config.json` (PLAN Phase 3; `reports/spikes/phase3-models.md`).
//!
//! EmbedAnything 0.7.1 resizes every image to 224 px and maps pixels to `[-1, 1]`,
//! whatever the model expects. The vision adapter therefore builds its own pixel
//! tensors: resize (shortest edge or exact size), optional center crop, rescale, and
//! per-channel normalization, as the Hugging Face image processors define them. The
//! canonical description of these steps is part of the embedding-space identity.

use image::imageops::FilterType;
use serde::Deserialize;

/// How the image is resized before cropping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resize {
    /// Scale so the shorter side has this length, keeping the aspect ratio.
    ShortestEdge(u32),
    /// Scale to exactly this size, ignoring the aspect ratio.
    Exact {
        /// Output height.
        height: u32,
        /// Output width.
        width: u32,
    },
}

/// Resampling filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resample {
    /// PIL `BILINEAR` (2).
    Bilinear,
    /// PIL `BICUBIC` (3), as Catmull-Rom.
    Bicubic,
}

impl Resample {
    const fn filter(self) -> FilterType {
        match self {
            Self::Bilinear => FilterType::Triangle,
            Self::Bicubic => FilterType::CatmullRom,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Bilinear => "bilinear",
            Self::Bicubic => "bicubic",
        }
    }
}

/// The preprocessing of one model.
#[derive(Debug, Clone, PartialEq)]
pub struct ImagePreprocessing {
    /// Resize step.
    pub resize: Resize,
    /// Center crop `(height, width)`, when the model crops.
    pub crop: Option<(u32, u32)>,
    /// Filter used by the resize.
    pub resample: Resample,
    /// Factor applied to 0..=255 pixel values before normalization.
    pub rescale: f32,
    /// Per-channel mean (RGB).
    pub mean: [f32; 3],
    /// Per-channel standard deviation (RGB).
    pub std: [f32; 3],
}

/// A size given as one number or as a map.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SizeSpec {
    Square(u32),
    Map {
        shortest_edge: Option<u32>,
        height: Option<u32>,
        width: Option<u32>,
    },
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    size: Option<SizeSpec>,
    crop_size: Option<SizeSpec>,
    #[serde(default)]
    do_center_crop: bool,
    #[serde(default = "yes")]
    do_resize: bool,
    #[serde(default = "yes")]
    do_rescale: bool,
    rescale_factor: Option<f32>,
    #[serde(default = "yes")]
    do_normalize: bool,
    image_mean: Option<[f32; 3]>,
    image_std: Option<[f32; 3]>,
    resample: Option<u8>,
}

const fn yes() -> bool {
    true
}

/// A decoded image as model input: channel-major (`3 × height × width`) values.
#[derive(Debug, Clone, PartialEq)]
pub struct PixelTensor {
    /// Values in `C, H, W` order.
    pub values: Vec<f32>,
    /// Height in pixels.
    pub height: usize,
    /// Width in pixels.
    pub width: usize,
}

impl ImagePreprocessing {
    /// Reads a Hugging Face `preprocessor_config.json`.
    ///
    /// # Errors
    ///
    /// Returns a reason when the file is not valid JSON or describes a step this adapter
    /// does not implement (no resize, a crop larger than the resize, an unknown filter,
    /// or a non-positive standard deviation).
    pub fn from_json(text: &str) -> Result<Self, String> {
        let raw: RawConfig = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if !raw.do_resize {
            return Err("models without a resize step are not supported".to_owned());
        }
        let resize = match raw.size {
            Some(SizeSpec::Square(edge)) => Resize::ShortestEdge(edge),
            Some(SizeSpec::Map {
                shortest_edge: Some(edge),
                ..
            }) => Resize::ShortestEdge(edge),
            Some(SizeSpec::Map {
                height: Some(height),
                width: Some(width),
                ..
            }) => Resize::Exact { height, width },
            _ => return Err("size must give shortest_edge or height and width".to_owned()),
        };
        let crop = if raw.do_center_crop {
            match raw.crop_size {
                Some(SizeSpec::Square(side)) => Some((side, side)),
                Some(SizeSpec::Map {
                    height: Some(height),
                    width: Some(width),
                    ..
                }) => Some((height, width)),
                _ => return Err("do_center_crop needs crop_size".to_owned()),
            }
        } else {
            None
        };
        let resample = match raw.resample.unwrap_or(3) {
            2 => Resample::Bilinear,
            3 => Resample::Bicubic,
            other => return Err(format!("resample filter {other} is not supported")),
        };
        let rescale = if raw.do_rescale {
            raw.rescale_factor.unwrap_or(1.0 / 255.0)
        } else {
            1.0
        };
        let (mean, std) = if raw.do_normalize {
            (
                raw.image_mean.ok_or("do_normalize needs image_mean")?,
                raw.image_std.ok_or("do_normalize needs image_std")?,
            )
        } else {
            ([0.0; 3], [1.0; 3])
        };
        if std.iter().any(|s| !(s.is_finite() && *s > 0.0)) {
            return Err("image_std must be positive".to_owned());
        }
        let preprocessing = Self {
            resize,
            crop,
            resample,
            rescale,
            mean,
            std,
        };
        if let (Some((ch, cw)), Resize::Exact { height, width }) = (crop, resize)
            && (ch > height || cw > width)
        {
            return Err("crop_size exceeds the resized image".to_owned());
        }
        Ok(preprocessing)
    }

    /// Canonical description, recorded in the embedding-space identity.
    #[must_use]
    pub fn descriptor(&self) -> String {
        let resize = match self.resize {
            Resize::ShortestEdge(edge) => format!("shortest_edge={edge}"),
            Resize::Exact { height, width } => format!("exact={height}x{width}"),
        };
        let crop = self
            .crop
            .map_or_else(|| "none".to_owned(), |(h, w)| format!("{h}x{w}"));
        format!(
            "image:resize={resize};crop={crop};resample={};rescale={};mean={:?};std={:?}",
            self.resample.name(),
            self.rescale,
            self.mean,
            self.std
        )
    }

    /// Model input size, `(height, width)`, when it does not depend on the source image
    /// (a crop or an exact resize). Images can only be batched when it is fixed.
    #[must_use]
    pub const fn fixed_output_size(&self) -> Option<(u32, u32)> {
        match (self.crop, self.resize) {
            (Some(crop), _) => Some(crop),
            (None, Resize::Exact { height, width }) => Some((height, width)),
            (None, Resize::ShortestEdge(_)) => None,
        }
    }

    /// Size of the model input, `(height, width)`.
    #[must_use]
    pub fn output_size(&self, source_height: u32, source_width: u32) -> (u32, u32) {
        let resized = self.resized_size(source_height, source_width);
        self.crop.unwrap_or(resized)
    }

    fn resized_size(&self, height: u32, width: u32) -> (u32, u32) {
        match self.resize {
            Resize::Exact { height, width } => (height, width),
            Resize::ShortestEdge(edge) => {
                // As the Hugging Face processors: the long side is truncated, not rounded.
                let (short, long) = (height.min(width).max(1), height.max(width));
                let scaled = u32::try_from(u64::from(long) * u64::from(edge) / u64::from(short))
                    .unwrap_or(u32::MAX);
                if height <= width {
                    (edge, scaled)
                } else {
                    (scaled, edge)
                }
            }
        }
    }

    /// Decodes `bytes` (PNG or JPEG) and turns them into the model input.
    ///
    /// # Errors
    ///
    /// Returns a reason when the image cannot be decoded or is smaller than the crop.
    pub fn pixels(&self, bytes: &[u8]) -> Result<PixelTensor, String> {
        let decoded = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
        let rgb = decoded.to_rgb8();
        let (height, width) = self.resized_size(rgb.height(), rgb.width());
        let resized =
            image::imageops::resize(&rgb, width.max(1), height.max(1), self.resample.filter());
        let (out_h, out_w) = self.crop.unwrap_or((resized.height(), resized.width()));
        if out_h > resized.height() || out_w > resized.width() {
            return Err(format!(
                "resized image {}x{} is smaller than the crop {out_h}x{out_w}",
                resized.height(),
                resized.width()
            ));
        }
        // Center crop as the Hugging Face processors: offsets rounded down.
        let top = (resized.height() - out_h) / 2;
        let left = (resized.width() - out_w) / 2;
        let (h, w) = (
            usize::try_from(out_h).map_err(|e| e.to_string())?,
            usize::try_from(out_w).map_err(|e| e.to_string())?,
        );
        let mut values = vec![0.0_f32; 3 * h * w];
        for (y, row) in (top..top + out_h).enumerate() {
            for (x, col) in (left..left + out_w).enumerate() {
                let pixel = resized.get_pixel(col, row);
                for (c, value) in pixel.0.iter().enumerate() {
                    let scaled = f32::from(*value) * self.rescale;
                    values[c * h * w + y * w + x] = (scaled - self.mean[c]) / self.std[c];
                }
            }
        }
        Ok(PixelTensor {
            values,
            height: h,
            width: w,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIP: &str = r#"{"crop_size": 224, "do_center_crop": true, "do_normalize": true,
        "do_resize": true, "feature_extractor_type": "CLIPFeatureExtractor",
        "image_mean": [0.48145466, 0.4578275, 0.40821073],
        "image_std": [0.26862954, 0.26130258, 0.27577711], "resample": 3, "size": 224}"#;
    const SIGLIP: &str = r#"{"do_normalize": true, "do_rescale": true, "do_resize": true,
        "image_mean": [0.5, 0.5, 0.5], "image_processor_type": "SiglipImageProcessor",
        "image_std": [0.5, 0.5, 0.5], "processor_class": "SiglipProcessor", "resample": 3,
        "rescale_factor": 0.00392156862745098, "size": {"height": 256, "width": 256}}"#;

    fn png(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(width, height, image::Rgb(rgb));
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .expect("encode");
        out
    }

    #[test]
    #[expect(
        clippy::excessive_precision,
        reason = "the constants are copied verbatim from the pinned preprocessor_config.json"
    )]
    fn the_pinned_clip_and_siglip_configs_are_read_exactly() {
        let clip = ImagePreprocessing::from_json(CLIP).expect("clip");
        assert_eq!(clip.resize, Resize::ShortestEdge(224));
        assert_eq!(clip.crop, Some((224, 224)));
        assert_eq!(clip.resample, Resample::Bicubic);
        assert_eq!(clip.std, [0.268_629_54, 0.261_302_58, 0.275_777_1]);
        let siglip = ImagePreprocessing::from_json(SIGLIP).expect("siglip");
        assert_eq!(
            siglip.resize,
            Resize::Exact {
                height: 256,
                width: 256
            }
        );
        assert_eq!(siglip.crop, None);
        assert_eq!(siglip.mean, [0.5; 3]);
        assert_ne!(clip.descriptor(), siglip.descriptor(), "spaces must differ");
        assert_eq!(clip.fixed_output_size(), Some((224, 224)));
        assert_eq!(siglip.fixed_output_size(), Some((256, 256)));
        let uncropped = ImagePreprocessing { crop: None, ..clip };
        assert_eq!(
            uncropped.fixed_output_size(),
            None,
            "a shortest-edge resize without a crop depends on the source"
        );
    }

    #[test]
    #[expect(
        clippy::excessive_precision,
        reason = "the constants are copied verbatim from the pinned preprocessor_config.json"
    )]
    fn clip_resizes_the_short_side_then_center_crops() {
        let clip = ImagePreprocessing::from_json(CLIP).expect("clip");
        // 256x192 (the fixture ultrasound): short side 192 -> 224, long side
        // 256*224/192 = 298 (truncated), then a 224x224 center crop.
        assert_eq!(clip.resized_size(192, 256), (224, 298));
        assert_eq!(clip.output_size(192, 256), (224, 224));
        let tensor = clip.pixels(&png(256, 192, [255, 0, 128])).expect("pixels");
        assert_eq!((tensor.height, tensor.width), (224, 224));
        assert_eq!(tensor.values.len(), 3 * 224 * 224);
        // A uniform image stays uniform: each channel is (v/255 - mean) / std.
        let expected = [
            (1.0 - 0.481_454_66) / 0.268_629_54,
            (0.0 - 0.457_827_5) / 0.261_302_58,
            (128.0 / 255.0 - 0.408_210_73) / 0.275_777_1,
        ];
        for (c, want) in expected.iter().enumerate() {
            let channel = &tensor.values[c * 224 * 224..(c + 1) * 224 * 224];
            for value in channel {
                assert!(
                    (value - want).abs() < 1e-4,
                    "channel {c}: {value} vs {want}"
                );
            }
        }
    }

    #[test]
    fn siglip_resizes_exactly_and_maps_to_minus_one_one() {
        let siglip = ImagePreprocessing::from_json(SIGLIP).expect("siglip");
        let tensor = siglip.pixels(&png(300, 100, [0, 255, 0])).expect("pixels");
        assert_eq!((tensor.height, tensor.width), (256, 256));
        let plane = 256 * 256;
        assert!(
            tensor.values[..plane]
                .iter()
                .all(|v| (v + 1.0).abs() < 1e-4)
        );
        assert!(
            tensor.values[plane..2 * plane]
                .iter()
                .all(|v| (v - 1.0).abs() < 1e-4)
        );
    }

    #[test]
    fn unsupported_configs_are_refused() {
        for bad in [
            r#"{"do_resize": false}"#,
            r#"{"size": {"longest_edge": 10}}"#,
            r#"{"size": 10, "resample": 0, "image_mean": [0,0,0], "image_std": [1,1,1]}"#,
            r#"{"size": 10, "image_mean": [0,0,0], "image_std": [1,0,1]}"#,
            r#"{"size": 10, "do_center_crop": true, "image_mean": [0,0,0], "image_std": [1,1,1]}"#,
            "not json",
        ] {
            assert!(ImagePreprocessing::from_json(bad).is_err(), "{bad}");
        }
        let clip = ImagePreprocessing::from_json(CLIP).expect("clip");
        assert!(clip.pixels(b"not an image").is_err());
    }
}
