use actix_web::middleware::DefaultHeaders;

pub(super) fn security_headers() -> DefaultHeaders {
    DefaultHeaders::new()
        .add(("X-Content-Type-Options", "nosniff"))
        .add(("X-Frame-Options", "DENY"))
        .add(("Referrer-Policy", "no-referrer"))
        .add(("Cache-Control", "no-store"))
        .add(("Content-Security-Policy", "default-src 'none'; frame-ancestors 'none'"))
}
