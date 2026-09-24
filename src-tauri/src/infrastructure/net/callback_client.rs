use crate::application::ports::CallbackSender;
use crate::callback::{Attempt, Callback, Payload};

#[derive(Default)]
pub struct CallbackClient;

impl CallbackSender for CallbackClient {
    fn deliver(&self, callback: &Callback, payload: &Payload, progress: &mut dyn FnMut(&str, u32, &Attempt)) {
        crate::callback::deliver_with(callback, payload, progress)
    }
}
