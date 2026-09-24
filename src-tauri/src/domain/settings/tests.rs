use std::collections::HashMap;

use super::*;

#[test]
fn paper_parsing() {
    assert_eq!("a4".parse::<Paper>().unwrap(), Paper::Standard("A4"));
    assert_eq!("".parse::<Paper>().unwrap(), Paper::PrinterDefault);
    assert_eq!("80mm".parse::<Paper>().unwrap(), Paper::Roll { width_mm: 80.0 });
    assert_eq!("roll:58".parse::<Paper>().unwrap(), Paper::Roll { width_mm: 58.0 });
    assert_eq!(
        "100x150mm".parse::<Paper>().unwrap(),
        Paper::Custom {
            width_mm: 100.0,
            height_mm: 150.0
        }
    );
    assert_eq!(
        "4x6in".parse::<Paper>().unwrap(),
        Paper::Custom {
            width_mm: 101.6,
            height_mm: 152.4
        }
    );
    assert!("banana".parse::<Paper>().is_err());
    assert_eq!(
        Paper::Custom {
            width_mm: 100.0,
            height_mm: 150.0
        }
        .to_string(),
        "100x150mm"
    );
}

#[test]
fn validate_rejects_oversized_custom_paper() {
    let settings = PrintSettings {
        paper: Some(Paper::Custom {
            width_mm: 50000.0,
            height_mm: 50000.0,
        }),
        ..Default::default()
    };
    assert!(settings.validate().unwrap_err().contains("must be between"));
}

#[test]
fn scale_margins_pages() {
    assert_eq!("90%".parse::<Scale>().unwrap(), Scale::Percent(90.0));
    assert_eq!("fit".parse::<Scale>().unwrap(), Scale::Fit);
    assert_eq!(
        "5,3".parse::<Margins>().unwrap(),
        Margins {
            top: 5.0,
            right: 3.0,
            bottom: 5.0,
            left: 3.0
        }
    );
    let pages: PageRange = "1-3,5,8-".parse().unwrap();
    assert_eq!(pages.indexes(9), vec![0, 1, 2, 4, 7, 8]);
    assert_eq!(pages.to_string(), "1-3,5,8-");
    assert!("0-2".parse::<PageRange>().is_err());
    assert!("3-1".parse::<PageRange>().is_err());
}

#[test]
fn overlay_and_query() {
    let preset = PrintSettings {
        printer: Some("EPSON".into()),
        copies: Some(2),
        paper: Some(Paper::Roll { width_mm: 80.0 }),
        ..Default::default()
    };
    let mut q = HashMap::new();
    q.insert("printer_name".to_string(), "".to_string());
    q.insert("copies".to_string(), "3".to_string());
    q.insert("orientation".to_string(), "portrait".to_string());
    q.insert("preset".to_string(), "struk".to_string());
    let (req, preset_name) = PrintSettings::from_query(&q).unwrap();
    assert_eq!(preset_name.as_deref(), Some("struk"));
    let merged = preset.overlay(&req);
    assert_eq!(merged.printer.as_deref(), Some("EPSON"));
    assert_eq!(merged.copies, Some(3));
    assert_eq!(merged.orientation, Some(Orientation::Portrait));
    assert_eq!(merged.paper, Some(Paper::Roll { width_mm: 80.0 }));

    q.insert("copies".to_string(), "0".to_string());
    assert!(PrintSettings::from_query(&q).is_err());
}

#[test]
fn json_roundtrip_rejects_unknown() {
    let s: PrintSettings = serde_json::from_str(r#"{"paper":"80mm","scale":"90%","margins":"2"}"#).unwrap();
    assert_eq!(
        serde_json::to_string(&s).unwrap(),
        r#"{"paper":"80mm","scale":"90%","margins":"2"}"#
    );
    assert!(serde_json::from_str::<PrintSettings>(r#"{"papr":"A4"}"#).is_err());
    assert!(serde_json::from_str::<PrintSettings>(r#"{"paper":"A9"}"#).is_err());
}
