//! Print settings: the partial form used by presets and requests, and the fully
//! resolved form used for a print job.
//!
//! Every value has a compact string form so the same syntax works in query
//! parameters, presets and exported preset files (e.g. `paper=80x200mm`,
//! `margins=5,3`, `pages=1-3,5`, `scale=90%`).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

const MM_PER_INCH: f32 = 25.4;
const PT_PER_INCH: f32 = 72.0;

/// Upper bound for a custom/roll paper side, mirroring template.rs's
/// MAX_SIZE_MM so both paths reject the same absurd sizes.
pub const MAX_PAPER_MM: f32 = 2000.0;

pub fn mm_to_pt(mm: f32) -> f32 {
    mm / MM_PER_INCH * PT_PER_INCH
}

pub fn pt_to_mm(pt: f32) -> f32 {
    pt / PT_PER_INCH * MM_PER_INCH
}

/// Implements serde through the value's string form.
macro_rules! string_serde {
    ($ty:ty) => {
        impl Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.to_string())
            }
        }
        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Orientation {
    Portrait,
    Landscape,
    /// Follow each page's own shape.
    Auto,
}

impl FromStr for Orientation {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "portrait" => Ok(Self::Portrait),
            "landscape" => Ok(Self::Landscape),
            "auto" => Ok(Self::Auto),
            _ => Err(format!("Orientation '{}' is invalid (portrait, landscape, auto)", s)),
        }
    }
}

impl fmt::Display for Orientation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::Portrait => "portrait",
            Self::Landscape => "landscape",
            Self::Auto => "auto",
        })
    }
}
string_serde!(Orientation);

/// Standard cut-sheet sizes, in millimetres (portrait).
pub const STANDARD_PAPERS: &[(&str, f32, f32)] = &[
    ("A3", 297.0, 420.0),
    ("A4", 210.0, 297.0),
    ("A5", 148.0, 210.0),
    ("A6", 105.0, 148.0),
    ("B5", 176.0, 250.0),
    ("Letter", 215.9, 279.4),
    ("Legal", 215.9, 355.6),
    ("F4", 215.0, 330.0),
];

#[derive(Clone, Debug, PartialEq)]
pub enum Paper {
    /// Whatever paper the printer is currently configured for.
    PrinterDefault,
    /// One of [`STANDARD_PAPERS`].
    Standard(&'static str),
    /// Continuous roll (receipt printers): fixed width, length follows the content.
    Roll {
        width_mm: f32,
    },
    Custom {
        width_mm: f32,
        height_mm: f32,
    },
}

impl Paper {
    /// Portrait size in millimetres; `None` for roll paper and the printer default.
    pub fn size_mm(&self) -> Option<(f32, f32)> {
        match self {
            Paper::Standard(name) => STANDARD_PAPERS
                .iter()
                .find(|(n, _, _)| n == name)
                .map(|&(_, w, h)| (w, h)),
            Paper::Custom { width_mm, height_mm } => Some((*width_mm, *height_mm)),
            _ => None,
        }
    }
}

/// Parses a length like `80`, `80mm`, `8cm` or `3.15in` into millimetres.
fn parse_length_mm(s: &str) -> Option<f32> {
    let s = s.trim().to_ascii_lowercase();
    let (num, factor) = if let Some(n) = s.strip_suffix("mm") {
        (n, 1.0)
    } else if let Some(n) = s.strip_suffix("cm") {
        (n, 10.0)
    } else if let Some(n) = s.strip_suffix("in") {
        (n, MM_PER_INCH)
    } else {
        (s.as_str(), 1.0)
    };
    let v: f32 = num.trim().replace(',', ".").parse().ok()?;
    (v.is_finite() && v >= 0.0).then_some(v * factor)
}

fn fmt_num(v: f32) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{}", rounded)
    }
}

