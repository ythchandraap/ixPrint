use super::*;
use crate::application::state::test_support::state;
use actix_web::test;

#[actix_web::test]
async fn contract_all_routes_registered() {
    let app = test::init_service(App::new().app_data(web::Data::new(state())).configure(routes)).await;

    let routes = [
        ("GET", "/"),
        ("GET", "/docs"),
        ("GET", "/list-printer"),
        ("GET", "/printer-capabilities"),
        ("GET", "/queue"),
        ("GET", "/job-status"),
        ("GET", "/presets"),
        ("GET", "/presets/export"),
        ("POST", "/print"),
        ("POST", "/test-page"),
        ("GET", "/templates"),
        ("POST", "/print-template"),
    ];

    for (method, path) in routes {
        let request = match method {
            "GET" => test::TestRequest::get().uri(path).to_request(),
            "POST" => test::TestRequest::post().uri(path).to_request(),
            _ => unreachable!(),
        };
        let response = test::call_service(&app, request).await;
        assert_ne!(
            response.status(),
            actix_web::http::StatusCode::NOT_FOUND,
            "{method} {path}"
        );
    }
}

#[actix_web::test]
async fn in_flight_limit_returns_retry_after() {
    let state = state();
    let _guards: Vec<_> = (0..crate::application::security::MAX_IN_FLIGHT_PER_CLIENT)
        .map(|_| state.in_flight.acquire("ip:127.0.0.1").unwrap())
        .collect();
    let app = test::init_service(App::new().app_data(web::Data::new(state.clone())).configure(routes)).await;
    let peer: std::net::SocketAddr = "127.0.0.1:4000".parse().unwrap();
    let request = test::TestRequest::post().uri("/test-page").peer_addr(peer).to_request();
    let response = test::call_service(&app, request).await;

    assert_eq!(response.status().as_u16(), 429);
    assert_eq!(response.headers().get("Retry-After").unwrap(), "2");
    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(
        body["message"],
        "Too many print requests in progress for this client; retry shortly"
    );
}

#[actix_web::test]
async fn json_print_validation() {
    let state = state();
    let app = test::init_service(App::new().app_data(web::Data::new(state.clone())).configure(routes)).await;
    let call = |uri: &'static str, body: &'static str| {
        let app = &app;
        async move {
            let req = test::TestRequest::post()
                .uri(uri)
                .insert_header(("Content-Type", "application/json"))
                .set_payload(body.to_string())
                .to_request();
            let resp = test::call_service(app, req).await;
            let code = resp.status().as_u16();
            let json: serde_json::Value = test::read_body_json(resp).await;
            (code, json["message"].as_str().unwrap_or_default().to_string())
        }
    };

    // A preset with a design, and one without.
    let design: crate::domain::template::Template = serde_json::from_value(serde_json::json!({
        "width_mm": 60, "height_mm": 40,
        "elements": [{ "type": "text", "x": 1, "y": 1, "w": 50, "h": 8, "text": "{{nama}} {{rm}}" }]
    }))
    .unwrap();
    state
        .config
        .update(&mut |c| {
            c.presets.insert(
                "label".into(),
                crate::domain::preset::Preset {
                    settings: Default::default(),
                    design: Some(design.clone()),
                },
            );
            c.presets.insert("a4".into(), Default::default());
            Ok(())
        })
        .unwrap();

    for (uri, body, expect) in [
        ("/print", "{not json", "JSON"),
        (
            "/print",
            r#"{"preset":"label","data":{},"nama":"extra"}"#,
            "unknown field",
        ),
        ("/print", r#"{"preset":"label","data":{"a":[1]}}"#, "data"),
        ("/print", r#"{"preset":"missing","data":{"name":"A"}}"#, "was not found"),
        ("/print", r#"{"preset":"a4","data":{"name":"A"}}"#, "No printer"),
        ("/print", r#"{"data":{"name":"A"}}"#, "No printer"),
        (
            "/print",
            r#"{"preset":"label","data":{"nama":"A"},"callback_url":"https://a.id/cb"}"#,
            "token",
        ),
        (
            "/print-template",
            r#"{"template":"missing","data":{"name":"A"}}"#,
            "was not found",
        ),
        (
            "/print-template",
            r#"{"template":"label","preset":"a4","data":{}}"#,
            "preset",
        ),
    ] {
        let (code, msg) = call(uri, body).await;
        assert_eq!(code, 400, "{} {} → {}", uri, body, msg);
        // This validation can reach preset/design checks first when the
        // test host has an OS default printer, which varies by machine.
        let expected = expect == "No printer" && msg.contains("design") || msg.contains(expect);
        assert!(expected, "{} {} → {}", uri, body, msg);
    }

    // A PDF upload without a file is still rejected as before.
    let resp = test::call_service(&app, test::TestRequest::post().uri("/print").to_request()).await;
    assert_eq!(resp.status().as_u16(), 400);

    // An upload that is not a PDF never reaches PDFium.
    let body =
        "--XX\r\nContent-Disposition: form-data; name=\"file\"; filename=\"x.pdf\"\r\n\r\nMZ not a pdf\r\n--XX--\r\n";
    let req = test::TestRequest::post()
        .uri("/print")
        .insert_header(("Content-Type", "multipart/form-data; boundary=XX"))
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status().as_u16(), 400);
    let json: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(json["message"], "File is not a PDF");
    assert_eq!(std::fs::read_dir(&state.jobs_dir).map(|d| d.count()).unwrap_or(0), 0);

    // Unknown printers are refused before any spooler tool runs.
    let resp = test::call_service(&app, test::TestRequest::get().uri("/queue?printer=-h").to_request()).await;
    assert_eq!(resp.status().as_u16(), 404);

    // GET /templates lists presets with a design and their fields.
    let resp = test::call_service(&app, test::TestRequest::get().uri("/templates").to_request()).await;
    let list: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["name"], "label");
    assert_eq!(list[0]["fields"], serde_json::json!(["nama", "rm"]));
}

#[actix_web::test]
async fn test_page_rejects_oversized_custom_paper() {
    let state = state();
    let app = test::init_service(App::new().app_data(web::Data::new(state)).configure(routes)).await;
    let req = test::TestRequest::post()
        .uri("/test-page?paper=50000x50000mm")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status().as_u16(), 400);
    let json: serde_json::Value = test::read_body_json(resp).await;
    assert!(json["message"].as_str().unwrap_or_default().contains("must be between"));
}

#[actix_web::test]
async fn desktop_key_only_authorizes_loopback_requests() {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    let state = state();
    state
        .config
        .update(&mut |c| {
            c.require_token = true;
            Ok(())
        })
        .unwrap();
    let app = test::init_service(App::new().app_data(web::Data::new(state.clone())).configure(routes)).await;

    let local = test::TestRequest::post()
        .uri("/print")
        .peer_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4000))
        .insert_header(("X-IxPrint-Desktop", state.desktop_key.as_str()))
        .to_request();
    let local_response = test::call_service(&app, local).await;
    assert_eq!(local_response.status().as_u16(), 400); // Authorized; empty upload is rejected by /print.

    let remote = test::TestRequest::post()
        .uri("/print")
        .peer_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 4000))
        .insert_header(("X-IxPrint-Desktop", state.desktop_key.as_str()))
        .to_request();
    let remote_response = test::call_service(&app, remote).await;
    assert_eq!(remote_response.status().as_u16(), 401);
}

