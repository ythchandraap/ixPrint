use crate::domain::settings::{mm_to_pt, pt_to_mm};
use pdfium_render::prelude::*;

pub struct Canvas<'a, 'b> {
    pub doc: &'b PdfDocument<'a>,
    pub page: PdfPage<'a>,
    pub width_mm: f32,
    pub height_mm: f32,
}

impl Canvas<'_, '_> {
    /// Page coordinates in points from mm measured from the top-left corner.
    pub fn pt(&self, x_mm: f32, y_mm: f32) -> (PdfPoints, PdfPoints) {
        (
            PdfPoints::new(mm_to_pt(x_mm)),
            PdfPoints::new(mm_to_pt(self.height_mm - y_mm)),
        )
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width_pt: f32, gray: u8) -> Result<(), PdfiumError> {
        let (ax, ay) = self.pt(x1, y1);
        let (bx, by) = self.pt(x2, y2);
        let color = PdfColor::new(gray, gray, gray, 255);
        let obj = PdfPagePathObject::new_line(self.doc, ax, ay, bx, by, color, PdfPoints::new(width_pt))?;
        self.page.objects_mut().add_path_object(obj)?;
        Ok(())
    }

    /// Rectangle in mm from the top-left; `fill` and/or `stroke` as gray levels.
    pub fn rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        stroke: Option<(u8, f32)>,
        fill: Option<u8>,
    ) -> Result<(), PdfiumError> {
        let (left, top) = self.pt(x, y);
        let (right, bottom) = self.pt(x + w, y + h);
        let gray = |g: u8| PdfColor::new(g, g, g, 255);
        let obj = PdfPagePathObject::new_rect(
            self.doc,
            PdfRect::new(bottom, left, top, right),
            stroke.map(|(g, _)| gray(g)),
            stroke.map(|(_, w)| PdfPoints::new(w)),
            fill.map(gray),
        )?;
        self.page.objects_mut().add_path_object(obj)?;
        Ok(())
    }

    /// Draws `text` with its baseline at `y_mm`; `x_mm = None` centers it.
    /// Returns the text width in mm.
    pub fn text(
        &mut self,
        text: &str,
        font: PdfFontToken,
        size_pt: f32,
        x_mm: Option<f32>,
        y_mm: f32,
    ) -> Result<f32, PdfiumError> {
        let mut obj = PdfPageTextObject::new(self.doc, text, font, PdfPoints::new(size_pt))?;
        let width_mm = pt_to_mm(obj.width()?.value);
        let x = x_mm.unwrap_or((self.width_mm - width_mm) / 2.0);
        let (px, py) = self.pt(x, y_mm);
        obj.translate(px, py)?;
        self.page.objects_mut().add_text_object(obj)?;
        Ok(width_mm)
    }

    /// Width of `text` in mm without drawing it.
    pub fn measure(&self, text: &str, font: PdfFontToken, size_pt: f32) -> f32 {
        PdfPageTextObject::new(self.doc, text, font, PdfPoints::new(size_pt))
            .and_then(|o| o.width())
            .map(|w| pt_to_mm(w.value))
            .unwrap_or(0.0)
    }
}

/// Greedy word wrap to `max_mm`.
pub fn wrap(canvas: &Canvas, text: &str, font: PdfFontToken, size: f32, max_mm: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", current, word)
        };
        if !current.is_empty() && canvas.measure(&candidate, font, size) > max_mm {
            lines.push(std::mem::replace(&mut current, word.to_string()));
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}
