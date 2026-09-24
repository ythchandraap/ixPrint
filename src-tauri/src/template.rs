//! Designs: a label/receipt drawn in ix.print with `{{field}}` placeholders,
//! stored as the optional `design` of a preset. Clients send only the data;
//! ix.print renders one page per record at the design's exact size and prints
//! it with the preset's settings.

use crate::canvas::{wrap, Canvas};
use crate::layout::pdfium;
use crate::settings::{mm_to_pt, pt_to_mm, MAX_PAPER_MM};
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Values for the placeholders of one printed page.
pub type Record = BTreeMap<String, String>;

pub const MAX_RECORDS: usize = 500;
const MAX_ELEMENTS: usize = 500;
const MIN_SIZE_MM: f32 = 5.0;
/// Same bound as `PrintSettings::validate`'s custom/roll paper check, so a
/// design's page size and a plain print job reject the same absurd sizes.
const MAX_SIZE_MM: f32 = MAX_PAPER_MM;

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
    /// Top edge in mm; decides which band (header / row / footer) an element is in.
    fn top(&self) -> f32 {
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
    fn shifted(&self, dy: f32) -> Element {
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
    fn templates(&self) -> Vec<&str> {
        match self {
            Element::Text { text, .. } => vec![text],
            Element::Barcode { value, .. } | Element::Qr { value, .. } => vec![value],
            Element::Image { src, .. } if src.contains("{{") => vec![src],
            _ => vec![],
        }
    }

    fn check(&self) -> Result<(), String> {
        let finite = |vals: &[f32]| vals.iter().all(|v| v.is_finite() && v.abs() <= MAX_SIZE_MM * 2.0);
        let ok = match self {
            Element::Text { x, y, w, h, size, .. } => {
                finite(&[*x, *y, *w, *h, *size]) && *w > 0.0 && *h > 0.0 && (2.0..=300.0).contains(size)
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

/// Fields ix.print fills in itself.
pub const AUTO_FIELDS: [&str; 4] = ["no", "halaman", "total_halaman", "jumlah_data"];

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FooterMode {
    /// Once, right after the last row (totals, signatures).
    #[default]
    LastPage,
    /// At the bottom of every page.
    EveryPage,
}

/// How records map to pages.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Layout {
    /// One page per record (labels).
    #[default]
    Label,
    /// Records as rows under a header, continuing on new pages with the header
    /// repeated. In the editor the design is three stacked bands: header
    /// (`header_mm`), one row (`row_mm`) and footer (`footer_mm`); an element
    /// belongs to the band its top edge is in.
    List {
        header_mm: f32,
        row_mm: f32,
        #[serde(default)]
        footer_mm: f32,
        #[serde(default)]
        footer: FooterMode,
        /// Page height follows the content (receipt rolls): one long page.
        #[serde(default)]
        roll: bool,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Template {
    /// Page width.
    pub width_mm: f32,
    /// Page height (for label mode also the design height; unused on a roll).
    pub height_mm: f32,
    #[serde(default)]
    pub layout: Layout,
    #[serde(default)]
    pub elements: Vec<Element>,
    /// Example data for the editor preview and test prints.
    #[serde(default)]
    pub sample: Record,
}

impl Template {
    pub fn validate(&self) -> Result<(), String> {
        for (label, v) in [("width", self.width_mm), ("height", self.height_mm)] {
            if !(MIN_SIZE_MM..=MAX_SIZE_MM).contains(&v) {
                return Err(format!("Template {} must be {}–{} mm", label, MIN_SIZE_MM, MAX_SIZE_MM));
            }
        }
        if self.elements.len() > MAX_ELEMENTS {
            return Err(format!("maksimal {} elemen", MAX_ELEMENTS));
        }
        for (i, el) in self.elements.iter().enumerate() {
            el.check().map_err(|e| format!("elemen #{}: {}", i + 1, e))?;
        }
        if let Layout::List {
            header_mm,
            row_mm,
            footer_mm,
            footer,
            roll,
        } = self.layout
        {
            let ok = |v: f32| v.is_finite() && (0.0..=MAX_SIZE_MM).contains(&v);
            if !(ok(header_mm) && ok(footer_mm) && ok(row_mm) && row_mm >= 1.0) {
                return Err("Header/row/footer height is invalid (row must be at least 1 mm)".into());
            }
            if !roll {
                let fixed = header_mm
                    + if footer == FooterMode::EveryPage {
                        footer_mm
                    } else {
                        0.0
                    };
                if fixed + row_mm > self.height_mm + 0.01 {
                    return Err(format!(
                        "header + row{} ({} mm) is taller than the page ({} mm)",
                        if footer == FooterMode::EveryPage {
                            " + footer"
                        } else {
                            ""
                        },
                        fixed + row_mm,
                        self.height_mm
                    ));
                }
                if header_mm + row_mm + footer_mm > self.height_mm + 0.01 {
                    return Err("Header + row + footer is taller than the page".into());
                }
            }
        }
        Ok(())
    }

    /// Placeholder names clients should send, sorted; excludes the fields ix.print
    /// fills in itself ({{no}}, {{halaman}}, ...), see [`Template::auto_fields`].
    pub fn fields(&self) -> Vec<String> {
        let mut out = self.all_fields();
        out.retain(|f| !AUTO_FIELDS.contains(&f.as_str()));
        out
    }

    /// Automatic fields used by the design; clients may still send their own values.
    pub fn auto_fields(&self) -> Vec<String> {
        let mut out = self.all_fields();
        out.retain(|f| AUTO_FIELDS.contains(&f.as_str()));
        out
    }

    fn all_fields(&self) -> Vec<String> {
        let mut out = BTreeSet::new();
        for el in &self.elements {
            for s in el.templates() {
                out.extend(placeholders(s));
            }
        }
        out.into_iter().collect()
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Band {
    Header,
    Row,
    Footer,
}

/// One printed page of a list: which rows it holds and where the footer goes.
#[derive(Debug, PartialEq)]
struct ListPage {
    rows: std::ops::Range<usize>,
    /// Top of the footer band in mm, if the footer is on this page.
    footer_top: Option<f32>,
    height_mm: f32,
}

/// Splits `n` rows into pages (pure layout math).
fn paginate(
    n: usize,
    page_h: f32,
    header: f32,
    row: f32,
    footer_h: f32,
    mode: FooterMode,
    roll: bool,
) -> Vec<ListPage> {
    const EPS: f32 = 0.01;
    if roll {
        let height = header + n as f32 * row + footer_h;
        return vec![ListPage {
            rows: 0..n,
            footer_top: Some(header + n as f32 * row),
            height_mm: height,
        }];
    }
    let every = mode == FooterMode::EveryPage;
    let bottom = page_h - if every { footer_h } else { 0.0 };
    let per_page = (((bottom - header) + EPS) / row).floor().max(1.0) as usize;
    let mut pages = Vec::new();
    let mut start = 0;
    loop {
        let end = (start + per_page).min(n);
        pages.push(ListPage {
            rows: start..end,
            footer_top: every.then_some(page_h - footer_h),
            height_mm: page_h,
        });
        start = end;
        if start >= n {
            break;
        }
    }
    if !every && footer_h > 0.0 {
        let last = pages.last_mut().unwrap();
        let after = header + last.rows.len() as f32 * row;
        if after + footer_h <= page_h + EPS {
            last.footer_top = Some(after);
        } else {
            // No room left: the footer gets its own page, under the header.
            pages.push(ListPage {
                rows: n..n,
                footer_top: Some(header),
                height_mm: page_h,
            });
        }
    }
    pages
}

fn is_field_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')
}

/// Names inside `{{…}}` (surrounding spaces allowed).
fn placeholders(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim();
        if !name.is_empty() && name.chars().all(is_field_char) {
            out.push(name.to_string());
        }
        rest = &after[end + 2..];
    }
    out
}

/// Replaces `{{field}}` with the record's value; unknown fields become empty.
pub fn fill(s: &str, record: &Record) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) if after[..end].trim().chars().all(is_field_char) && !after[..end].trim().is_empty() => {
                out.push_str(record.get(after[..end].trim()).map(String::as_str).unwrap_or(""));
                rest = &after[end + 2..];
            }
            _ => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Converts a JSON object (strings, numbers, booleans, null) into a record.
pub fn record_from_json(value: &serde_json::Value) -> Result<Record, String> {
    let obj = value.as_object().ok_or("Each data item must be a JSON object")?;
    obj.iter()
        .map(|(k, v)| {
            let s = match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => String::new(),
                _ => return Err(format!("Value '{}' must be text/number, not an object/array", k)),
            };
            Ok((k.clone(), s))
        })
        .collect()
}

struct Fonts {
    regular: PdfFontToken,
    bold: PdfFontToken,
}

/// Lays text into its box: wrapping, shrinking to fit, alignment.
#[allow(clippy::too_many_arguments)]
fn draw_text(
    c: &mut Canvas,
    fonts: &Fonts,
    text: &str,
    (x, y, w, h): (f32, f32, f32, f32),
    size: f32,
    bold: bool,
    align: HAlign,
    valign: VAlign,
    do_wrap: bool,
    fit: bool,
) -> Result<(), PdfiumError> {
    let font = if bold { fonts.bold } else { fonts.regular };
    let layout = |size: f32| -> Vec<String> {
        text.split('\n')
            .flat_map(|para| {
                if do_wrap && !para.trim().is_empty() {
                    wrap(c, para, font, size, w)
                } else {
                    vec![para.to_string()]
                }
            })
            .collect()
    };
    let line_h = |size: f32| pt_to_mm(size) * 1.2;
    let mut size = size;
    let mut lines = layout(size);
    if fit {
        let fits = |lines: &[String], size: f32| {
            lines.len() as f32 * line_h(size) <= h + 0.01 && lines.iter().all(|l| c.measure(l, font, size) <= w + 0.01)
        };
        while size > 3.0 && !fits(&lines, size) {
            size = (size - 0.5).max(3.0);
            lines = layout(size);
        }
    }
    let block = lines.len() as f32 * line_h(size);
    let top = match valign {
        VAlign::Top => y,
        VAlign::Middle => y + (h - block) / 2.0,
        VAlign::Bottom => y + h - block,
    };
    for (i, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let lw = c.measure(line, font, size);
        let lx = match align {
            HAlign::Left => x,
            HAlign::Center => x + (w - lw) / 2.0,
            HAlign::Right => x + w - lw,
        };
        // Baseline sits about 0.8 em below the top of the line box.
        let baseline = top + i as f32 * line_h(size) + pt_to_mm(size) * 0.95;
        c.text(line, font, size, Some(lx), baseline)?;
    }
    Ok(())
}

/// Black rectangles for every run of dark modules in a row.
fn draw_runs(c: &mut Canvas, row: &[bool], x: f32, y: f32, module: f32, height: f32) -> Result<(), PdfiumError> {
    let mut i = 0;
    while i < row.len() {
        if row[i] {
            let start = i;
            while i < row.len() && row[i] {
                i += 1;
            }
            c.rect(
                x + start as f32 * module,
                y,
                (i - start) as f32 * module,
                height,
                None,
                Some(0),
            )?;
        } else {
            i += 1;
        }
    }
    Ok(())
}

fn draw_element(c: &mut Canvas, fonts: &Fonts, el: &Element, record: &Record) -> Result<(), String> {
    let pdf = |e: PdfiumError| e.to_string();
    match el {
        Element::Text {
            x,
            y,
            w,
            h,
            text,
            size,
            bold,
            align,
            valign,
            wrap,
            fit,
            ..
        } => {
            let text = fill(text, record);
            draw_text(
                c,
                fonts,
                &text,
                (*x, *y, *w, *h),
                *size,
                *bold,
                *align,
                *valign,
                *wrap,
                *fit,
            )
            .map_err(pdf)
        }
        Element::Line {
            x1, y1, x2, y2, width, ..
        } => c.line(*x1, *y1, *x2, *y2, *width, 0).map_err(pdf),
        Element::Rect {
            x,
            y,
            w,
            h,
            stroke,
            fill: filled,
            ..
        } => c
            .rect(
                *x,
                *y,
                *w,
                *h,
                (*stroke > 0.0).then_some((0, *stroke)),
                filled.then_some(0),
            )
            .map_err(pdf),
        Element::Barcode {
            x,
            y,
            w,
            h,
            value,
            show_text,
            ..
        } => {
            let value = fill(value, record);
            if value.is_empty() {
                return Ok(());
            }
            if !value.chars().all(|ch| (' '..='~').contains(&ch)) {
                return Err(format!("Barcode '{}' may contain ASCII characters only", value));
            }
            let modules = barcoders::sym::code128::Code128::new(format!("Ɓ{}", value))
                .map_err(|e| format!("barcode '{}': {}", value, e))?
                .encode();
            let text_h = if *show_text { (h * 0.25).clamp(2.0, 5.0) } else { 0.0 };
            let bars_h = (h - text_h).max(1.0);
            let module = w / modules.len() as f32;
            let row: Vec<bool> = modules.iter().map(|m| *m == 1).collect();
            draw_runs(c, &row, *x, *y, module, bars_h).map_err(pdf)?;
            if *show_text {
                let size = mm_to_pt(text_h * 0.8);
                draw_text(
                    c,
                    fonts,
                    &value,
                    (*x, y + bars_h, *w, text_h),
                    size,
                    false,
                    HAlign::Center,
                    VAlign::Middle,
                    false,
                    true,
                )
                .map_err(pdf)?;
            }
            Ok(())
        }
        Element::Image {
            x,
            y,
            w,
            h,
            src,
            fit,
            grayscale,
            ..
        } => {
            let src = fill(src, record);
            if src.trim().is_empty() {
                return Ok(());
            }
            let img = decode_image(&src, *grayscale)?;
            let (iw, ih) = (img.width().max(1) as f32, img.height().max(1) as f32);
            // Box to draw into, in mm from the top-left.
            let (dx, dy, dw, dh) = match fit {
                ImageFit::Stretch => (*x, *y, *w, *h),
                ImageFit::Contain => {
                    let s = (w / iw).min(h / ih);
                    let (dw, dh) = (iw * s, ih * s);
                    (x + (w - dw) / 2.0, y + (h - dh) / 2.0, dw, dh)
                }
            };
            let mut obj = PdfPageImageObject::new_with_size(
                c.doc,
                &img,
                PdfPoints::new(mm_to_pt(dw)),
                PdfPoints::new(mm_to_pt(dh)),
            )
            .map_err(pdf)?;
            let (px, py) = c.pt(dx, dy + dh);
            obj.translate(px, py).map_err(pdf)?;
            c.page.objects_mut().add_image_object(obj).map_err(pdf)?;
            Ok(())
        }
        Element::Qr { x, y, size, value, .. } => {
            let value = fill(value, record);
            if value.is_empty() {
                return Ok(());
            }
            let code = qrcode::QrCode::with_error_correction_level(value.as_bytes(), qrcode::EcLevel::M)
                .map_err(|e| format!("QR: {}", e))?;
            let n = code.width();
            let colors = code.to_colors();
            let module = size / n as f32;
            for (r, row) in colors.chunks(n).enumerate() {
                let dark: Vec<bool> = row.iter().map(|m| *m == qrcode::Color::Dark).collect();
                draw_runs(c, &dark, *x, y + r as f32 * module, module, module).map_err(pdf)?;
            }
            Ok(())
        }
    }
}

/// Decodes a data URL / base64 image, flattening transparency onto white
/// (PDF image objects would otherwise print transparent areas black).
fn decode_image(src: &str, grayscale: bool) -> Result<image::DynamicImage, String> {
    use base64::Engine;
    let b64 = match src.split_once(";base64,") {
        Some((head, data)) if head.starts_with("data:") => data,
        _ => src,
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|_| "Image is not a valid data URL/base64 value".to_string())?;
    // Clients send these images: a small file claiming huge dimensions must not
    // be able to exhaust memory while decoding.
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8000);
    limits.max_image_height = Some(8000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    reader.limits(limits);
    let img = reader.decode().map_err(|e| {
		format!("Image could not be read (PNG, JPEG, GIF, BMP, WebP, TIFF, ICO; SVG is converted in the editor first; max. 8000×8000 px): {}", e)
    })?;
    let mut rgba = img.to_rgba8();
    for p in rgba.pixels_mut() {
        let a = p[3] as u32;
        for c in 0..3 {
            p[c] = ((p[c] as u32 * a + 255 * (255 - a)) / 255) as u8;
        }
        if grayscale {
            let l = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32).round() as u8;
            p[0] = l;
            p[1] = l;
            p[2] = l;
        }
        p[3] = 255;
    }
    Ok(image::DynamicImage::ImageRgb8(
        image::DynamicImage::ImageRgba8(rgba).to_rgb8(),
    ))
}

/// Values for one element: automatic fields, overridden by the shared header
/// values, overridden by the record. Clients may send their own {{no}}, {{halaman}}...
fn values(header: &Record, record: Option<&Record>, auto: [(&str, usize); 4]) -> Record {
    let mut v: Record = auto.iter().map(|(k, n)| (k.to_string(), n.to_string())).collect();
    // An empty value for an automatic field means "not given", not "print nothing".
    let given = |(k, val): (&String, &String)| {
        (!(val.is_empty() && AUTO_FIELDS.contains(&k.as_str()))).then(|| (k.clone(), val.clone()))
    };
    v.extend(header.iter().filter_map(given));
    if let Some(r) = record {
        v.extend(r.iter().filter_map(given));
    }
    v
}

/// Builds the PDF: one page per record (label) or rows paginated under a header (list).
fn build<'a>(
    pdfium: &'a Pdfium,
    template: &Template,
    records: &[Record],
    header: &Record,
) -> Result<PdfDocument<'a>, String> {
    let mut doc = pdfium.create_new_pdf().map_err(|e| e.to_string())?;
    let fonts = Fonts {
        regular: doc.fonts_mut().helvetica(),
        bold: doc.fonts_mut().helvetica_bold(),
    };
    let w = template.width_mm;
    let n = records.len();
    let err = |i: usize, e: String| format!("elemen #{}: {}", i + 1, e);

    // (page height, list of (element, dy, values)) per page
    let pages: Vec<(f32, Vec<(usize, f32, Record)>)> = match template.layout {
        Layout::Label => records
            .iter()
            .enumerate()
            .map(|(p, r)| {
                let v = values(
                    header,
                    Some(r),
                    [
                        ("no", p + 1),
                        ("halaman", p + 1),
                        ("total_halaman", n),
                        ("jumlah_data", n),
                    ],
                );
                (
                    template.height_mm,
                    (0..template.elements.len()).map(|i| (i, 0.0, v.clone())).collect(),
                )
            })
            .collect(),
        Layout::List {
            header_mm,
            row_mm,
            footer_mm,
            footer,
            roll,
        } => {
            let band = |el: &Element| {
                let top = el.top();
                if top < header_mm - 0.001 {
                    Band::Header
                } else if top < header_mm + row_mm - 0.001 {
                    Band::Row
                } else {
                    Band::Footer
                }
            };
            let layout = paginate(n, template.height_mm, header_mm, row_mm, footer_mm, footer, roll);
            let total = layout.len();
            layout
                .iter()
                .enumerate()
                .map(|(p, page)| {
                    let auto = |no: usize| {
                        [
                            ("no", no),
                            ("halaman", p + 1),
                            ("total_halaman", total),
                            ("jumlah_data", n),
                        ]
                    };
                    let shared = values(header, None, auto(0));
                    let mut items = Vec::new();
                    for (i, el) in template.elements.iter().enumerate() {
                        match band(el) {
                            Band::Header => items.push((i, 0.0, shared.clone())),
                            Band::Row => {
                                for (k, r) in page.rows.clone().enumerate() {
                                    let dy = k as f32 * row_mm;
                                    items.push((i, dy, values(header, Some(&records[r]), auto(r + 1))));
                                }
                            }
                            Band::Footer => {
                                if let Some(top) = page.footer_top {
                                    items.push((i, top - (header_mm + row_mm), shared.clone()));
                                }
                            }
                        }
                    }
                    (page.height_mm, items)
                })
                .collect()
        }
    };

    for (height, items) in pages {
        let page = doc
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::Custom(
                PdfPoints::new(mm_to_pt(w)),
                PdfPoints::new(mm_to_pt(height)),
            ))
            .map_err(|e| e.to_string())?;
        let mut c = Canvas {
            doc: &doc,
            page,
            width_mm: w,
            height_mm: height,
        };
        for (i, dy, vals) in items {
            let el = &template.elements[i];
            let el = if dy == 0.0 { el.clone() } else { el.shifted(dy) };
            draw_element(&mut c, &fonts, &el, &vals).map_err(|e| err(i, e))?;
        }
        c.page.regenerate_content().map_err(|e| e.to_string())?;
    }
    Ok(doc)
}

