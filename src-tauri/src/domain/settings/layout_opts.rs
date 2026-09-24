use super::paper::{fmt_num, parse_length_mm};
use std::fmt;
use std::str::FromStr;

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
            .split([',', ' '])
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
