/// One printed sheet, in points. `width > height` means the sheet is fed in landscape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sheet {
    pub width_pt: f32,
    pub height_pt: f32,
}

impl Sheet {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn is_landscape(&self) -> bool {
        self.width_pt > self.height_pt
    }
}

pub struct LayoutResult {
    pub sheets: Vec<Sheet>,
}
