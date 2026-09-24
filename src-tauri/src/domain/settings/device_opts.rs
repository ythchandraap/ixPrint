use std::fmt;
use std::str::FromStr;

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
