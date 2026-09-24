use super::units::MM_PER_INCH;
use std::fmt;
use std::str::FromStr;

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
pub(super) fn parse_length_mm(s: &str) -> Option<f32> {
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

pub(super) fn fmt_num(v: f32) -> String {
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
