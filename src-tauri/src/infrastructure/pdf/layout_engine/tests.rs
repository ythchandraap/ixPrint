use super::*;
use crate::application::job_service::{test_page_info, test_sheet_mm};
use crate::domain::settings::{pt_to_mm, Margins, Orientation, PageRange, PrintSettings, Scale};

/// Loads the PDFium build for the machine running the tests.
pub fn init_for_tests() {
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    let os = if cfg!(target_os = "macos") {
        "mac"
    } else if cfg!(windows) {
        "win"
    } else {
        "linux"
    };
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("pdfium")
        .join(format!("{}-{}", os, arch));
    init(&[dir]).expect("run `bun run fetch-pdfium` first");
}

fn job(overrides: PrintSettings) -> JobSettings {
    JobSettings::resolve(&overrides, "test".into())
}

const A4: (f32, f32) = (210.0, 297.0);

#[test]
fn shrink_fit_and_orientation() {
    // Landscape A4 content, auto orientation → landscape A4 sheet, 1:1.
    let (w, h) = (mm_to_pt(297.0), mm_to_pt(210.0));
    let p = place(w, h, &job(PrintSettings::default()), A4);
    assert!(p.sheet.is_landscape());
    assert!((p.scale - 1.0).abs() < 1e-4);

    // Forced portrait: shrunk to the sheet width and centered vertically.
    let s = job(PrintSettings {
        orientation: Some(Orientation::Portrait),
        ..Default::default()
    });
    let p = place(w, h, &s, A4);
    assert!(!p.sheet.is_landscape());
    assert!((p.scale - 210.0 / 297.0).abs() < 1e-3);
    assert!((p.x).abs() < 0.01);
    assert!((p.y - (p.sheet.height_pt - h * p.scale) / 2.0).abs() < 0.01);

    // Shrink never enlarges; fit does.
    let small = (100.0, 100.0);
    assert_eq!(place(small.0, small.1, &job(PrintSettings::default()), A4).scale, 1.0);
    let fit = job(PrintSettings {
        scale: Some(Scale::Fit),
        ..Default::default()
    });
    assert!(place(small.0, small.1, &fit, A4).scale > 5.0);
}

#[test]
fn roll_paper_follows_content() {
    let s = job(PrintSettings {
        paper: Some(Paper::Roll { width_mm: 80.0 }),
        scale: Some(Scale::Fit),
        margins: Some(Margins {
            top: 2.0,
            right: 2.0,
            bottom: 2.0,
            left: 2.0,
        }),
        ..Default::default()
    });
    let p = place(mm_to_pt(210.0), mm_to_pt(297.0), &s, A4);
    assert!((p.sheet.width_pt - mm_to_pt(80.0)).abs() < 0.01);
    let expected_h = mm_to_pt(297.0) * (mm_to_pt(76.0) / mm_to_pt(210.0)) + mm_to_pt(4.0);
    assert!((p.sheet.height_pt - expected_h).abs() < 0.1);
}

#[test]
fn custom_size_is_used_as_entered() {
    let label = |orientation| {
        job(PrintSettings {
            paper: Some(Paper::Custom {
                width_mm: 60.0,
                height_mm: 75.0,
            }),
            orientation: Some(orientation),
            scale: Some(Scale::Fit),
            ..Default::default()
        })
    };
    let (w, h) = (mm_to_pt(60.0), mm_to_pt(75.0));
    let landscape_doc = (mm_to_pt(297.0), mm_to_pt(210.0));

    for o in [Orientation::Auto, Orientation::Portrait, Orientation::Landscape] {
        let p = place(landscape_doc.0, landscape_doc.1, &label(o), A4);
        // The label never changes size or orientation.
        assert!(
            (p.sheet.width_pt - w).abs() < 0.01 && (p.sheet.height_pt - h).abs() < 0.01,
            "{:?}",
            o
        );
    }
    // Auto turns a landscape page to fit the portrait label; Portrait keeps it upright.
    assert_eq!(
        place(landscape_doc.0, landscape_doc.1, &label(Orientation::Auto), A4).extra_rotation,
        90
    );
    assert_eq!(
        place(landscape_doc.0, landscape_doc.1, &label(Orientation::Portrait), A4).extra_rotation,
        0
    );
    assert_eq!(
        place(landscape_doc.0, landscape_doc.1, &label(Orientation::Landscape), A4).extra_rotation,
        90
    );
    // A portrait page on a portrait label needs no turn in Auto.
    assert_eq!(
        place(landscape_doc.1, landscape_doc.0, &label(Orientation::Auto), A4).extra_rotation,
        0
    );

    // A landscape label entered as 100x50 stays 100 wide.
    let wide = job(PrintSettings {
        paper: Some(Paper::Custom {
            width_mm: 100.0,
            height_mm: 50.0,
        }),
        ..Default::default()
    });
    let p = place(landscape_doc.0, landscape_doc.1, &wide, A4);
    assert!((p.sheet.width_pt - mm_to_pt(100.0)).abs() < 0.01);
    assert_eq!(p.extra_rotation, 0);
}