#[actix_web::test]
async fn json_print_reports_unknown_printer_before_spooling() {
    let state = state();
    let design: crate::domain::template::Template = serde_json::from_value(serde_json::json!({
        "width_mm": 60,
        "height_mm": 40,
        "elements": [{ "type": "text", "x": 1, "y": 1, "w": 50, "h": 8, "text": "{{name}}" }]
    }))
    .unwrap();
    state
        .config
        .update(&mut |c| {
            c.presets.insert(
                "label".into(),
                crate::domain::preset::Preset {
                    settings: Default::default(),
                    design: Some(design.clone()),
                },
            );
            Ok(())
        })
        .unwrap();
    let app = test::init_service(App::new().app_data(web::Data::new(state)).configure(routes)).await;
    let missing_printer = format!("__ixprint_missing_{}__", uuid::Uuid::new_v4());
    let payload = serde_json::json!({
        "preset": "label",
        "data": { "name": "Example" },
        "settings": { "printer": missing_printer }
    });
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/print")
            .insert_header(("Content-Type", "application/json"))
            .set_payload(payload.to_string())
            .to_request(),
    )
    .await;
    assert_eq!(response.status().as_u16(), 400);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert!(body["message"].as_str().unwrap().contains("was not found"));
}

#[actix_web::test]
async fn browser_origins() {
    let state = state();
    let app = test::init_service(
        App::new()
            .wrap(crate::interfaces::http::cors::cors(state.clone()))
            .app_data(web::Data::new(state.clone()))
            .configure(routes),
    )
    .await;
    let status = |origin: &'static str| {
        let app = &app;
        async move {
            let req = test::TestRequest::get()
                .uri("/presets")
                .insert_header(("Host", "192.168.1.6:18032"))
                .insert_header(("Origin", origin))
                .to_request();
            test::call_service(app, req).await.status().as_u16()
        }
    };

    // New installs: no website allowed, except pages served here (/docs).
    assert_ne!(status("https://evil.example").await, 200);
    assert_eq!(status("http://192.168.1.6:18032").await, 200);
    // Requests without Origin (servers, curl) are not a CORS matter.
    assert_eq!(
        test::call_service(&app, test::TestRequest::get().uri("/presets").to_request())
            .await
            .status()
            .as_u16(),
        200
    );

    state
        .config
        .update(&mut |c| {
            c.allowed_origins = vec!["https://simrs.example".into()];
            Ok(())
        })
        .unwrap();
    assert_eq!(status("https://simrs.example").await, 200);
    assert_eq!(status("https://SIMRS.example").await, 200);
    assert_ne!(status("https://simrs.example.evil.test").await, 200);
    assert_ne!(status("null").await, 200);

    state
        .config
        .update(&mut |c| {
            c.allowed_origins = vec!["*".into()];
            Ok(())
        })
        .unwrap();
    assert_eq!(status("https://anything.example").await, 200);
}

