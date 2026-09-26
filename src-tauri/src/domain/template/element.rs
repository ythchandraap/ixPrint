use crate::domain::settings::MAX_PAPER_MM;
use serde::{Deserialize, Serialize};

pub(super) const MAX_ELEMENTS: usize = 500;
pub(super) const MIN_SIZE_MM: f32 = 5.0;
/// Same bound as `PrintSettings::validate`'s custom/roll paper check, so a
/// design's page size and a plain print job reject the same absurd sizes.
pub(super) const MAX_SIZE_MM: f32 = MAX_PAPER_MM;

fn default_font_size() -> f32 {
    10.0
}
fn default_true() -> bool {
    true
}
fn default_stroke() -> f32 {
    0.75
}

/// Largest embedded image, as base64 text (about 3 MB of image data).
const MAX_IMAGE_CHARS: usize = 4_000_000;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    /// Keep the aspect ratio, centered inside the box.
    #[default]
    Contain,
    /// Fill the box exactly.
    Stretch,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextFont {
    #[default]
    Helvetica,
    Times,
    Courier,
    InstrumentSans,
    IbmPlexMono,
}

impl TextFont {
    fn supports_weight(self, weight: TextWeight) -> bool {
        match self {
            Self::Helvetica | Self::Times | Self::Courier => matches!(weight, TextWeight::Regular | TextWeight::Bold),
            Self::InstrumentSans => true,
            Self::IbmPlexMono => matches!(weight, TextWeight::Regular | TextWeight::Medium | TextWeight::SemiBold),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextWeight {
    #[default]
    Regular,
    Medium,
    #[serde(rename = "semibold")]
    SemiBold,
    Bold,
}

/// Positions and sizes are in mm from the template's top-left corner.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Element {
    Text {
        #[serde(default)]
        id: String,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        /// May contain `{{field}}` placeholders and line breaks.
        text: String,
        #[serde(default = "default_font_size")]
        size: f32,
        /// App or PDF standard family; missing values use Helvetica for older templates.
        #[serde(default)]
        font: TextFont,
        #[serde(default)]
        weight: Option<TextWeight>,
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        align: HAlign,
        #[serde(default)]
        valign: VAlign,
        /// Wrap long lines inside the box.
        #[serde(default = "default_true")]
        wrap: bool,
        /// Shrink the font until the text fits the box.
        #[serde(default = "default_true")]
        fit: bool,
    },
    Line {
        #[serde(default)]
        id: String,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        #[serde(default = "default_stroke")]
        width: f32,
    },
    Rect {
        #[serde(default)]
        id: String,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        /// Border in points; 0 = no border.
        #[serde(default = "default_stroke")]
        stroke: f32,
        #[serde(default)]
        fill: bool,
    },
    /// Code 128 (printable ASCII).
    Barcode {
        #[serde(default)]
        id: String,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        value: String,
        #[serde(default = "default_true")]
        show_text: bool,
    },
    Qr {
        #[serde(default)]
        id: String,
        x: f32,
        y: f32,
        size: f32,
        value: String,
    },
    /// Logo or picture (PNG, JPEG, GIF, BMP, WebP, TIFF, ICO; the editor turns
    /// any other format into PNG). `src` is a data URL (or bare base64), or a
    /// `{{field}}` whose data value is one — e.g. a patient photo per record.
    Image {
        #[serde(default)]
        id: String,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        src: String,
        #[serde(default)]
        fit: ImageFit,
        /// Print in shades of gray (clearer on thermal / monochrome printers).
        #[serde(default)]
        grayscale: bool,
    },
}

impl Element {
    pub(super) fn id(&self) -> &str {
        match self {
            Element::Text { id, .. }
            | Element::Line { id, .. }
            | Element::Rect { id, .. }
            | Element::Barcode { id, .. }
            | Element::Qr { id, .. }
            | Element::Image { id, .. } => id,
        }
    }

    /// Top edge in mm; decides which band (header / row / footer) an element is in.
    pub(crate) fn top(&self) -> f32 {
        match self {
            Element::Line { y1, y2, .. } => y1.min(*y2),
            Element::Text { y, .. }
            | Element::Rect { y, .. }
            | Element::Barcode { y, .. }
            | Element::Qr { y, .. }
            | Element::Image { y, .. } => *y,
        }
    }

    /// A copy moved down by `dy` mm.
    pub(crate) fn shifted(&self, dy: f32) -> Element {
        let mut el = self.clone();
        match &mut el {
            Element::Line { y1, y2, .. } => {
                *y1 += dy;
                *y2 += dy;
            }
            Element::Text { y, .. }
            | Element::Rect { y, .. }
            | Element::Barcode { y, .. }
            | Element::Qr { y, .. }
            | Element::Image { y, .. } => *y += dy,
        }
        el
    }

    /// Strings that may contain placeholders.
    pub(super) fn templates(&self) -> Vec<&str> {
        match self {
            Element::Text { text, .. } => vec![text],
            Element::Barcode { value, .. } | Element::Qr { value, .. } => vec![value],
            Element::Image { src, .. } if src.contains("{{") => vec![src],
            _ => vec![],
        }
    }

    pub(super) fn check(&self) -> Result<(), String> {
        let finite = |vals: &[f32]| vals.iter().all(|v| v.is_finite() && v.abs() <= MAX_SIZE_MM * 2.0);
        let ok = match self {
            Element::Text {
                x,
                y,
                w,
                h,
                size,
                font,
                weight,
                ..
            } => {
                finite(&[*x, *y, *w, *h, *size])
                    && *w > 0.0
                    && *h > 0.0
                    && (2.0..=300.0).contains(size)
                    && weight.is_none_or(|weight| font.supports_weight(weight))
            }
            Element::Line {
                x1, y1, x2, y2, width, ..
            } => finite(&[*x1, *y1, *x2, *y2, *width]) && *width > 0.0,
            Element::Rect { x, y, w, h, stroke, .. } => {
                finite(&[*x, *y, *w, *h, *stroke]) && *w > 0.0 && *h > 0.0 && *stroke >= 0.0
            }
            Element::Barcode { x, y, w, h, .. } => finite(&[*x, *y, *w, *h]) && *w > 0.0 && *h > 0.0,
            Element::Qr { x, y, size, .. } => finite(&[*x, *y, *size]) && *size > 0.0,
            Element::Image { x, y, w, h, src, .. } => {
                if src.len() > MAX_IMAGE_CHARS {
                    return Err("Image is too large (max. approx. 3 MB)".into());
                }
                finite(&[*x, *y, *w, *h]) && *w > 0.0 && *h > 0.0
            }
        };
        if ok {
            Ok(())
        } else {
            Err("Element size or position is invalid".into())
        }
    }
}