#[test]
fn offsets_and_alignment() {
    let s = job(PrintSettings {
        align: Some(Align::TopLeft),
        offset_x: Some(10.0),
        offset_y: Some(5.0),
        ..Default::default()
    });
    let p = place(100.0, 100.0, &s, A4);
    assert!((p.x - mm_to_pt(10.0)).abs() < 0.01);
    assert!((p.y - (p.sheet.height_pt - 100.0 - mm_to_pt(5.0))).abs() < 0.01);
}

/// Makes a page with a red square marking its displayed top-left corner, then
/// applies `rotation` and an offset crop box, which is where layout bugs hide.
fn make_marked_pdf(path: &Path, rotation: PdfPageRenderRotation) {
    let pdfium = pdfium().unwrap();
    let mut doc = pdfium.create_new_pdf().unwrap();
    // Media 400x300; crop to (50,50)-(350,250) → 300x200 visible.
    let mut page = doc
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::Custom(PdfPoints::new(400.0), PdfPoints::new(300.0)))
        .unwrap();
    // Visible area gets a blue background; the marker sits at the displayed
    // top-left corner once the rotation is applied.
    let blue = PdfPagePathObject::new_rect(
        &doc,
        PdfRect::new_from_values(50.0, 50.0, 250.0, 350.0),
        None,
        None,
        Some(PdfColor::new(0, 0, 255, 255)),
    )
    .unwrap();
    page.objects_mut().add_path_object(blue).unwrap();
    // Unrotated corner that ends up displayed top-left for each rotation.
    let (x, y) = match rotation {
        PdfPageRenderRotation::None => (50.0, 210.0),        // top-left
        PdfPageRenderRotation::Degrees90 => (50.0, 50.0),    // bottom-left
        PdfPageRenderRotation::Degrees180 => (310.0, 50.0),  // bottom-right
        PdfPageRenderRotation::Degrees270 => (310.0, 210.0), // top-right
    };
    let red = PdfPagePathObject::new_rect(
        &doc,
        PdfRect::new_from_values(y, x, y + 40.0, x + 40.0),
        None,
        None,
        Some(PdfColor::new(255, 0, 0, 255)),
    )
    .unwrap();
    page.objects_mut().add_path_object(red).unwrap();
    page.boundaries_mut()
        .set_crop(PdfRect::new_from_values(50.0, 50.0, 250.0, 350.0))
        .unwrap();
    page.set_rotation(rotation);
    doc.save_to_file(path).unwrap();
}

/// Renders page 0 of `path` at 1px per point and returns (width, height, rgba).
fn render(path: &Path) -> (usize, usize, Vec<u8>) {
    let pdfium = pdfium().unwrap();
    let doc = pdfium.load_pdf_from_file(path, None).unwrap();
    let page = doc.pages().get(0).unwrap();
    let (w, h) = (page.width().value.round() as i32, page.height().value.round() as i32);
    let bmp = page
        .render_with_config(&PdfRenderConfig::new().set_fixed_size(w, h))
        .unwrap();
    (w as usize, h as usize, bmp.as_rgba_bytes())
}

fn pixel(img: &(usize, usize, Vec<u8>), x: usize, y: usize) -> (u8, u8, u8) {
    let i = (y * img.0 + x) * 4;
    (img.2[i], img.2[i + 1], img.2[i + 2])
}

