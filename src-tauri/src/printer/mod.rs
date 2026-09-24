//! Compatibility facade for printer types and OS operations.

pub use crate::domain::printer::*;
#[allow(unused_imports)]
pub use crate::infrastructure::printing::os_default_printer;
pub use crate::infrastructure::printing::{capabilities, default_paper_mm, job_status, list_printers, print, queue};
pub mod manage;
