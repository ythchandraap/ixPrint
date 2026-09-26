use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

fn remote(url: String) -> Remote {
    Remote {
        id: "remote-id".into(),
        name: "Source".into(),
        url,
        token: "fxp_never-log-this-token".into(),
        preset: None,
        created_at: String::new(),
    }
}

fn serve_once(response: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 2048];
        let _ = stream.read(&mut request);
        let _ = stream.write_all(response.as_bytes());
    });
    format!("http://{}", address)
}

fn serve_once_capture(response: String) -> (String, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let thread = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut chunk = [0; 2048];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = stream.read(&mut chunk).unwrap();
            if count == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..count]);
        }
        stream.write_all(response.as_bytes()).unwrap();
        String::from_utf8_lossy(&request).into_owned()
    });
    (format!("http://{}", address), thread)
}

#[test]
fn shared_preset_client_does_not_follow_redirects_and_caps_response_size() {
    let redirect = serve_once(
        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .into(),
    );
    let error = RemoteClient
        .fetch_shared_preset(&remote(redirect), "Label")
        .unwrap_err();
    assert!(error.to_string().contains("HTTP 302"));

    let oversized = serve_once(format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        MAX_SHARED_BUNDLE_BYTES + 1
    ));
    let error = RemoteClient
        .fetch_shared_preset(&remote(oversized), "Label")
        .unwrap_err();
    assert!(error.to_string().contains("larger than allowed"));
}

#[test]
fn shared_preset_client_never_displays_token_from_remote_error_body() {
    let body = r#"{"message":"fxp_never-log-this-token"}"#;
    let base = serve_once(format!(
            "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ));
    let error = RemoteClient.shared_presets(&remote(base)).unwrap_err().to_string();
    assert!(error.contains("token rejected"));
    assert!(!error.contains("fxp_never-log-this-token"));

    let unavailable = RemoteClient
        .shared_presets(&remote("http://127.0.0.1:1".into()))
        .err()
        .unwrap()
        .to_string();
    assert!(unavailable.contains("cannot be reached"));
    assert!(!unavailable.contains("fxp_never-log-this-token"));
}

#[test]
fn remote_pdf_and_test_page_send_stable_idempotency_keys() {
    let body = r#"{"job_id":"remote-job","sheets":1}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let (url, request) = serve_once_capture(response.clone());
    let path = std::env::temp_dir().join(format!("ixprint-remote-{}.pdf", uuid::Uuid::new_v4()));
    std::fs::write(&path, b"%PDF-test").unwrap();
    RemoteClient
        .print_pdf(
            &remote(url),
            &path,
            "document.pdf",
            &PrintSettings::default(),
            "logical-42",
        )
        .unwrap();
    let pdf_request = request.join().unwrap();
    assert!(pdf_request
        .to_ascii_lowercase()
        .contains("idempotency-key: ixprint-logical-42"));
    let _ = std::fs::remove_file(path);

    let (url, request) = serve_once_capture(response);
    RemoteClient
        .test_page(&remote(url), 1, &PrintSettings::default(), "logical-42")
        .unwrap();
    let test_request = request.join().unwrap();
    assert!(test_request
        .to_ascii_lowercase()
        .contains("idempotency-key: ixprint-logical-42"));
    assert_eq!(logical_idempotency_key("logical-42"), "ixprint-logical-42");
    assert_ne!(
        logical_idempotency_key("logical-42"),
        logical_idempotency_key("logical-43")
    );
}

#[test]
fn remote_print_does_not_retry_when_the_receiver_drops_its_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let count = Arc::new(AtomicUsize::new(0));
    let server_stopped = stopped.clone();
    let server_count = count.clone();
    let server = thread::spawn(move || {
        while !server_stopped.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    server_count.fetch_add(1, Ordering::Relaxed);
                    let mut request = [0; 2048];
                    let _ = stream.read(&mut request);
                    // Model acceptance followed by a lost response.
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                }
                Err(error) => panic!("failed to accept remote request: {}", error),
            }
        }
    });

    let path = std::env::temp_dir().join(format!("ixprint-remote-{}.pdf", uuid::Uuid::new_v4()));
    std::fs::write(&path, b"%PDF-test").unwrap();
    let result = RemoteClient.print_pdf(
        &remote(format!("http://{}", address)),
        &path,
        "document.pdf",
        &PrintSettings::default(),
        "logical-42",
    );
    stopped.store(true, Ordering::Relaxed);
    server.join().unwrap();
    let _ = std::fs::remove_file(path);
    assert!(result.is_err());
    assert_eq!(count.load(Ordering::Relaxed), 1);
}

#[test]
fn remote_print_error_redacts_the_remote_token() {
    let body = r#"{"message":"upstream failed fxp_never-log-this-token"}"#;
    let base = serve_once(format!(
        "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    ));
    let path = std::env::temp_dir().join(format!("ixprint-remote-{}.pdf", uuid::Uuid::new_v4()));
    std::fs::write(&path, b"%PDF-test").unwrap();
    let error = RemoteClient
        .print_pdf(
            &remote(base),
            &path,
            "document.pdf",
            &PrintSettings::default(),
            "logical-42",
        )
        .err()
        .unwrap()
        .to_string();
    let _ = std::fs::remove_file(path);
    assert!(!error.contains("fxp_never-log-this-token"));
}
