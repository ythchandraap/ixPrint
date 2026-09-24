//! Compatibility facade for printer administration.

pub use crate::domain::printer_admin::*;
pub use crate::infrastructure::printing::admin::{add, details, devices, drivers, remove, set_os_default, update};