impl FromStr for Paper {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let t = s.trim();
        let lower = t.to_ascii_lowercase();
        if lower.is_empty() || lower == "default" {
            return Ok(Paper::PrinterDefault);
        }
        if let Some(&(name, _, _)) = STANDARD_PAPERS.iter().find(|(n, _, _)| n.eq_ignore_ascii_case(&lower)) {
            return Ok(Paper::Standard(name));
        }
        let invalid = || {
            format!(
                "Paper '{}' is invalid (default, A4, Letter, ..., roll '80mm', custom '100x150mm')",
                s
            )
        };
        let body = lower.strip_prefix("roll:").unwrap_or(&lower);
        if let Some((w, h)) = body.split_once('x') {
            // Unit written once at the end applies to both sides: "100x150mm".
            let unit: String = h.chars().skip_while(|c| c.is_ascii_digit() || *c == '.').collect();
            let w_str = if w.chars().all(|c| c.is_ascii_digit() || c == '.') {
                format!("{}{}", w, unit)
            } else {
                w.to_string()
            };
            let width_mm = parse_length_mm(&w_str).ok_or_else(invalid)?;
            let height_mm = parse_length_mm(h).ok_or_else(invalid)?;
            if width_mm < 10.0 || height_mm < 10.0 {
                return Err(invalid());
            }
            return Ok(Paper::Custom { width_mm, height_mm });
        }
        let width_mm = parse_length_mm(body).ok_or_else(invalid)?;
        if width_mm < 10.0 {
            return Err(invalid());
        }
        Ok(Paper::Roll { width_mm })
    }
}

impl fmt::Display for Paper {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Paper::PrinterDefault => f.write_str("default"),
            Paper::Standard(name) => f.write_str(name),
            Paper::Roll { width_mm } => write!(f, "{}mm", fmt_num(*width_mm)),
            Paper::Custom { width_mm, height_mm } => {
                write!(f, "{}x{}mm", fmt_num(*width_mm), fmt_num(*height_mm))
            }
        }
    }
}
string_serde!(Paper);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scale {
    /// Shrink pages that don't fit; never enlarge (SumatraPDF's default).
    Shrink,
    /// Scale up or down to fill the printable area.
    Fit,
    /// Actual size.
    None,
    Percent(f32),
}

impl FromStr for Scale {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let t = s.trim().to_ascii_lowercase();
        match t.as_str() {
            "shrink" => Ok(Self::Shrink),
            "fit" => Ok(Self::Fit),
            "none" | "actual" => Ok(Self::None),
            _ => {
                let v: f32 = t.trim_end_matches('%').trim().replace(',', ".").parse().map_err(|_| {
                    format!(
                        "Scale '{}' is invalid (shrink, fit, none, or a percentage such as 90)",
                        s
                    )
                })?;
                if !(1.0..=1000.0).contains(&v) {
                    return Err(format!("Scale {}% is outside the 1–1000 range", v));
                }
                Ok(Self::Percent(v))
            }
        }
    }
}

impl fmt::Display for Scale {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Shrink => f.write_str("shrink"),
            Self::Fit => f.write_str("fit"),
            Self::None => f.write_str("none"),
            Self::Percent(v) => write!(f, "{}%", fmt_num(*v)),
        }
    }
}
string_serde!(Scale);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Center,
    Top,
    TopLeft,
}

impl FromStr for Align {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "center" => Ok(Self::Center),
            "top" => Ok(Self::Top),
            "top-left" => Ok(Self::TopLeft),
            _ => Err(format!("Alignment '{}' is invalid (center, top, top-left)", s)),
        }
    }
}

impl fmt::Display for Align {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::Center => "center",
            Self::Top => "top",
            Self::TopLeft => "top-left",
        })
    }
}
string_serde!(Align);

/// Margins in millimetres.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Margins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl FromStr for Margins {
    type Err = String;
    /// CSS-like: `5` (all), `5,3` (vertical, horizontal) or `5,3,5,3` (top, right, bottom, left).
    fn from_str(s: &str) -> Result<Self, String> {
        let invalid = || format!("Margins '{}' are invalid (e.g. 5, 5,3 or 5,3,5,3 in mm)", s);
        let v: Vec<f32> = s
            .split(|c| c == ',' || c == ' ')
            .filter(|p| !p.is_empty())
            .map(|p| parse_length_mm(p).ok_or_else(invalid))
            .collect::<Result<_, _>>()?;
        match v.as_slice() {
            [a] => Ok(Margins {
                top: *a,
                right: *a,
                bottom: *a,
                left: *a,
            }),
            [vert, hor] => Ok(Margins {
                top: *vert,
                right: *hor,
                bottom: *vert,
                left: *hor,
            }),
            [t, r, b, l] => Ok(Margins {
                top: *t,
                right: *r,
                bottom: *b,
                left: *l,
            }),
            _ => Err(invalid()),
        }
    }
}

