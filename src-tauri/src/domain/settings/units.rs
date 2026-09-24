pub(super) const MM_PER_INCH: f32 = 25.4;
const PT_PER_INCH: f32 = 72.0;

/// Upper bound for a custom/roll paper side.
pub const MAX_PAPER_MM: f32 = 2000.0;

pub fn mm_to_pt(mm: f32) -> f32 {
    mm / MM_PER_INCH * PT_PER_INCH
}

pub fn pt_to_mm(pt: f32) -> f32 {
    pt / PT_PER_INCH * MM_PER_INCH
}
