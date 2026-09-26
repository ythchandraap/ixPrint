use super::*;
use crate::application::ports::HistoryRepository;
use crate::application::state::test_support::state;
use crate::infrastructure::persistence::history_store::History;
use actix_web::test;
use std::sync::Arc;

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
        ("GET", "/presets/shared"),
        ("GET", "/presets/shared/export?name=test"),
        ("POST", "/print"),
        ("POST", "/preflight"),
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
async fn shared_preset_listing_requires_bearer_and_only_returns_shared_metadata() {
    let state = state();
    let (token, stored) = crate::domain::auth::generate("test receiver").unwrap();
    let token_id = stored.id.clone();
    state
        .config
        .update(&mut |config| {
            config.require_token = false;
            config.tokens.push(stored.clone());
            config.presets.insert("Struk".into(), Default::default());
            config.presets.insert("Label".into(), Default::default());
            config.preset_drafts.insert("Draft".into(), Default::default());
            config.shared_presets.extend(["Struk".into(), "Label".into()]);
            Ok(())
        })
        .unwrap();
    let app = test::init_service(App::new().app_data(web::Data::new(state.clone())).configure(routes)).await;
    let unauthenticated = test::call_service(&app, test::TestRequest::get().uri("/presets/shared").to_request()).await;
    assert_eq!(unauthenticated.status().as_u16(), 401);
    let query_token = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/presets/shared?token={}", token))
            .to_request(),
    )
    .await;
    assert_eq!(query_token.status().as_u16(), 401);
    let alternate_header = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/presets/shared")
            .insert_header(("X-Api-Token", token.as_str()))
            .to_request(),
    )
    .await;
    assert_eq!(alternate_header.status().as_u16(), 401);
    let bearer = format!("Bearer {}", token);
    let disabled = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/presets/shared")
            .insert_header(("Authorization", bearer.as_str()))
            .to_request(),
    )
    .await;
    assert_eq!(disabled.status().as_u16(), 403);
    let body: serde_json::Value = test::read_body_json(disabled).await;
    assert_eq!(body["message"], "Shared preset listing is disabled");

    state
        .config
        .update(&mut |config| {
            config.preset_list_endpoint_enabled = true;
            Ok(())
        })
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/presets/shared")
            .insert_header(("Authorization", bearer.as_str()))
            .to_request(),
    )
    .await;
    assert_eq!(response.status().as_u16(), 200);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(
        body,
        serde_json::json!({
            "presets": [
                {"name": "Label", "has_design": false},
                {"name": "Struk", "has_design": false}
            ]
        })
    );
    assert!(state.token_usage.snapshot().contains_key(&token_id));
}

#[actix_web::test]
async fn shared_preset_export_is_one_published_shared_preset_without_defaults() {
    let state = state();
    let (token, stored) = crate::domain::auth::generate("test receiver").unwrap();
    state
        .config
        .update(&mut |config| {
            config.require_token = false;
            config.tokens.push(stored.clone());
            config.default_printer = Some("Local printer".into());
            config.presets.insert("Shared".into(), Default::default());
            config.presets.insert("Private".into(), Default::default());
            config.default_preset = Some("Private".into());
            config.preset_drafts.insert("Shared".into(), Default::default());
            config.preset_previous.insert("Shared".into(), Default::default());
            config.shared_presets.insert("Shared".into());
            Ok(())
        })
        .unwrap();
    let app = test::init_service(App::new().app_data(web::Data::new(state.clone())).configure(routes)).await;
    let bearer = format!("Bearer {}", token);
    for uri in [
        "/presets/shared/export?name=missing",
        "/presets/shared/export?name=Private",
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(uri)
                .insert_header(("Authorization", bearer.as_str()))
                .to_request(),
        )
        .await;
        assert_eq!(response.status().as_u16(), 404);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["message"], "Preset is not available");
    }
    let invalid = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/presets/shared/export?name=%20")
            .insert_header(("Authorization", bearer.as_str()))
            .to_request(),
    )
    .await;
    assert_eq!(invalid.status().as_u16(), 400);
    let no_token = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/presets/shared/export?name=Shared")
            .to_request(),
    )
    .await;
    assert_eq!(no_token.status().as_u16(), 401);

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/presets/shared/export?name=Shared")
            .insert_header(("Authorization", bearer.as_str()))
            .to_request(),
    )
    .await;
    assert_eq!(response.status().as_u16(), 200);
    let body_bytes = test::read_body(response).await;
    let repeated = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/presets/shared/export?name=Shared")
            .insert_header(("Authorization", bearer.as_str()))
            .to_request(),
    )
    .await;
    assert_eq!(repeated.status().as_u16(), 200);
    assert_eq!(body_bytes, test::read_body(repeated).await);
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["format"], "ixprint-presets");
    assert_eq!(body["exported_at"], "");
    assert_eq!(body["presets"].as_object().unwrap().len(), 1);
    assert!(body["presets"].get("Shared").is_some());
    assert!(body["default_printer"].is_null());
    assert!(body["default_preset"].is_null());
    assert!(body["presets"].get("Private").is_none());
    assert!(body["presets"].get("Draft").is_none());
}

