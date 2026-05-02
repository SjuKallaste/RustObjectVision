use egui::ColorImage;
use image::DynamicImage;

// ── Color filter definition ───────────────────────────────────────────────────

/// Named color filter. Chromatic colors use a hue range; achromatic ones
/// (White / Gray / Black) set `hue_range` to `None` and are matched by
/// saturation + value thresholds instead.
#[derive(Clone, PartialEq)]
pub struct ColorFilter {
    pub label:     &'static str,
    pub swatch:    egui::Color32,
    /// Hue range in degrees [0, 360). `None` means achromatic.
    pub hue_range: Option<(f32, f32)>,
}

pub fn all_color_filters() -> Vec<ColorFilter> {
    vec![
        ColorFilter { label: "Red", swatch: egui::Color32::from_rgb(220,  50,  50), hue_range: Some((330.0, 30.0)) },
        ColorFilter { label: "Orange", swatch: egui::Color32::from_rgb(230, 130,  30), hue_range: Some(( 15.0, 45.0)) },
        ColorFilter { label: "Yellow", swatch: egui::Color32::from_rgb(230, 210,  30), hue_range: Some(( 40.0, 75.0)) },
        ColorFilter { label: "Green", swatch: egui::Color32::from_rgb( 40, 190,  60), hue_range: Some(( 70.0, 165.0)) },
        ColorFilter { label: "Cyan", swatch: egui::Color32::from_rgb( 30, 200, 210), hue_range: Some((160.0, 200.0)) },
        ColorFilter { label: "Blue", swatch: egui::Color32::from_rgb( 50,  90, 220), hue_range: Some((195.0, 265.0)) },
        ColorFilter { label: "Purple", swatch: egui::Color32::from_rgb(140,  50, 210), hue_range: Some((260.0, 310.0)) },
        ColorFilter { label: "Pink", swatch: egui::Color32::from_rgb(220,  80, 160), hue_range: Some((305.0, 340.0)) },
        ColorFilter { label: "White", swatch: egui::Color32::from_rgb(240, 240, 240), hue_range: None },
        ColorFilter { label: "Gray", swatch: egui::Color32::from_rgb(140, 140, 140), hue_range: None },
        ColorFilter { label: "Black", swatch: egui::Color32::from_rgb( 30,  30,  30), hue_range: None },
    ]
}

pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let cmax  = rf.max(gf).max(bf);
    let cmin  = rf.min(gf).min(bf);
    let delta = cmax - cmin;
    let v = cmax;
    let s = if cmax == 0.0 { 0.0 } else { delta / cmax };
    let h = if delta == 0.0 {
        0.0
    } else if cmax == rf {
        60.0 * (((gf - bf) / delta) % 6.0)
    } else if cmax == gf {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    (h, s, v)
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h as u32) / 60 {
        0 => (c, x, 0.0f32),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8]
}

pub fn pixel_matches_filter(r: u8, g: u8, b: u8, filter: &ColorFilter) -> bool {
    let (h, s, v) = rgb_to_hsv(r, g, b);
    match filter.hue_range {
        None => {
            if s > 0.25 { return false; }
            match filter.label {
                "White" => v > 0.75,
                "Black" => v < 0.25,
                _       => v >= 0.25 && v <= 0.75, // Gray
            }
        }
        Some((lo, hi)) => {
            if s < 0.20 || v < 0.10 { return false; }
            if lo > hi { h >= lo || h <= hi } else { h >= lo && h <= hi }
        }
    }
}

pub fn build_color_filter_texture(img: &DynamicImage, filters: &[&ColorFilter]) -> ColorImage {
    let rgb = img.to_rgb8();
    let w   = rgb.width()  as usize;
    let h   = rgb.height() as usize;
    let pixels = rgb.pixels().map(|p| {
        let (r, g, b) = (p[0], p[1], p[2]);
        if filters.iter().any(|f| pixel_matches_filter(r, g, b, f)) {
            egui::Color32::from_rgb(r, g, b)
        } else {
            egui::Color32::BLACK
        }
    }).collect();
    ColorImage { size: [w, h], pixels }
}

pub fn compute_prominent_filters(
    img:       &DynamicImage,
    filters:   &[ColorFilter],
    threshold: f64,
) -> Vec<usize> {
    let rgb = img.to_rgb8();
    let total = (rgb.width() * rgb.height()) as f64;
    let mut counts = vec![0usize; filters.len()];

    for p in rgb.pixels() {
        for (i, f) in filters.iter().enumerate() {
            if pixel_matches_filter(p[0], p[1], p[2], f) {
                counts[i] += 1;
                break;
            }
        }
    }

    let mut prominent: Vec<(usize, usize)> = counts
        .iter()
        .enumerate()
        .filter(|&(_, &c)| c as f64 / total >= threshold)
        .map(|(i, &c)| (i, c))
        .collect();

    prominent.sort_by(|a, b| b.1.cmp(&a.1));
    prominent.into_iter().map(|(i, _)| i).collect()
}

pub fn pixel_area_for_filter(
    img: &DynamicImage,
    filter: &ColorFilter,
    scale_px_per_cm: f64,
) -> (usize, f64) {
    let rgb = img.to_rgb8();
    let count = rgb.pixels()
        .filter(|p| pixel_matches_filter(p[0], p[1], p[2], filter))
        .count();
    let area = count as f64 / (scale_px_per_cm * scale_px_per_cm);
    (count, area)
}
