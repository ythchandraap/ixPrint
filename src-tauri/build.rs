use std::path::PathBuf;

/// Copies the PDFium library for the target being built into `pdfium/lib/`,
/// which `tauri.conf.json` bundles as a resource.
fn stage_pdfium() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let (os_dir, lib) = match os.as_str() {
        "windows" => ("win", "pdfium.dll"),
        "macos" => ("mac", "libpdfium.dylib"),
        "linux" => ("linux", "libpdfium.so"),
        other => panic!("ixPrint tidak mendukung target OS '{}'", other),
    };
    let arch_dir = match arch.as_str() {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => panic!("ixPrint tidak mendukung arsitektur '{}'", other),
    };
    let target = format!("{}-{}", os_dir, arch_dir);

    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("pdfium");
    let source = root.join(&target);
    println!("cargo:rerun-if-changed={}", source.display());
    if !source.join(lib).exists() {
        panic!(
            "PDFium untuk {} tidak ada di {}. Jalankan: bun run fetch-pdfium {}",
            target,
            source.display(),
            target
        );
    }

    let staged = root.join("lib");
    let _ = std::fs::remove_dir_all(&staged);
    std::fs::create_dir_all(&staged).unwrap();
    for file in [lib, "LICENSE"] {
        let from = source.join(file);
        if from.exists() {
            std::fs::copy(&from, staged.join(file)).unwrap();
        }
    }
}

fn main() {
    stage_pdfium();
    tauri_build::build()
}
