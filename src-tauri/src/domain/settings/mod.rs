//! Print settings: the partial form used by presets and requests, and the fully
//! resolved form used for a print job.
//!
//! Every value has a compact string form so the same syntax works in query
//! parameters, presets and exported preset files (e.g. `paper=80x200mm`,
//! `margins=5,3`, `pages=1-3,5`, `scale=90%`).

macro_rules! string_serde {
    ($ty:ty) => {
        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.to_string())
            }
        }
        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

mod device_opts;
mod layout_opts;
mod page_range;
mod paper;
mod print_settings;
pub mod units;

#[cfg(test)]
mod tests;

pub use device_opts::{ColorMode, Duplex};
pub use layout_opts::{Align, Margins, Scale};
pub use page_range::PageRange;
pub use paper::{Orientation, Paper, STANDARD_PAPERS};
pub use print_settings::{JobSettings, PrintSettings, MAX_COPIES, MAX_DPI, MIN_DPI};
pub use units::{mm_to_pt, pt_to_mm, MAX_PAPER_MM};