#[actix_web::test]
async fn token_protection() {
    let state = state();
    let app = test::init_service(App::new().app_data(web::Data::new(state.clone())).configure(routes)).await;
    let status = |req: test::TestRequest| {
        let app = &app;
        async move { test::call_service(app, req.to_request()).await.status().as_u16() }
    };

    // Tokens optional: open, but a wrong token is still rejected.
    assert_eq!(status(test::TestRequest::get().uri("/presets")).await, 200);
    assert_eq!(
        status(
            test::TestRequest::get()
                .uri("/presets")
                .insert_header(("Authorization", "Bearer fxp_nope"))
        )
        .await,
        401
    );

    // Required with no token yet: everything is refused, with a hint to create one.
    state
        .config
        .update(&mut |c| {
            c.require_token = true;
            Ok(())
        })
        .unwrap();
    assert_eq!(status(test::TestRequest::get().uri("/presets")).await, 401);
    let (token, stored) = crate::domain::auth::generate("SIMRS").unwrap();
    let id = stored.id.clone();
    state
        .config
        .update(&mut |c| {
            c.tokens.push(stored.clone());
            c.require_token = true;
            Ok(())
        })
        .unwrap();

    // Required: every API endpoint needs it, in any of the three places.
    for uri in [
        "/presets",
        "/presets/export",
        "/list-printer",
        "/queue?printer=x",
        "/job-status?job_id=x",
    ] {
        assert_eq!(status(test::TestRequest::get().uri(uri)).await, 401, "{}", uri);
    }
    assert_eq!(status(test::TestRequest::post().uri("/print")).await, 401);
    assert_eq!(status(test::TestRequest::post().uri("/test-page")).await, 401);
    assert_eq!(status(test::TestRequest::get().uri("/templates")).await, 401);
    assert_eq!(status(test::TestRequest::post().uri("/print-template")).await, 401);
    let bearer = format!("Bearer {}", token);
    assert_eq!(
        status(
            test::TestRequest::get()
                .uri("/presets")
                .insert_header(("Authorization", bearer.as_str()))
        )
        .await,
        200
    );
    assert_eq!(
        status(
            test::TestRequest::get()
                .uri("/presets")
                .insert_header(("X-Api-Token", token.as_str()))
        )
        .await,
        200
    );
    assert_eq!(
        status(test::TestRequest::get().uri(&format!("/presets?token={}", token))).await,
        200
    );
    assert!(state.token_usage.snapshot().contains_key(&id));

    // Health check and docs stay public.
    assert_eq!(status(test::TestRequest::get().uri("/")).await, 200);
    assert_eq!(status(test::TestRequest::get().uri("/docs")).await, 200);

    // Repeated wrong tokens lock the client out, even with a valid token afterwards.
    let attacker: std::net::SocketAddr = "10.0.0.66:5000".parse().unwrap();
    for _ in 0..10 {
        let req = test::TestRequest::get()
            .uri("/presets")
            .peer_addr(attacker)
            .insert_header(("X-Api-Token", "fxp_guess"));
        assert_eq!(status(req).await, 401);
    }
    let req = test::TestRequest::get()
        .uri("/presets")
        .peer_addr(attacker)
        .insert_header(("Authorization", bearer.as_str()));
    assert_eq!(status(req).await, 429);
    let other: std::net::SocketAddr = "10.0.0.67:5000".parse().unwrap();
    let req = test::TestRequest::get()
        .uri("/presets")
        .peer_addr(other)
        .insert_header(("Authorization", bearer.as_str()));
    assert_eq!(status(req).await, 200);

    // Revoked token stops working.
    state
        .config
        .update(&mut |c| {
            c.tokens.clear();
            Ok(())
        })
        .unwrap();
    assert_eq!(
        status(
            test::TestRequest::get()
                .uri("/presets")
                .insert_header(("Authorization", bearer.as_str()))
        )
        .await,
        401
    );
}
