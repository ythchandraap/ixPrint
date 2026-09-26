//! Pure design, element, and record types used by template printing.

mod element;
mod model;
mod pagination;
mod record;

pub use element::{Element, HAlign, ImageFit, VAlign};
pub use model::{FooterMode, Layout, Template, TemplateGroup, AUTO_FIELDS, MAX_RECORDS};
pub(crate) use pagination::{paginate, Band};
pub use record::{fill, record_from_json, Record};
