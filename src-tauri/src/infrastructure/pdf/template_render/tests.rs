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
    crate::infrastructure::pdf::layout_engine::tests::init_for_tests();
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
    crate::infrastructure::pdf::layout_engine::tests::init_for_tests();
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
    crate::infrastructure::pdf::layout_engine::tests::init_for_tests();
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
