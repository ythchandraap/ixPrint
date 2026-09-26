use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

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
        .unwrap_err()
        .to_string();
    assert!(unavailable.contains("cannot be reached"));
    assert!(!unavailable.contains("fxp_never-log-this-token"));
}
