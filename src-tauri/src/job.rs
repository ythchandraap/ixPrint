//! Compatibility exports for the print job model and use case.

pub use crate::application::job_service::run;
pub use crate::application::state::AppState;
pub use crate::domain::job::{Document, JobError, JobRequest, JobResult};

#[cfg(test)]
use crate::printer;
#[cfg(test)]
use crate::settings::{JobSettings, PrintSettings};
#[cfg(test)]
use crate::testpage;

#[cfg(test)]
fn check_paper_fits(job: &crate::settings::JobSettings) -> Result<(), String> {
    let (w, h) = match job.paper {
        crate::settings::Paper::Custom { width_mm, height_mm } => (width_mm, Some(height_mm)),
        crate::settings::Paper::Roll { width_mm } => (width_mm, None),
        _ => return Ok(()),
    };
    let Ok(caps) = printer::capabilities(&job.printer) else {
        return Ok(());
    };
    let too_wide = caps.max_width_mm.is_some_and(|max| w > max + 0.5);
    let too_long = matches!((h, caps.max_height_mm), (Some(h), Some(max)) if h > max + 0.5);
    if too_wide || too_long {
        let max = |v: Option<f32>| v.map(|m| format!("{:.0}", m)).unwrap_or_else(|| "?".into());
        return Err(format!(
            "Paper size {} is larger than printer '{}' maximum ({} × {} mm). Check the units (mm).",
            job.paper,
            job.printer,
            max(caps.max_width_mm),
            max(caps.max_height_mm)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::job_service::{test_page_info, test_sheet_mm};
    use crate::layout;
    use crate::settings::{pt_to_mm, Margins, Orientation, Scale};
    use pdfium_render::prelude::*;

    /// The calibration page must come out of the layout engine at the sheet size,
    /// with its frame exactly on the paper edge.
    #[test]
    fn test_page_frame_lands_on_sheet_edge() {
        crate::layout::tests::init_for_tests();
        let state = crate::application::state::test_support::state();
        let dir = std::env::temp_dir().join(format!("ixprint-tp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let cases = [
            (
                r#"{"paper":"A4","orientation":"landscape","margins":"10","scale":"fit"}"#,
                (297.0, 210.0),
            ),
            (r#"{"paper":"80mm"}"#, (80.0, 80.0)),
            (r#"{"paper":"100x150mm","orientation":"auto"}"#, (100.0, 150.0)),
        ];
        for (i, (json, (ew, eh))) in cases.iter().enumerate() {
            let s: PrintSettings = serde_json::from_str(json).unwrap();
            let mut job = JobSettings::resolve(&s, "test".into());
            let (w, h) = test_sheet_mm(&job, (210.0, 297.0));
            let src = dir.join(format!("t{}.pdf", i));
            testpage::generate(&src, w, h, &test_page_info(&state, &job, None, w, h), 1).unwrap();
            job.scale = Scale::None;
            job.margins = Margins::default();
            job.orientation = Orientation::Auto;
            let out = dir.join(format!("o{}.pdf", i));
            let result = layout::build(&src, &out, &job, (210.0, 297.0)).unwrap();
            let sheet = result.sheets[0];
            assert!((pt_to_mm(sheet.width_pt) - ew).abs() < 0.2, "{} width", json);
            assert!((pt_to_mm(sheet.height_pt) - eh).abs() < 0.2, "{} height", json);

            // Render at 4 px/pt: the outermost pixels on every side must be the black frame.
            let pdfium = layout::pdfium().unwrap();
            let doc = pdfium.load_pdf_from_file(&out, None).unwrap();
            let page = doc.pages().get(0).unwrap();
            let (pw, ph) = ((sheet.width_pt * 4.0) as i32, (sheet.height_pt * 4.0) as i32);
            let bmp = page
                .render_with_config(&PdfRenderConfig::new().set_fixed_size(pw, ph))
                .unwrap();
            let px = bmp.as_rgba_bytes();
            let dark = |x: i32, y: i32| px[((y * pw + x) * 4) as usize] < 80;
            for (x, y) in [(1, ph / 3), (pw - 2, ph / 3), (pw / 3, 1), (pw / 3, ph - 2)] {
                assert!(dark(x, y), "{}: frame missing at ({}, {})", json, x, y);
            }
            // ...and just inside the frame it is white again.
            assert!(!dark(12, ph / 3), "{}: frame too thick", json);
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod live_probe {
    use super::*;

    /// `ixPrint_PROBE_PRINTER=<name> cargo test --lib paper_limits -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn paper_limits() {
        let printer = std::env::var("ixPrint_PROBE_PRINTER").unwrap();
        let caps = printer::capabilities(&printer).unwrap();
        println!("max: {:?} x {:?} mm", caps.max_width_mm, caps.max_height_mm);
        for paper in ["600x750mm", "60x75mm", "80mm", "300mm"] {
            let s = PrintSettings {
                paper: Some(paper.parse().unwrap()),
                ..Default::default()
            };
            println!(
                "{}: {:?}",
                paper,
                check_paper_fits(&JobSettings::resolve(&s, printer.clone()))
            );
        }
    }
}
