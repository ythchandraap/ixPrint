use crate::domain::settings::{JobSettings, Paper, PrintSettings};

fn check_paper_fits(job: &JobSettings) -> Result<(), String> {
    let (w, h) = match job.paper {
        Paper::Custom { width_mm, height_mm } => (width_mm, Some(height_mm)),
        Paper::Roll { width_mm } => (width_mm, None),
        _ => return Ok(()),
    };
    let Ok(caps) = super::capabilities(&job.printer) else {
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

#[test]
#[ignore]
fn paper_limits() {
    let printer = std::env::var("ixPrint_PROBE_PRINTER").unwrap();
    let caps = super::capabilities(&printer).unwrap();
    println!("max: {:?} x {:?} mm", caps.max_width_mm, caps.max_height_mm);
    for paper in ["600x750mm", "60x75mm", "80mm", "300mm"] {
        let settings = PrintSettings {
            paper: Some(paper.parse().unwrap()),
            ..Default::default()
        };
        println!(
            "{}: {:?}",
            paper,
            check_paper_fits(&JobSettings::resolve(&settings, printer.clone()))
        );
    }
}
