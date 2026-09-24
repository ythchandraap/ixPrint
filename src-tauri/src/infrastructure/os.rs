use crate::application::ports::Clock;

#[derive(Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_rfc3339(&self) -> String {
        chrono::Local::now().to_rfc3339()
    }
}