#[test]
fn output_matches_source_for_every_rotation() {
    init_for_tests();
    let dir = std::env::temp_dir().join(format!("ixprint-layout-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();

    for rotation in [
        PdfPageRenderRotation::None,
        PdfPageRenderRotation::Degrees90,
        PdfPageRenderRotation::Degrees180,
        PdfPageRenderRotation::Degrees270,
    ] {
        let src = dir.join(format!("src-{:?}.pdf", rotation));
        let out = dir.join(format!("out-{:?}.pdf", rotation));
        make_marked_pdf(&src, rotation);

        let expected = render(&src);
        let (ew, eh) = (expected.0 as f32, expected.1 as f32);
        // Sheet exactly the displayed page size, 1:1 → output must equal the source render.
        let s = job(PrintSettings {
            paper: Some(Paper::Custom {
                width_mm: crate::domain::settings::pt_to_mm(ew),
                height_mm: crate::domain::settings::pt_to_mm(eh),
            }),
            scale: Some(Scale::None),
            ..Default::default()
        });
        let result = build(&src, &out, &s, A4).unwrap();
        assert_eq!(result.sheets.len(), 1);
        let actual = render(&out);
        assert_eq!((actual.0, actual.1), (expected.0, expected.1), "{:?} size", rotation);

        // Red marker top-left, blue elsewhere, no white gaps from a wrong crop offset.
        assert_eq!(pixel(&actual, 5, 5), (255, 0, 0), "{:?} marker", rotation);
        assert_eq!(
            pixel(&actual, actual.0 - 5, actual.1 - 5),
            (0, 0, 255),
            "{:?} corner",
            rotation
        );
        assert_eq!(
            pixel(&actual, actual.0 / 2, actual.1 / 2),
            (0, 0, 255),
            "{:?} middle",
            rotation
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn roll_landscape_turns_content_clockwise() {
    init_for_tests();
    let dir = std::env::temp_dir().join(format!("ixprint-roll-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (src, out) = (dir.join("src.pdf"), dir.join("out.pdf"));
    make_marked_pdf(&src, PdfPageRenderRotation::None); // 300x200 displayed

    let s = job(PrintSettings {
        paper: Some(Paper::Roll {
            width_mm: crate::domain::settings::pt_to_mm(200.0),
        }),
        orientation: Some(Orientation::Landscape),
        scale: Some(Scale::None),
        ..Default::default()
    });
    build(&src, &out, &s, A4).unwrap();
    let img = render(&out);
    assert_eq!((img.0, img.1), (200, 300));
    // The top-left marker is now top-right.
    assert_eq!(pixel(&img, img.0 - 5, 5), (255, 0, 0));
    assert_eq!(pixel(&img, 5, 5), (0, 0, 255));
    let _ = std::fs::remove_dir_all(dir);
}

/// Concurrent jobs (parallel `/print` requests) must not crash PDFium.
#[test]
fn concurrent_layouts() {
    init_for_tests();
    let dir = std::env::temp_dir().join(format!("ixprint-conc-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("src.pdf");
    make_marked_pdf(&src, PdfPageRenderRotation::Degrees90);
    let handles: Vec<_> = (0..12)
        .map(|i| {
            let (src, out) = (src.clone(), dir.join(format!("o{}.pdf", i)));
            let page = dir.join(format!("t{}.pdf", i));
            std::thread::spawn(move || {
                for _ in 0..10 {
                    crate::infrastructure::pdf::test_page::generate(&page, 80.0, 100.0, &["x".into()], 2).unwrap();
                    build(&page, &out, &job(PrintSettings::default()), A4).unwrap();
                    build(&src, &out, &job(PrintSettings::default()), A4).unwrap();
                    render(&out);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn page_selection_and_errors() {
    init_for_tests();
    let dir = std::env::temp_dir().join(format!("ixprint-pages-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("src.pdf");
    make_marked_pdf(&src, PdfPageRenderRotation::None);

    let s = job(PrintSettings {
        pages: Some("5-9".parse::<PageRange>().unwrap()),
        ..Default::default()
    });
    assert!(build(&src, &dir.join("o.pdf"), &s, A4).is_err());

    let bogus = dir.join("bogus.pdf");
    std::fs::write(&bogus, b"not a pdf").unwrap();
    assert!(build(&bogus, &dir.join("o2.pdf"), &job(PrintSettings::default()), A4).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_page_frame_lands_on_sheet_edge() {
    init_for_tests();
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
        crate::infrastructure::pdf::test_page::generate(&src, w, h, &test_page_info(&state, &job, None, w, h), 1)
            .unwrap();
        job.scale = Scale::None;
        job.margins = Margins::default();
        job.orientation = Orientation::Auto;
        let out = dir.join(format!("o{}.pdf", i));
        let result = build(&src, &out, &job, (210.0, 297.0)).unwrap();
        let sheet = result.sheets[0];
        assert!((pt_to_mm(sheet.width_pt) - ew).abs() < 0.2, "{} width", json);
        assert!((pt_to_mm(sheet.height_pt) - eh).abs() < 0.2, "{} height", json);

        // Render at 4 px/pt: the outermost pixels on every side must be the black frame.
        let pdfium = pdfium().unwrap();
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
