use super::units::MAX_PAPER_MM;
use super::{Align, ColorMode, Duplex, Margins, Orientation, PageRange, Paper, Scale};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

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
                return Err(format!("copies must be between 1 and {}", MAX_COPIES));
            }
        }
        if let Some(d) = self.dpi {
            if !(MIN_DPI..=MAX_DPI).contains(&d) {
                return Err(format!("dpi must be between {} and {}", MIN_DPI, MAX_DPI));
            }
        }
        for v in [self.offset_x, self.offset_y].into_iter().flatten() {
            if !v.is_finite() || v.abs() > 500.0 {
                return Err("offset must be between -500 and 500 mm".into());
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
                    _ => Err(format!("{}: '{}' must be true or false", key, v)),
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
