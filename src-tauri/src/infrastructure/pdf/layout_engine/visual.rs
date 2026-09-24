use super::*;
use crate::domain::settings::PrintSettings;

/// Writes print-ready PDFs of the sample document to $IXPRINT_VISUAL_OUT for eyeballing.
#[test]
#[ignore]
fn sample_layouts() {
    tests::init_for_tests();
    let out = PathBuf::from(std::env::var("IXPRINT_VISUAL_OUT").unwrap());
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assets/RSUD.pdf");
    let cases: &[(&str, &str)] = &[
        ("a4-auto", r#"{}"#),
        ("a4-portrait", r#"{"paper":"A4","orientation":"portrait"}"#),
        ("roll80-fit", r#"{"paper":"80mm","scale":"fit","margins":"2"}"#),
        (
            "label-100x150",
            r#"{"paper":"100x150mm","scale":"fit","orientation":"portrait","margins":"3"}"#,
        ),
        (
            "a5-topleft-offset",
            r#"{"paper":"A5","align":"top-left","offset_x":5,"offset_y":5,"scale":"80%"}"#,
        ),
    ];
    for (name, json) in cases {
        let s: PrintSettings = serde_json::from_str(json).unwrap();
        let job = JobSettings::resolve(&s, "test".into());
        let r = build(&src, &out.join(format!("{}.pdf", name)), &job, (210.0, 297.0)).unwrap();
        println!(
            "{}: {} sheet(s), first {:.1}x{:.1}mm",
            name,
            r.sheets.len(),
            crate::domain::settings::pt_to_mm(r.sheets[0].width_pt),
            crate::domain::settings::pt_to_mm(r.sheets[0].height_pt)
        );
    }
}
