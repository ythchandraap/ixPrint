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
    println!(
        "cargo:rerun-if-changed={}",
        root.join("THIRD-PARTY-NOTICES.md").display()
    );
    if !source.join(lib).exists() {
        panic!(
            "PDFium untuk {} tidak ada di {}. Jalankan: bun run fetch-pdfium {}",
            target,
            source.display(),
            target
        );
    }
    if std::env::var("PROFILE").is_ok_and(|profile| profile == "release") {
        let manifest = source.join("PDFIUM_BUILD");
        let licenses = source.join("licenses");
        if !manifest.is_file() || !source.join("LICENSE").is_file() || !licenses.is_dir() {
            panic!(
                "PDFium release bundle for {} is missing its checksum manifest or license notices. Run `bun run fetch-pdfium {}` before building",
                target, target
            );
        }
        let details = std::fs::read_to_string(&manifest).unwrap();
        let expected_sha256 = match target.as_str() {
            "win-x64" => "73cc0de638ac2095e7445bf56a38200a5b7c7ca0e9f4ba144598f2457377ac08",
            "mac-x64" => "6dedf83990e0e3d6b7c93c9e7589c5a126b0ae14b7464d76120cff7a26afb18b",
            "mac-arm64" => "52e94ca5aa8847934330daf3f8150c190682c5ca93831468794f8b90d4392e40",
            "linux-x64" => "1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d",
            _ => panic!("No pinned PDFium release bundle is configured for {}", target),
        };
        if details != format!("7881\nsha256={}\n", expected_sha256) {
            panic!(
                "PDFium release bundle for {} does not match its pinned archive checksum",
                target
            );
        }
    }

    let staged = root.join("lib");
    let _ = std::fs::remove_dir_all(&staged);
    std::fs::create_dir_all(&staged).unwrap();
    for file in [lib, "LICENSE", "PDFIUM_BUILD"] {
        let from = source.join(file);
        if from.exists() {
            std::fs::copy(&from, staged.join(file)).unwrap();
        }
    }
    let licenses = source.join("licenses");
    if licenses.is_dir() {
        copy_directory(&licenses, &staged.join("licenses"));
    }
    let notices = root.join("THIRD-PARTY-NOTICES.md");
    std::fs::copy(&notices, staged.join("THIRD-PARTY-NOTICES.md")).unwrap();
}

fn copy_directory(source: &std::path::Path, destination: &std::path::Path) {
    std::fs::create_dir_all(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_directory(&source_path, &destination_path);
        } else {
            std::fs::copy(&source_path, &destination_path).unwrap();
        }
    }
}

fn main() {
    stage_pdfium();
    tauri_build::build()
}
