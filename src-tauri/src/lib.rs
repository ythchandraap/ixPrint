use actix_multipart::Multipart;
use local_ip_address::local_ip;
use std::{env, str};
use std::process::Command;
use actix_web::{web, App,HttpRequest, HttpResponse, HttpServer, Responder};
use serde::Serialize;
use futures::StreamExt;
use actix_cors::Cors;

use std::{ fs, io::Write, path::{ PathBuf}, fs::File};
use serde::Deserialize;
use actix_web::web::Query;


#[derive(Serialize)]
struct Printer {
    name: String,
    status: String,
    shared: String,
}

#[derive(Serialize)]
struct ResponseMessage {
    message: String,
}
#[derive(Deserialize)]
pub struct PrintQueryParams {
    printer_name: String,
    copies: String,
    orientation: String,
}
fn fetch_printers() -> Vec<Printer> {
    let output = Command::new("powershell")
        .args(&[
            "-Command",
            "Get-Printer | Select-Object Name, PrinterStatus, Shared | ConvertTo-Csv -NoTypeInformation",
        ])
        .output();
    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout
                .lines()
                .skip(1) // Skip CSV headers
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split(',').collect();
                    if parts.len() == 3 {
                        Some(Printer {
                            name: parts[0].trim_matches('"').trim().to_string(),
                            status: parts[1].trim_matches('"').trim().to_string(),
                            shared: parts[2].trim_matches('"').trim().to_string(),
                        })
                    } else {
                        None
                    }
                })
                .collect()
        }
        Err(_) => vec![],
    
    }
}

#[tauri::command]
fn get_printers_json() -> Vec<Printer> {
    fetch_printers()
}
 

// Actix HTTP handler version
async fn get_printers_http() -> impl Responder {
    HttpResponse::Ok().json(fetch_printers())
}

pub async fn handle_print(
    _req: HttpRequest,
    mut multipart: Multipart,
    query: Query<PrintQueryParams>,
) -> impl Responder {
    
    let printer_name = if query.printer_name.is_empty() { "Microsoft Print to PDF" } else { &query.printer_name };
    let copies = if query.copies.is_empty() { "1" } else { &query.copies };
    let orientation = if query.orientation.is_empty() { "landscape" } else { &query.orientation };
    
    println!("Printer: {}, Copies: {}, Orientation: {}", printer_name, copies, orientation);

    let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let folder_path = home_dir.join("fxprint/file");

    if let Err(e) = fs::create_dir_all(&folder_path) {
        eprintln!("Failed to create folder: {}", e);
        return HttpResponse::InternalServerError().body("Failed to create target folder");
    }

    while let Some(item) = multipart.next().await {
        match item {
            Ok(mut field) => {
                let filename = field
                    .content_disposition()
                    .and_then(|cd| cd.get_filename())
                    .unwrap_or("unnamed");

                let file_path = folder_path.join(sanitize_filename::sanitize(&filename));
                let mut f = match File::create(&file_path) {
                    Ok(file) => file,
                    Err(e) => {
                        eprintln!("File create error: {}", e);
                        return HttpResponse::InternalServerError().body("File create failed");
                    }
                };

                while let Some(chunk) = field.next().await {
                    let data = match chunk {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("Chunk read error: {}", e);
                            return HttpResponse::InternalServerError().body("Failed to read chunk");
                        }
                    };
                    if let Err(e) = f.write_all(&data) {
                        eprintln!("Write error: {}", e);
                        return HttpResponse::InternalServerError().body("Failed to write file");
                    }
                }
                // File saved, now run the print command

                if os_detected() == "windows" {
                    let exe_path = PathBuf::from("src/assets/spdf.exe"); // Adjust if needed
    
                    let command_str = format!(
                        "Start-Process -FilePath '{}' -ArgumentList '-print-to \"{}\" -print-settings \"{}x,{}\" \"{}\"' -Wait -WindowStyle Hidden",
                        exe_path.display(),
                        printer_name,
                        copies,
                        orientation,
                        file_path.display()
                    );
    
                    println!("Running command: {}", command_str);
    
                    let output = Command::new("powershell")
                        .args(["-c", &command_str])
                        .output();
    
                    match output {
                        Ok(out) if out.status.success() => {
                            println!("Print successful: {}", String::from_utf8_lossy(&out.stdout));
                            return HttpResponse::Ok().json(ResponseMessage {
                                message: "File uploaded and sent to printer".to_string(),
                            });
                        }
                        Ok(out) => {
                            eprintln!("Print failed: {}", String::from_utf8_lossy(&out.stderr));
                            return HttpResponse::InternalServerError()
                                .body(format!("Print failed: {}", String::from_utf8_lossy(&out.stderr)));
                        }
                        Err(e) => {
                            eprintln!("Failed to run print command: {}", e);
                            return HttpResponse::InternalServerError().body("Failed to execute print command");
                        }
                    }
                } else{
                    return HttpResponse::Ok().json(ResponseMessage {
                        message: " This is not supported. Only Windows is supported.".to_string(),
                    });
                }
            }
            Err(e) => {
                eprintln!("Multipart error: {}", e);
                return HttpResponse::BadRequest().body("Malformed multipart data");
            }
        }
    }

    HttpResponse::BadRequest().body("No file uploaded")
}



async fn hello() -> impl Responder {
    "Hello, world!"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|_app| {
            tauri::async_runtime::spawn(async {
                HttpServer::new(|| {
                    App::new()
                        .wrap(
                            Cors::default()
                                .allow_any_origin()
                                .allow_any_method()
                                .allow_any_header()
                        )
                        .route("/", web::get().to(hello))
                        .route("/list-printer", web::get().to(get_printers_http))
                        .route("/print", web::post().to(handle_print))
                })
                .bind("0.0.0.0:18032")
                .expect("Failed to bind to port 18032")
                .run()
                .await
                .expect("Failed to run Actix server");
            });
            Ok(())
        })
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            this_local_ip,
            os_detected,
            get_printers_json
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn this_local_ip() -> String {
    match local_ip() {
        Ok(ip) => ip.to_string(),
        Err(_) => "Error getting local IP".to_string(),
    }
}

#[tauri::command]
fn os_detected() -> String {
    let os = tauri_plugin_os::platform();
    os.to_string()
}