impl fmt::Display for Margins {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Margins {
            top,
            right,
            bottom,
            left,
        } = *self;
        if top == right && right == bottom && bottom == left {
            write!(f, "{}", fmt_num(top))
        } else if top == bottom && left == right {
            write!(f, "{},{}", fmt_num(top), fmt_num(right))
        } else {
            write!(
                f,
                "{},{},{},{}",
                fmt_num(top),
                fmt_num(right),
                fmt_num(bottom),
                fmt_num(left)
            )
        }
    }
}
string_serde!(Margins);

/// 1-based page ranges, e.g. `1-3,5,8-` (open end = until the last page).
#[derive(Clone, Debug, PartialEq)]
pub struct PageRange(Vec<(u32, Option<u32>)>);

impl PageRange {
    /// Selected 0-based page indexes, in the order written; out-of-range pages are skipped.
    pub fn indexes(&self, page_count: u32) -> Vec<u32> {
        let mut out = Vec::new();
        for &(start, end) in &self.0 {
            let end = end.unwrap_or(page_count).min(page_count);
            for p in start..=end {
                if p >= 1 && p <= page_count {
                    out.push(p - 1);
                }
            }
        }
        out
    }
}

impl FromStr for PageRange {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let invalid = || format!("Pages '{}' are invalid (e.g. 1-3,5,8-)", s);
        let mut ranges = Vec::new();
        for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let range = match part.split_once('-') {
                Some((a, b)) => {
                    let start: u32 = a.trim().parse().map_err(|_| invalid())?;
                    let end = match b.trim() {
                        "" => None,
                        b => Some(b.parse::<u32>().map_err(|_| invalid())?),
                    };
                    (start, end)
                }
                None => {
                    let p: u32 = part.parse().map_err(|_| invalid())?;
                    (p, Some(p))
                }
            };
            if range.0 == 0 || range.1.is_some_and(|e| e < range.0) {
                return Err(invalid());
            }
            ranges.push(range);
        }
        if ranges.is_empty() {
            return Err(invalid());
        }
        Ok(PageRange(ranges))
    }
}

impl fmt::Display for PageRange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let parts: Vec<String> = self
            .0
            .iter()
            .map(|&(start, end)| match end {
                Some(e) if e == start => start.to_string(),
                Some(e) => format!("{}-{}", start, e),
                None => format!("{}-", start),
            })
            .collect();
        f.write_str(&parts.join(","))
    }
}
string_serde!(PageRange);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Duplex {
    Off,
    LongEdge,
    ShortEdge,
}

impl FromStr for Duplex {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "simplex" => Ok(Self::Off),
            "long-edge" | "long" => Ok(Self::LongEdge),
            "short-edge" | "short" => Ok(Self::ShortEdge),
            _ => Err(format!("Duplex '{}' is invalid (off, long-edge, short-edge)", s)),
        }
    }
}

impl fmt::Display for Duplex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::Off => "off",
            Self::LongEdge => "long-edge",
            Self::ShortEdge => "short-edge",
        })
    }
}
string_serde!(Duplex);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColorMode {
    Color,
    Grayscale,
}

impl FromStr for ColorMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "color" | "colour" => Ok(Self::Color),
            "grayscale" | "greyscale" | "mono" | "monochrome" => Ok(Self::Grayscale),
            _ => Err(format!("Colour '{}' is invalid (color, grayscale)", s)),
        }
    }
}

impl fmt::Display for ColorMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::Color => "color",
            Self::Grayscale => "grayscale",
        })
    }
}
string_serde!(ColorMode);

/// A partial set of settings: a preset, or the overrides sent with a request.
/// Unset fields fall through to the next layer.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PrintSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copies: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<Orientation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paper: Option<Paper>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<Scale>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margins: Option<Margins>,
    /// Shift in millimetres; positive moves right.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset_x: Option<f32>,
    /// Shift in millimetres; positive moves down.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset_y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<PageRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplex: Option<Duplex>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tray: Option<String>,
    /// Raster resolution used on Windows (the CUPS path stays vector).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi: Option<u32>,
}

pub const MAX_COPIES: u32 = 999;
pub const MIN_DPI: u32 = 72;
pub const MAX_DPI: u32 = 600;

impl PrintSettings {
    /// `self` with every field set in `over` replaced.
    pub fn overlay(&self, over: &PrintSettings) -> PrintSettings {
        macro_rules! pick {
            ($($f:ident),*) => { PrintSettings { $($f: over.$f.clone().or_else(|| self.$f.clone()),)* } };
        }
        pick!(
            printer,
            copies,
            collate,
            orientation,
            paper,
            scale,
            align,
            margins,
            offset_x,
            offset_y,
            pages,
            duplex,
            color,
            tray,
            dpi
        )
    }

