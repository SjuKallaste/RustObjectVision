use egui::Pos2;

#[derive(Clone, PartialEq, Debug)]
pub enum Mode {
    Idle,
    Ready,
    CalibP1,
    CalibP2 { p1: Pos2 },
    CalibLen { p1: Pos2, p2: Pos2 },
    Segmented,
}

#[derive(Clone, PartialEq)]
pub enum Unit {
    Cm2,
    Mm2,
}

impl Unit {
    pub fn label(&self) -> &'static str {
        match self { Unit::Cm2 => "cm²", Unit::Mm2 => "mm²" }
    }
    pub fn factor(&self) -> f64 {
        match self { Unit::Cm2 => 1.0, Unit::Mm2 => 100.0 }
    }
}

#[derive(Clone)]
pub struct Region {
    pub index:       usize, // 1-based display index
    pub pixel_count: usize,
    pub area_cm2:    f64,
    pub avg_color:   [u8; 3],
    pub centroid:    (f32, f32),
}