#[actix_web::test]
async fn preflight_validates_required_data_without_sending_a_job() {
    let state = state();
    let design: crate::domain::template::Template = serde_json::from_value(serde_json::json!({
        "width_mm": 50, "height_mm": 30,
        "required_fields": ["sku"],
        "elements": [{"type":"text", "x":1, "y":1, "w":40, "h":8, "text":"{{sku}}"}]
    }))
    .unwrap();
    state
        .config
        .update(&mut |config| {
            config.presets.insert(
                "label".into(),
                crate::domain::preset::Preset {
                    settings: Default::default(),
                    design: Some(design.clone()),
                },
            );
            Ok(())
        })
        .unwrap();
    let app = test::init_service(App::new().app_data(web::Data::new(state.clone())).configure(routes)).await;
    let request = test::TestRequest::post()
        .uri("/preflight")
        .insert_header(("Content-Type", "application/json"))
        .set_payload(r#"{"preset":"label","data":[{}]}"#)
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status().as_u16(), 200);
    let report: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(report["valid"], false);
    assert_eq!(report["records"], 1);
    assert_eq!(report["required_fields"][0], "sku");
    assert!(state.history.days().is_empty());
}

#[actix_web::test]
async fn idempotency_key_replays_failure_and_rejects_changed_content() {
    let state = state();
    let app = test::init_service(App::new().app_data(web::Data::new(state.clone())).configure(routes)).await;
    let peer: std::net::SocketAddr = "127.0.0.1:4000".parse().unwrap();
    let send = |body: &'static str| {
        test::TestRequest::post()
            .uri("/print")
            .peer_addr(peer)
            .insert_header(("Content-Type", "application/json"))
            .insert_header(("Idempotency-Key", "order-123"))
            .set_payload(body)
            .to_request()
    };
    let first = test::call_service(&app, send(r#"{"preset":"missing","data":{"sku":"A"}}"#)).await;
    assert_eq!(first.status().as_u16(), 400);
    let second = test::call_service(&app, send(r#"{"preset":"missing","data":{"sku":"A"}}"#)).await;
    assert_eq!(second.status().as_u16(), 400);
    let repeated: serde_json::Value = test::read_body_json(second).await;
    assert!(repeated["message"]
        .as_str()
        .unwrap()
        .contains("Original request failed"));
    let changed = test::call_service(&app, send(r#"{"preset":"missing","data":{"sku":"B"}}"#)).await;
    assert_eq!(changed.status().as_u16(), 409);
    assert_eq!(state.history.days()[0].total, 1);
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
    let root = std::env::temp_dir().join(format!("ixprint-http-history-content-{}", uuid::Uuid::new_v4()));
    let mut app_state = Arc::try_unwrap(state()).ok().unwrap();
    let history = Arc::new(History::with_content_dir(
        root.join("history"),
        root.join("history-content"),
    ));
    app_state.history = history.clone();
    let state = Arc::new(app_state);
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
        "data": { "name": "SECRET_INPUT_VALUE" },
        "header": { "batch": "B-7" },
        "settings": { "printer": missing_printer, "copies": 1 }
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

    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let entry = history.read_page(&day, 1, 20, false).unwrap().entries.remove(0);
    assert_eq!(
        entry.content.as_ref().unwrap().input_kind,
        Some(crate::domain::history::HistoryInputKind::Json)
    );
    assert_eq!(history.read_input_json(&entry).unwrap(), Some(payload));
    let jsonl = std::fs::read_to_string(root.join("history").join(format!("{day}.jsonl"))).unwrap();
    assert!(!jsonl.contains("SECRET_INPUT_VALUE"));
    let _ = std::fs::remove_dir_all(root);
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