/// Writes the filled design to `path`. `header` holds values shared by every page.
pub fn render(path: &Path, template: &Template, records: &[Record], header: &Record) -> Result<(), String> {
    template.validate()?;
    if records.is_empty() {
        return Err("Data is empty: send at least one record".into());
    }
    if records.len() > MAX_RECORDS {
        return Err(format!("maksimal {} record per job", MAX_RECORDS));
    }
    let pdfium = pdfium()?;
    let doc = build(&pdfium, template, records, header)?;
    doc.save_to_file(path)
        .map_err(|e| format!("Gagal menyimpan PDF template: {}", e))
}

/// The first `max_pages` printed pages as PNGs, for a print preview.
pub fn pages_png(
    template: &Template,
    records: &[Record],
    header: &Record,
    max_pages: usize,
    px_per_mm: f32,
) -> Result<(Vec<Vec<u8>>, usize), String> {
    template.validate()?;
    let pdfium = pdfium()?;
    let doc = build(&pdfium, template, records, header)?;
    let total = doc.pages().len() as usize;
    let pages = (0..total.min(max_pages))
        .map(|i| render_page_png(&doc, i, px_per_mm))
        .collect::<Result<_, _>>()?;
    Ok((pages, total))
}

/// The editor canvas: a label page, or for lists the bands (header, one row,
/// footer) stacked, so element positions match the editor 1:1.
pub fn preview_png(template: &Template, record: &Record, px_per_mm: f32) -> Result<Vec<u8>, String> {
    template.validate()?;
    let mut t = template.clone();
    if let Layout::List {
        header_mm,
        row_mm,
        footer_mm,
        ..
    } = t.layout
    {
        t.layout = Layout::List {
            header_mm,
            row_mm,
            footer_mm,
            footer: FooterMode::LastPage,
            roll: true,
        };
    }
    let pdfium = pdfium()?;
    let doc = build(&pdfium, &t, std::slice::from_ref(record), record)?;
    render_page_png(&doc, 0, px_per_mm)
}