    /// Checks numeric ranges; string forms are already validated while parsing.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(c) = self.copies {
            if c == 0 || c > MAX_COPIES {
                return Err(format!("copies harus 1–{}", MAX_COPIES));
            }
        }
        if let Some(d) = self.dpi {
            if !(MIN_DPI..=MAX_DPI).contains(&d) {
                return Err(format!("dpi harus {}–{}", MIN_DPI, MAX_DPI));
            }
        }
        for v in [self.offset_x, self.offset_y].into_iter().flatten() {
            if !v.is_finite() || v.abs() > 500.0 {
                return Err("offset harus antara -500 dan 500 mm".into());
            }
        }
        if let Some(p) = &self.printer {
            if p.trim().is_empty() {
                return Err("Printer name cannot be empty".into());
            }
        }
        if let Some(paper) = &self.paper {
            let sides: &[f32] = match paper {
                Paper::Custom { width_mm, height_mm } => &[*width_mm, *height_mm],
                Paper::Roll { width_mm } => &[*width_mm],
                _ => &[],
            };
            for &v in sides {
                if !(10.0..=MAX_PAPER_MM).contains(&v) {
                    return Err(format!(
                        "Custom paper size must be between 10 and {} mm per side",
                        MAX_PAPER_MM
                    ));
                }
            }
        }
        Ok(())
    }

    /// Reads settings from query parameters. Empty values count as "not set", so
    /// existing clients sending `copies=&orientation=` keep working.
    /// Returns the settings and the requested preset name, if any.
    pub fn from_query(params: &HashMap<String, String>) -> Result<(PrintSettings, Option<String>), String> {
        fn get<'a>(params: &'a HashMap<String, String>, keys: &[&str]) -> Option<&'a str> {
            keys.iter()
                .filter_map(|k| params.get(*k))
                .map(|v| v.trim())
                .find(|v| !v.is_empty())
        }
        fn parse<T: FromStr>(params: &HashMap<String, String>, key: &str) -> Result<Option<T>, String>
        where
            T::Err: fmt::Display,
        {
            get(params, &[key])
                .map(|v| v.parse::<T>().map_err(|e| format!("{}: {}", key, e)))
                .transpose()
        }
        fn parse_bool(params: &HashMap<String, String>, key: &str) -> Result<Option<bool>, String> {
            get(params, &[key])
                .map(|v| match v.to_ascii_lowercase().as_str() {
                    "1" | "true" | "yes" | "on" => Ok(true),
                    "0" | "false" | "no" | "off" => Ok(false),
                    _ => Err(format!("{}: '{}' harus true/false", key, v)),
                })
                .transpose()
        }

        let settings = PrintSettings {
            printer: get(params, &["printer", "printer_name"]).map(str::to_string),
            copies: parse(params, "copies")?,
            collate: parse_bool(params, "collate")?,
            orientation: parse(params, "orientation")?,
            paper: parse(params, "paper")?,
            scale: parse(params, "scale")?,
            align: parse(params, "align")?,
            margins: parse(params, "margins")?,
            offset_x: parse(params, "offset_x")?,
            offset_y: parse(params, "offset_y")?,
            pages: parse(params, "pages")?,
            duplex: parse(params, "duplex")?,
            color: parse(params, "color")?,
            tray: get(params, &["tray"]).map(str::to_string),
            dpi: parse(params, "dpi")?,
        };
        settings.validate()?;
        Ok((settings, get(params, &["preset"]).map(str::to_string)))
    }
}

/// Fully resolved settings for one job. `None` in the device options means
/// "leave the printer's own setting alone".
#[derive(Serialize, Clone, Debug)]
pub struct JobSettings {
    pub printer: String,
    pub copies: u32,
    pub collate: bool,
    pub orientation: Orientation,
    pub paper: Paper,
    pub scale: Scale,
    pub align: Align,
    pub margins: Margins,
    pub offset_x: f32,
    pub offset_y: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<PageRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplex: Option<Duplex>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tray: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi: Option<u32>,
}

