//! Compatibility facade for printer types and OS operations.

pub use crate::domain::printer::*;
pub use crate::infrastructure::printing::{
    capabilities, default_paper_mm, job_status, list_printers, os_default_printer, print, queue,
};
pub mod manage;
