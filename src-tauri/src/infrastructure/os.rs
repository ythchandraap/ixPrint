use crate::application::ports::Clock;

pub fn lan_url(port: u16) -> Option<String> {
    local_ip_address::local_ip()
        .ok()
        .map(|ip| format!("http://{}:{}", ip, port))
}

pub fn local_ip() -> Option<String> {
    local_ip_address::local_ip().ok().map(|ip| ip.to_string())
}

#[derive(Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_rfc3339(&self) -> String {
        chrono::Local::now().to_rfc3339()
    }
}
