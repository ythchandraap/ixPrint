use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(path: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn forbidden_tokens(layer: &str) -> &'static [&'static str] {
    const FRAMEWORK: &[&str] = &[
        "tauri",
        "actix_web",
        "actix_cors",
        "actix_multipart",
        "ureq",
        "pdfium_render",
        "std::fs",
        "std::process",
    ];
    const DOMAIN: &[&str] = &[
        "tauri",
        "actix_web",
        "actix_cors",
        "actix_multipart",
        "ureq",
        "pdfium_render",
        "std::fs",
        "std::process",
        "std::net::TcpStream",
        "crate::application",
        "crate::infrastructure",
        "crate::interfaces",
    ];
    const APPLICATION: &[&str] = &[
        "tauri",
        "actix_web",
        "actix_cors",
        "actix_multipart",
        "ureq",
        "pdfium_render",
        "std::fs",
        "std::process",
        "crate::infrastructure",
        "crate::interfaces",
    ];
    const INFRASTRUCTURE: &[&str] = &[
        "tauri",
        "actix_web",
        "actix_cors",
        "actix_multipart",
        "crate::interfaces",
    ];

    match layer {
        "domain" => DOMAIN,
        "application" => APPLICATION,
        "infrastructure" => INFRASTRUCTURE,
        _ => FRAMEWORK,
    }
}

#[test]
fn layers_do_not_import_forbidden_dependencies() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for layer in ["domain", "application", "infrastructure"] {
        let mut files = Vec::new();
        rust_files(&source.join(layer), &mut files);
        for file in files {
            let contents = fs::read_to_string(&file).expect("read Rust source");
            for (line_number, line) in contents.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") {
                    continue;
                }
                for token in forbidden_tokens(layer) {
                    assert!(
                        !line.contains(token),
                        "{}:{} contains forbidden dependency token {token}",
                        file.display(),
                        line_number + 1
                    );
                }
            }
        }
    }
}