impl JobSettings {
    /// Fills unset fields with the built-in defaults.
    pub fn resolve(s: &PrintSettings, printer: String) -> JobSettings {
        JobSettings {
            printer,
            copies: s.copies.unwrap_or(1),
            collate: s.collate.unwrap_or(true),
            orientation: s.orientation.unwrap_or(Orientation::Auto),
            paper: s.paper.clone().unwrap_or(Paper::PrinterDefault),
            scale: s.scale.unwrap_or(Scale::Shrink),
            align: s.align.unwrap_or(Align::Center),
            margins: s.margins.unwrap_or_default(),
            offset_x: s.offset_x.unwrap_or(0.0),
            offset_y: s.offset_y.unwrap_or(0.0),
            pages: s.pages.clone(),
            duplex: s.duplex,
            color: s.color,
            tray: s.tray.clone().filter(|t| !t.trim().is_empty()),
            dpi: s.dpi,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_parsing() {
        assert_eq!("a4".parse::<Paper>().unwrap(), Paper::Standard("A4"));
        assert_eq!("".parse::<Paper>().unwrap(), Paper::PrinterDefault);
        assert_eq!("80mm".parse::<Paper>().unwrap(), Paper::Roll { width_mm: 80.0 });
        assert_eq!("roll:58".parse::<Paper>().unwrap(), Paper::Roll { width_mm: 58.0 });
        assert_eq!(
            "100x150mm".parse::<Paper>().unwrap(),
            Paper::Custom {
                width_mm: 100.0,
                height_mm: 150.0
            }
        );
        assert_eq!(
            "4x6in".parse::<Paper>().unwrap(),
            Paper::Custom {
                width_mm: 101.6,
                height_mm: 152.4
            }
        );
        assert!("banana".parse::<Paper>().is_err());
        assert_eq!(
            Paper::Custom {
                width_mm: 100.0,
                height_mm: 150.0
            }
            .to_string(),
            "100x150mm"
        );
    }

    #[test]
    fn validate_rejects_oversized_custom_paper() {
        let settings = PrintSettings {
            paper: Some(Paper::Custom {
                width_mm: 50000.0,
                height_mm: 50000.0,
            }),
            ..Default::default()
        };
        assert!(settings.validate().unwrap_err().contains("must be between"));
    }

    #[test]
    fn scale_margins_pages() {
        assert_eq!("90%".parse::<Scale>().unwrap(), Scale::Percent(90.0));
        assert_eq!("fit".parse::<Scale>().unwrap(), Scale::Fit);
        assert_eq!(
            "5,3".parse::<Margins>().unwrap(),
            Margins {
                top: 5.0,
                right: 3.0,
                bottom: 5.0,
                left: 3.0
            }
        );
        let pages: PageRange = "1-3,5,8-".parse().unwrap();
        assert_eq!(pages.indexes(9), vec![0, 1, 2, 4, 7, 8]);
        assert_eq!(pages.to_string(), "1-3,5,8-");
        assert!("0-2".parse::<PageRange>().is_err());
        assert!("3-1".parse::<PageRange>().is_err());
    }

    #[test]
    fn overlay_and_query() {
        let preset = PrintSettings {
            printer: Some("EPSON".into()),
            copies: Some(2),
            paper: Some(Paper::Roll { width_mm: 80.0 }),
            ..Default::default()
        };
        let mut q = HashMap::new();
        q.insert("printer_name".to_string(), "".to_string());
        q.insert("copies".to_string(), "3".to_string());
        q.insert("orientation".to_string(), "portrait".to_string());
        q.insert("preset".to_string(), "struk".to_string());
        let (req, preset_name) = PrintSettings::from_query(&q).unwrap();
        assert_eq!(preset_name.as_deref(), Some("struk"));
        let merged = preset.overlay(&req);
        assert_eq!(merged.printer.as_deref(), Some("EPSON"));
        assert_eq!(merged.copies, Some(3));
        assert_eq!(merged.orientation, Some(Orientation::Portrait));
        assert_eq!(merged.paper, Some(Paper::Roll { width_mm: 80.0 }));

        q.insert("copies".to_string(), "0".to_string());
        assert!(PrintSettings::from_query(&q).is_err());
    }

    #[test]
    fn json_roundtrip_rejects_unknown() {
        let s: PrintSettings = serde_json::from_str(r#"{"paper":"80mm","scale":"90%","margins":"2"}"#).unwrap();
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"paper":"80mm","scale":"90%","margins":"2"}"#
        );
        assert!(serde_json::from_str::<PrintSettings>(r#"{"papr":"A4"}"#).is_err());
        assert!(serde_json::from_str::<PrintSettings>(r#"{"paper":"A9"}"#).is_err());
    }
}