pub(crate) fn render_page_png(doc: &PdfDocument, index: usize, px_per_mm: f32) -> Result<Vec<u8>, String> {
    let page = doc.pages().get(index as PdfPageIndex).map_err(|e| e.to_string())?;
    let w = (crate::settings::pt_to_mm(page.width().value) * px_per_mm)
        .round()
        .clamp(1.0, 4000.0) as i32;
    let h = (crate::settings::pt_to_mm(page.height().value) * px_per_mm)
        .round()
        .clamp(1.0, 4000.0) as i32;
    let rgba = page
        .render_with_config(&PdfRenderConfig::new().set_fixed_size(w, h))
        .map_err(|e| e.to_string())?
        .as_rgba_bytes();

    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, w as u32, h as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4×4 image, left half red, right half transparent, as a data URL in `format`.
    fn half_red(format: image::ImageFormat) -> String {
        use base64::Engine;
        let img = image::RgbaImage::from_fn(4, 4, |x, _| {
            if x < 2 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 0, 0])
            }
        });
        let img = if format == image::ImageFormat::Jpeg {
            image::DynamicImage::ImageRgb8(image::DynamicImage::ImageRgba8(img).to_rgb8())
        } else {
            image::DynamicImage::ImageRgba8(img)
        };
        let mut bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut bytes, format).unwrap();
        format!(
            "data:image/x;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
        )
    }

    fn sample() -> Template {
        serde_json::from_value(serde_json::json!({
            "width_mm": 60, "height_mm": 40,
            "elements": [
                { "type": "text", "x": 2, "y": 2, "w": 56, "h": 8, "text": "Nama: {{nama}}", "size": 12, "bold": true },
                { "type": "text", "x": 2, "y": 11, "w": 56, "h": 6, "text": "RM {{ rm }} / {{unknown}}", "align": "right" },
                { "type": "line", "x1": 2, "y1": 18, "x2": 58, "y2": 18 },
                { "type": "rect", "x": 1, "y": 1, "w": 58, "h": 38 },
                { "type": "barcode", "x": 2, "y": 20, "w": 36, "h": 15, "value": "{{rm}}" },
                { "type": "qr", "x": 42, "y": 20, "size": 15, "value": "RM:{{rm}}" },
                { "type": "image", "x": 50, "y": 2, "w": 8, "h": 8, "src": half_red(image::ImageFormat::Png) }
            ]
        }))
        .unwrap()
    }

    fn record(pairs: &[(&str, &str)]) -> Record {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn placeholders_and_fill() {
        let t = sample();
        assert_eq!(t.fields(), vec!["nama", "rm", "unknown"]);
        let r = record(&[("nama", "Chandra AP"), ("rm", "303030")]);
        assert_eq!(
            fill("Nama: {{nama}}, RM {{ rm }}{{x}}", &r),
            "Nama: Chandra AP, RM 303030"
        );
        assert_eq!(fill("{{ bad name }} {{", &r), "{{ bad name }} {{");
        let from_json = record_from_json(&serde_json::json!({"a": 1, "b": true, "c": null, "d": "x"})).unwrap();
        assert_eq!(from_json, record(&[("a", "1"), ("b", "true"), ("c", ""), ("d", "x")]));
        assert!(record_from_json(&serde_json::json!({"a": [1]})).is_err());
    }

    #[test]
    fn renders_pages_and_preview() {
        crate::layout::tests::init_for_tests();
        let dir = std::env::temp_dir().join(format!("ixprint-tpl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let t = sample();
        let records = vec![
            record(&[("nama", "A"), ("rm", "1001")]),
            record(&[("nama", "B"), ("rm", "1002")]),
        ];
        let path = dir.join("t.pdf");
        render(&path, &t, &records, &Record::new()).unwrap();
        {
            let pdfium = pdfium().unwrap();
            let doc = pdfium.load_pdf_from_file(&path, None).unwrap();
            assert_eq!(doc.pages().len(), 2);
            let page = doc.pages().get(1).unwrap();
            assert!((pt_to_mm(page.width().value) - 60.0).abs() < 0.1);
            let text = page.text().unwrap().all();
            assert!(text.contains("Nama: B") && text.contains("RM 1002"), "{}", text);
        }
        let png = preview_png(&t, &records[0], 8.0).unwrap();
        assert_eq!(&png[1..4], b"PNG");

        // Non-ASCII barcode is a clear error, not a crash.
        let r = record(&[("rm", "é")]);
        assert!(render(&dir.join("bad.pdf"), &t, &[r], &Record::new())
            .unwrap_err()
            .contains("ASCII"));
        // Long text shrinks to fit instead of overflowing.
        let long = record(&[("nama", &"Nama Sangat Panjang ".repeat(10)), ("rm", "1")]);
        render(&dir.join("long.pdf"), &t, &[long], &Record::new()).unwrap();
        if let Ok(out) = std::env::var("IXPRINT_VISUAL_OUT") {
            std::fs::write(Path::new(&out).join("template.png"), &png).unwrap();
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn images_in_any_format() {
        crate::layout::tests::init_for_tests();
        let formats = [
            image::ImageFormat::Png,
            image::ImageFormat::Jpeg,
            image::ImageFormat::Gif,
            image::ImageFormat::Bmp,
            image::ImageFormat::WebP,
            image::ImageFormat::Tiff,
            image::ImageFormat::Ico,
        ];
        for f in formats {
            let t: Template = serde_json::from_value(serde_json::json!({
                "width_mm": 20, "height_mm": 20,
                "elements": [{ "type": "image", "x": 0, "y": 0, "w": 20, "h": 20, "src": half_red(f), "fit": "stretch" }]
            }))
            .unwrap();
            let png = preview_png(&t, &Record::new(), 4.0).unwrap();
            let img = image::load_from_memory(&png).unwrap().to_rgb8();
            let left = img.get_pixel(10, 40);
            let right = img.get_pixel(70, 40);
            assert!(
                left[0] > 200 && left[1] < 80,
                "{:?}: left should be red, got {:?}",
                f,
                left
            );
            // Transparency becomes white, not black.
            if f != image::ImageFormat::Jpeg {
                assert!(
                    right[0] > 200 && right[1] > 200 && right[2] > 200,
                    "{:?}: right should be white, got {:?}",
                    f,
                    right
                );
            }
        }
        // Grayscale option and per-record images via a {{field}}.
        let t: Template = serde_json::from_value(serde_json::json!({
            "width_mm": 20, "height_mm": 20,
            "elements": [{ "type": "image", "x": 0, "y": 0, "w": 20, "h": 20, "src": "{{foto}}", "fit": "stretch", "grayscale": true }]
        }))
        .unwrap();
        assert_eq!(t.fields(), vec!["foto"]);
        let mut r = Record::new();
        r.insert("foto".into(), half_red(image::ImageFormat::Png));
        let img = image::load_from_memory(&preview_png(&t, &r, 4.0).unwrap())
            .unwrap()
            .to_rgb8();
        let p = img.get_pixel(10, 40);
        assert!(p[0] == p[1] && p[1] == p[2], "grayscale expected, got {:?}", p);
        // Garbage is a clear error.
        r.insert("foto".into(), "data:image/png;base64,bm90IGFuIGltYWdl".into());
        assert!(preview_png(&t, &r, 4.0).unwrap_err().contains("Image"));
    }

    #[test]
    fn paginates_lists() {
        // 100 mm page, 20 header, 10 per row → 8 rows per page.
        let p = paginate(20, 100.0, 20.0, 10.0, 15.0, FooterMode::LastPage, false);
        assert_eq!(
            p.iter().map(|x| x.rows.clone()).collect::<Vec<_>>(),
            vec![0..8, 8..16, 16..20]
        );
        assert_eq!(p[0].footer_top, None);
        assert_eq!(p[2].footer_top, Some(60.0)); // right after the 4 rows
                                                 // Footer that doesn't fit after the last row gets its own page.
        let p = paginate(8, 100.0, 20.0, 10.0, 15.0, FooterMode::LastPage, false);
        assert_eq!(p.len(), 2);
        assert_eq!((p[1].rows.clone(), p[1].footer_top), (8..8, Some(20.0)));
        // Footer on every page reserves its space: 6 rows per page.
        let p = paginate(13, 100.0, 20.0, 10.0, 15.0, FooterMode::EveryPage, false);
        assert_eq!(p.iter().map(|x| x.rows.len()).collect::<Vec<_>>(), vec![6, 6, 1]);
        assert!(p.iter().all(|x| x.footer_top == Some(85.0)));
        // Roll: one page as tall as the content.
        let p = paginate(5, 0.0, 20.0, 10.0, 15.0, FooterMode::LastPage, true);
        assert_eq!((p.len(), p[0].height_mm, p[0].footer_top), (1, 85.0, Some(70.0)));
    }

    #[test]
    fn renders_lists_with_repeated_header() {
        crate::layout::tests::init_for_tests();
        let t: Template = serde_json::from_value(serde_json::json!({
            "width_mm": 80, "height_mm": 60,
            "layout": { "mode": "list", "header_mm": 15, "row_mm": 8, "footer_mm": 10 },
            "elements": [
                { "type": "text", "x": 2, "y": 2, "w": 76, "h": 6, "text": "Unit {{unit}} hal {{halaman}}/{{total_halaman}}" },
                { "type": "text", "x": 2, "y": 16, "w": 76, "h": 6, "text": "{{no}}. {{nama}}" },
                { "type": "text", "x": 2, "y": 24, "w": 76, "h": 6, "text": "Total {{jumlah_data}} {{total}}" }
            ]
        }))
        .unwrap();
        assert_eq!(t.fields(), vec!["nama", "total", "unit"]);
        let records: Vec<Record> = (1..=7)
            .map(|i| [("nama".to_string(), format!("Pasien{}", i))].into())
            .collect();
        let header: Record = [
            ("unit".to_string(), "Anak".to_string()),
            ("total".to_string(), "Rp9".to_string()),
        ]
        .into();

        let pdfium = pdfium().unwrap();
        let doc = build(&pdfium, &t, &records, &header).unwrap();
        // (60 - 15) / 8 = 5 rows on page 1, 2 on page 2, footer after them.
        assert_eq!(doc.pages().len(), 2);
        let text = |i: i32| doc.pages().get(i).unwrap().text().unwrap().all();
        let (p1, p2) = (text(0), text(1));
        assert!(
            p1.contains("Unit Anak hal 1/2") && p2.contains("Unit Anak hal 2/2"),
            "{}\n---\n{}",
            p1,
            p2
        );
        assert!(p1.contains("1. Pasien1") && p1.contains("5. Pasien5") && !p1.contains("Pasien6"));
        assert!(p2.contains("6. Pasien6") && p2.contains("7. Pasien7"));
        assert!(!p1.contains("Total") && p2.contains("Total 7 Rp9"));

        drop(doc);
        if let Ok(out) = std::env::var("IXPRINT_VISUAL_OUT") {
            let mut framed = t.clone();
            framed.elements.push(
                serde_json::from_value(serde_json::json!(
                { "type": "line", "x1": 0, "y1": 14.5, "x2": 80, "y2": 14.5, "width": 0.5 }))
                .unwrap(),
            );
            // Reuses the lock held by `pdfium` above (it is not re-entrant).
            let doc = build(&pdfium, &framed, &records, &header).unwrap();
            for i in 0..doc.pages().len() as usize {
                let png = render_page_png(&doc, i, 6.0).unwrap();
                std::fs::write(Path::new(&out).join(format!("list-{}.png", i + 1)), png).unwrap();
            }
        }
        // Label mode: header values reach every page, records override them.
        let mut label = t.clone();
        label.layout = Layout::Label;
        let doc = build(&pdfium, &label, &records[..2], &header).unwrap();
        assert_eq!(doc.pages().len(), 2);
        assert!(doc
            .pages()
            .get(1)
            .unwrap()
            .text()
            .unwrap()
            .all()
            .contains("Unit Anak hal 2/2"));

        // User values win over automatic ones.
        let mut numbered = records[..2].to_vec();
        numbered[1].insert("no".into(), "B-02".into());
        numbered[0].insert("no".into(), String::new()); // empty = automatic
        let custom_header: Record = [
            ("unit".to_string(), "Anak".to_string()),
            ("total_halaman".to_string(), "9".to_string()),
        ]
        .into();
        let doc = build(&pdfium, &t, &numbered, &custom_header).unwrap();
        let p1 = doc.pages().get(0).unwrap().text().unwrap().all();
        assert!(
            p1.contains("1. Pasien1") && p1.contains("B-02. Pasien2") && p1.contains("hal 1/9"),
            "{}",
            p1
        );
        assert_eq!(t.auto_fields(), vec!["halaman", "jumlah_data", "no", "total_halaman"]);
        drop(doc);

        // Too-tall bands are rejected.
        let mut bad = t.clone();
        bad.layout = Layout::List {
            header_mm: 50.0,
            row_mm: 20.0,
            footer_mm: 0.0,
            footer: FooterMode::LastPage,
            roll: false,
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn rejects_bad_templates() {
        let mut t = sample();
        t.width_mm = 1.0;
        assert!(t.validate().is_err());
        let mut t = sample();
        if let Element::Text { size, .. } = &mut t.elements[0] {
            *size = 0.0;
        }
        assert!(t.validate().is_err());
    }
}
