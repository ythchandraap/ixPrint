use super::*;

#[test]
fn generates_page_of_requested_size() {
    crate::infrastructure::pdf::layout_engine::tests::init_for_tests();
    let dir = std::env::temp_dir().join(format!("ixprint-testpage-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    for (w, h) in [(210.0, 297.0), (80.0, 100.0), (58.0, 80.0), (50.0, 25.0)] {
        let path = dir.join(format!("{}x{}.pdf", w, h));
        generate(&path, w, h, &["Kertas: test".into()], 1).unwrap();
        let pdfium = pdfium().unwrap();
        let doc = pdfium.load_pdf_from_file(&path, None).unwrap();
        let page = doc.pages().get(0).unwrap();
        assert!((crate::domain::settings::pt_to_mm(page.width().value) - w).abs() < 0.1);
        assert!((crate::domain::settings::pt_to_mm(page.height().value) - h).abs() < 0.1);
    }
    // Several pages: same size, one PDF page each.
    let multi = dir.join("multi.pdf");
    generate(&multi, 60.0, 75.0, &[], 3).unwrap();
    {
        let pdfium = pdfium().unwrap();
        let doc = pdfium.load_pdf_from_file(&multi, None).unwrap();
        assert_eq!(doc.pages().len(), 3);
        let text = doc.pages().get(2).unwrap().text().unwrap().all();
        assert!(text.contains("Page 3 of 3"), "{}", text);
    }
    if let Ok(out) = std::env::var("IXPRINT_VISUAL_OUT") {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let _ = std::fs::copy(entry.path(), Path::new(&out).join(entry.file_name()));
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}
