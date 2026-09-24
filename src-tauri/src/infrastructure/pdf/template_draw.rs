use crate::domain::settings::{mm_to_pt, pt_to_mm};
use crate::domain::template::{fill, Element, HAlign, ImageFit, Record, VAlign};
use crate::infrastructure::pdf::canvas::{wrap, Canvas};
use pdfium_render::prelude::*;

pub(super) struct Fonts {
    pub(super) regular: PdfFontToken,
    pub(super) bold: PdfFontToken,
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

pub(super) fn draw_element(c: &mut Canvas, fonts: &Fonts, el: &Element, record: &Record) -> Result<(), String> {
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
