pub mod canvas;
pub mod layout_engine;
mod template_draw;
pub mod template_render;
pub mod test_page;

use crate::application::ports::PdfEngine;
use crate::domain::layout::LayoutResult;
use crate::domain::settings::JobSettings;
use crate::domain::template::{Record, Template};
use std::path::Path;

#[derive(Default)]
pub struct PdfEngineAdapter;

impl PdfEngine for PdfEngineAdapter {
    fn engine_status(&self) -> Result<(), String> {
        layout_engine::pdfium().map(|_| ())
    }

    fn build_layout(
        &self,
        source: &Path,
        output: &Path,
        settings: &JobSettings,
        default_paper_mm: (f32, f32),
    ) -> Result<LayoutResult, String> {
        layout_engine::build(source, output, settings, default_paper_mm)
    }

    fn render_template(
        &self,
        path: &Path,
        template: &Template,
        records: &[Record],
        header: &Record,
    ) -> Result<(), String> {
        template_render::render(path, template, records, header)
    }

    fn pages_png(
        &self,
        template: &Template,
        records: &[Record],
        header: &Record,
        max_pages: usize,
        px_per_mm: f32,
    ) -> Result<(Vec<Vec<u8>>, usize), String> {
        template_render::pages_png(template, records, header, max_pages, px_per_mm)
    }

    fn preview_png(&self, template: &Template, record: &Record, px_per_mm: f32) -> Result<Vec<u8>, String> {
        template_render::preview_png(template, record, px_per_mm)
    }

    fn render_page_png(&self, path: &Path, page: usize, px_per_mm: f32) -> Result<Vec<u8>, String> {
        let pdfium = layout_engine::pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(path, None)
            .map_err(|e| format!("Could not open the laid-out sheet: {}", e))?;
        template_render::render_page_png(&doc, page, px_per_mm)
    }

    fn test_page(&self, path: &Path, width_mm: f32, height_mm: f32, info: &[String], pages: u32) -> Result<(), String> {
        test_page::generate(path, width_mm, height_mm, info, pages)
    }

    fn remove_generated_file(&self, path: &Path) {
        let _ = std::fs::remove_file(path);
    }
}
