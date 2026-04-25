use eframe::egui;
use egui::{ColorImage, Pos2, Rect, ScrollArea, TextureHandle, TextureOptions, Vec2};
use image::DynamicImage;
use rfd::FileDialog;
use std::collections::{HashSet, VecDeque};
use std::io::Write;

#[derive(Clone, PartialEq, Debug)]
enum Mode {
    Idle,
    Ready,
    CalibP1,
    CalibP2 { p1: Pos2 },
    CalibLen { p1: Pos2, p2: Pos2 },
    Segmented,
}

#[derive(Clone, PartialEq)]
enum Unit { Cm2, Mm2 }

impl Unit {
    fn label(&self)  -> &'static str { match self { Unit::Cm2 => "cm²", Unit::Mm2 => "mm²" } }
    fn factor(&self) -> f64          { match self { Unit::Cm2 => 1.0,   Unit::Mm2 => 100.0  } }
}

#[derive(Clone)]
struct Region {
    index: usize, // 1-based display index
    pixel_count: usize,
    area_cm2: f64,
    avg_color: [u8; 3],
    centroid: (f32, f32),
}

/// Named color filter – picks pixels whose hue falls within [hue_min, hue_max].
/// Achromatic colors (white, gray, black) use the `is_achromatic` flag instead.
#[derive(Clone, PartialEq)]
struct ColorFilter {
    label:        &'static str,
    swatch:       egui::Color32,
    /// hue range in degrees [0, 360). None means "achromatic".
    hue_range:    Option<(f32, f32)>,
}

fn color_filters() -> Vec<ColorFilter> {
    vec![
        ColorFilter { label: "Red",    swatch: egui::Color32::from_rgb(220,  50,  50), hue_range: Some((330.0, 30.0))  },
        ColorFilter { label: "Orange", swatch: egui::Color32::from_rgb(230, 130,  30), hue_range: Some(( 15.0, 45.0))  },
        ColorFilter { label: "Yellow", swatch: egui::Color32::from_rgb(230, 210,  30), hue_range: Some(( 40.0, 75.0))  },
        ColorFilter { label: "Green",  swatch: egui::Color32::from_rgb( 40, 190,  60), hue_range: Some(( 70.0,165.0))  },
        ColorFilter { label: "Cyan",   swatch: egui::Color32::from_rgb( 30, 200, 210), hue_range: Some((160.0,200.0))  },
        ColorFilter { label: "Blue",   swatch: egui::Color32::from_rgb( 50,  90, 220), hue_range: Some((195.0,265.0))  },
        ColorFilter { label: "Purple", swatch: egui::Color32::from_rgb(140,  50, 210), hue_range: Some((260.0,310.0))  },
        ColorFilter { label: "Pink",   swatch: egui::Color32::from_rgb(220,  80, 160), hue_range: Some((305.0,340.0))  },
        ColorFilter { label: "White",  swatch: egui::Color32::from_rgb(240, 240, 240), hue_range: None                 },
        ColorFilter { label: "Gray",   swatch: egui::Color32::from_rgb(140, 140, 140), hue_range: None                 },
        ColorFilter { label: "Black",  swatch: egui::Color32::from_rgb( 30,  30,  30), hue_range: None                 },
    ]
}

/// Returns hue in [0, 360), saturation in [0,1], value in [0,1].
fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let cmax = rf.max(gf).max(bf);
    let cmin = rf.min(gf).min(bf);
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

/// Does this pixel match the given color filter?
fn pixel_matches_filter(r: u8, g: u8, b: u8, filter: &ColorFilter) -> bool {
    let (h, s, v) = rgb_to_hsv(r, g, b);

    match filter.hue_range {
        None => {
            // Achromatic: low saturation
            if s > 0.25 { return false; }
            match filter.label {
                "White" => v > 0.75,
                "Black" => v < 0.25,
                _       => v >= 0.25 && v <= 0.75, // Gray
            }
        }
        Some((lo, hi)) => {
            // Must have enough saturation & brightness to be a real color
            if s < 0.20 || v < 0.10 { return false; }
            // Wrap-around hue range (e.g. red spans 330°–30°)
            if lo > hi {
                h >= lo || h <= hi
            } else {
                h >= lo && h <= hi
            }
        }
    }
}

struct App {
    image: Option<DynamicImage>,
    img_w: u32,
    img_h: u32,
    orig_tex: Option<TextureHandle>,
    seg_tex: Option<TextureHandle>,
    edge_tex: Option<TextureHandle>,
    color_filter_tex: Option<TextureHandle>,
    img_rect: Rect,

    show_seg: bool,
    show_edges: bool,
    active_color_filters: HashSet<usize>, // indices into color_filters()

    mode: Mode,
    calib_len_buf: String,
    scale_px_per_cm: Option<f64>,

    tolerance: u32,
    min_pixels: usize,
    blur_radius: u32,

    label_map: Vec<i32>,
    regions: Vec<Region>,
    selected: HashSet<usize>,
    total_area_cm2: f64,
    unit: Unit,

    color_filters: Vec<ColorFilter>,
    status: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            image: None,
            img_w: 0,
            img_h: 0,
            orig_tex: None,
            seg_tex: None,
            edge_tex: None,
            color_filter_tex: None,
            img_rect: Rect::NOTHING,
            show_seg: false,
            show_edges: false,
            active_color_filters: HashSet::new(),
            mode: Mode::Idle,
            calib_len_buf: String::new(),
            scale_px_per_cm: None,
            tolerance: 30,
            min_pixels: 200,
            blur_radius: 0,
            label_map: Vec::new(),
            regions: Vec::new(),
            selected: HashSet::new(),
            total_area_cm2: 0.0,
            unit: Unit::Cm2,
            color_filters: color_filters(),
            status: "Step 1: Load an image.".into(),
        }
    }
}

fn color_dist(a: [u8; 3], b: [u8; 3]) -> u32 {
    (a[0] as i32 - b[0] as i32).abs() as u32
        + (a[1] as i32 - b[1] as i32).abs() as u32
        + (a[2] as i32 - b[2] as i32).abs() as u32
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
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

fn screen_to_norm(screen: Pos2, rect: Rect) -> Pos2 {
    Pos2::new(
        ((screen.x - rect.min.x) / rect.width()).clamp(0.0, 1.0),
        ((screen.y - rect.min.y) / rect.height()).clamp(0.0, 1.0),
    )
}

fn norm_to_screen(norm: Pos2, rect: Rect) -> Pos2 {
    Pos2::new(
        rect.min.x + norm.x * rect.width(),
        rect.min.y + norm.y * rect.height(),
    )
}

fn norm_to_px_dist(p1: Pos2, p2: Pos2, w: u32, h: u32) -> f64 {
    let dx = (p1.x - p2.x) as f64 * w as f64;
    let dy = (p1.y - p2.y) as f64 * h as f64;
    (dx * dx + dy * dy).sqrt()
}

fn box_blur(img: &DynamicImage, radius: u32) -> DynamicImage {
    if radius == 0 { return img.clone(); }
    let src = img.to_rgb8();
    let w   = src.width();
    let h   = src.height();
    let mut out = image::RgbImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let x0 = x.saturating_sub(radius);
            let x1 = (x + radius).min(w - 1);
            let y0 = y.saturating_sub(radius);
            let y1 = (y + radius).min(h - 1);
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            for ny in y0..=y1 {
                for nx in x0..=x1 {
                    let p = src.get_pixel(nx, ny);
                    r += p[0] as u32; g += p[1] as u32; b += p[2] as u32; n += 1;
                }
            }
            out.put_pixel(x, y, image::Rgb([(r/n) as u8, (g/n) as u8, (b/n) as u8]));
        }
    }
    DynamicImage::ImageRgb8(out)
}

fn sobel_texture(img: &DynamicImage) -> ColorImage {
    let gray = img.to_luma8();
    let w    = gray.width()  as usize;
    let h    = gray.height() as usize;
    let get  = |x: usize, y: usize| gray.get_pixel(x as u32, y as u32)[0] as f32;
    let mut pixels = vec![egui::Color32::TRANSPARENT; w * h];
    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let gx = -get(x-1,y-1) - 2.0*get(x-1,y) - get(x-1,y+1)
                +get(x+1,y-1) + 2.0*get(x+1,y) + get(x+1,y+1);
            let gy = -get(x-1,y-1) - 2.0*get(x,y-1) - get(x+1,y-1)
                +get(x-1,y+1) + 2.0*get(x,y+1) + get(x+1,y+1);
            let mag = (gx * gx + gy * gy).sqrt().min(255.0) as u8;
            pixels[y * w + x] = egui::Color32::from_rgba_unmultiplied(255, 100, 0, mag);
        }
    }
    ColorImage { size: [w, h], pixels }
}

fn segment(img: &DynamicImage, tol: u32, min_px: usize, scale: f64) -> (Vec<i32>, Vec<Region>) {
    let rgb = img.to_rgb8();
    let w   = rgb.width()  as usize;
    let h   = rgb.height() as usize;

    let pixels: Vec<[u8; 3]> = rgb.pixels().map(|p| [p[0], p[1], p[2]]).collect();
    let mut labels   = vec![-1i32; w * h];
    let mut next_lbl = 0usize;

    let mut counts:    Vec<usize>    = Vec::new();
    let mut color_sum: Vec<[u64; 3]> = Vec::new();
    let mut cx_sum:    Vec<u64>      = Vec::new();
    let mut cy_sum:    Vec<u64>      = Vec::new();

    for start in 0..(w * h) {
        if labels[start] != -1 { continue; }
        let lbl = next_lbl as i32;
        next_lbl += 1;
        counts.push(0); color_sum.push([0; 3]); cx_sum.push(0); cy_sum.push(0);

        let seed = pixels[start];
        labels[start] = lbl;
        let mut q = VecDeque::new();
        q.push_back(start);

        while let Some(idx) = q.pop_front() {
            let px = idx % w;
            let py = idx / w;
            let li = lbl as usize;
            counts[li] += 1;
            let c = pixels[idx];
            color_sum[li][0] += c[0] as u64;
            color_sum[li][1] += c[1] as u64;
            color_sum[li][2] += c[2] as u64;
            cx_sum[li]       += px as u64;
            cy_sum[li]       += py as u64;

            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let nx = px as i32 + dx;
                let ny = py as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 { continue; }
                let nidx = ny as usize * w + nx as usize;
                if labels[nidx] != -1 { continue; }
                if color_dist(seed, pixels[nidx]) <= tol {
                    labels[nidx] = lbl;
                    q.push_back(nidx);
                }
            }
        }
    }

    let px_per_cm2 = scale * scale;
    let mut id_map  = vec![-1i32; next_lbl];
    let mut regions: Vec<Region> = Vec::new();
    let mut new_id  = 0usize;

    for l in 0..next_lbl {
        if counts[l] < min_px { continue; }
        id_map[l] = new_id as i32;
        let cnt = counts[l];
        let cs  = color_sum[l];
        let avg = [
            (cs[0] / cnt as u64) as u8,
            (cs[1] / cnt as u64) as u8,
            (cs[2] / cnt as u64) as u8,
        ];
        let centroid = (
            cx_sum[l] as f32 / (cnt as f32 * w as f32),
            cy_sum[l] as f32 / (cnt as f32 * h as f32),
        );
        regions.push(Region {
            index: new_id + 1,
            pixel_count: cnt,
            area_cm2: cnt as f64 / px_per_cm2,
            avg_color: avg,
            centroid,
        });
        new_id += 1;
    }

    for lbl in labels.iter_mut() {
        if *lbl >= 0 { *lbl = id_map[*lbl as usize]; }
    }

    (labels, regions)
}

fn build_seg_texture(
    labels:   &[i32],
    w: u32, h: u32,
    n: usize,
    selected: &HashSet<usize>,
) -> ColorImage {
    let palette: Vec<egui::Color32> = (0..n)
        .map(|i| {
            let rgb = hsv_to_rgb(i as f32 * 360.0 / n.max(1) as f32, 0.75, 0.90);
            egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])
        })
        .collect();

    let has_sel = !selected.is_empty();
    let pixels  = labels.iter().map(|&l| {
        if l < 0 || l as usize >= n { return egui::Color32::from_gray(20); }
        let idx = l as usize;
        let c   = palette[idx];
        if has_sel && !selected.contains(&idx) {
            let [r, g, b, _] = c.to_array();
            egui::Color32::from_rgb(r / 6, g / 6, b / 6)
        } else { c }
    }).collect();

    ColorImage { size: [w as usize, h as usize], pixels }
}

/// Count pixels in the image that match a given filter, return area in cm².
fn pixel_area_for_filter(img: &DynamicImage, filter: &ColorFilter, scale_px_per_cm: f64) -> (usize, f64) {
    let rgb = img.to_rgb8();
    let count = rgb.pixels()
        .filter(|p| pixel_matches_filter(p[0], p[1], p[2], filter))
        .count();
    let area = count as f64 / (scale_px_per_cm * scale_px_per_cm);
    (count, area)
}


/// Matched pixels keep their original color; everything else is fully black.
fn build_color_filter_texture(img: &DynamicImage, filters: &[&ColorFilter]) -> ColorImage {
    let rgb = img.to_rgb8();
    let w = rgb.width() as usize;
    let h = rgb.height() as usize;
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

fn dyn_to_color_image(img: &DynamicImage) -> ColorImage {
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let pixels = rgba.pixels()
        .map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
        .collect();
    ColorImage { size: [w as usize, h as usize], pixels }
}

fn export_csv(regions: &[Region], unit: &Unit) -> String {
    if let Some(path) = FileDialog::new()
        .set_file_name("regions.csv")
        .add_filter("CSV", &["csv"])
        .save_file()
    {
        match std::fs::File::create(&path) {
            Ok(mut f) => {
                let _ = writeln!(f, "Region,Pixels,Area ({}),Avg R,Avg G,Avg B", unit.label());
                for r in regions {
                    let _ = writeln!(f, "{},{},{:.4},{},{},{}",
                                     r.index, r.pixel_count, r.area_cm2 * unit.factor(),
                                     r.avg_color[0], r.avg_color[1], r.avg_color[2]);
                }
                format!("Exported to {}", path.display())
            }
            Err(e) => format!("Export failed: {e}"),
        }
    } else {
        "Export cancelled.".into()
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        // ── TOP TOOLBAR ──────────────────────────────────────────────────────
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(5.0);

            ui.horizontal_wrapped(|ui| {

                if ui.button("📂  Load Image").clicked() {
                    if let Some(path) = FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "tiff", "webp"])
                        .pick_file()
                    {
                        match image::open(&path) {
                            Ok(img) => {
                                let ci = dyn_to_color_image(&img);
                                self.orig_tex        = Some(ctx.load_texture("orig", ci, TextureOptions::default()));
                                self.img_w           = img.width();
                                self.img_h           = img.height();
                                self.image           = Some(img);
                                self.seg_tex         = None;
                                self.edge_tex        = None;
                                self.color_filter_tex = None;
                                self.show_seg        = false;
                                self.show_edges      = false;
                                self.active_color_filters.clear();
                                self.scale_px_per_cm = None;
                                self.label_map.clear();
                                self.regions.clear();
                                self.selected.clear();
                                self.total_area_cm2  = 0.0;
                                self.mode            = Mode::Ready;
                                self.status          = format!(
                                    "Loaded ({} × {} px). Step 2 – Set Scale.",
                                    self.img_w, self.img_h
                                );
                            }
                            Err(e) => self.status = format!("Error: {e}"),
                        }
                    }
                }

                ui.separator();

                match self.mode.clone() {
                    Mode::CalibP1 => {
                        ui.colored_label(egui::Color32::YELLOW, "🎯 Click FIRST endpoint on image");
                        if ui.button("✖ Cancel").clicked() { self.mode = Mode::Ready; }
                    }
                    Mode::CalibP2 { .. } => {
                        ui.colored_label(egui::Color32::YELLOW, "🎯 Click SECOND endpoint on image");
                        if ui.button("✖ Cancel").clicked() { self.mode = Mode::Ready; }
                    }
                    Mode::CalibLen { p1, p2 } => {
                        ui.label("Line length:");
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.calib_len_buf)
                                .desired_width(65.0)
                                .hint_text("e.g. 5.0"),
                        );
                        ui.label("cm");
                        let confirmed = ui.button("✔ Confirm").clicked()
                            || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                        if confirmed {
                            match self.calib_len_buf.trim().parse::<f64>() {
                                Ok(len) if len > 0.0 => {
                                    let px_dist = norm_to_px_dist(p1, p2, self.img_w, self.img_h);
                                    let scale   = px_dist / len;
                                    self.scale_px_per_cm = Some(scale);
                                    self.mode            = Mode::Ready;
                                    self.calib_len_buf.clear();
                                    self.status = format!(
                                        "Scale set: {:.3} px/cm ({:.5} cm/px). Step 3 – Segment.",
                                        scale, 1.0 / scale
                                    );
                                }
                                Ok(_)  => self.status = "Length must be > 0.".into(),
                                Err(_) => self.status = "Enter a valid decimal number.".into(),
                            }
                        }
                        if ui.button("✖ Cancel").clicked() {
                            self.mode = Mode::Ready;
                            self.calib_len_buf.clear();
                        }
                    }
                    _ => {
                        let enabled = self.image.is_some();
                        if ui.add_enabled(enabled, egui::Button::new("📏  Set Scale"))
                            .on_hover_text("Draw a line over a known reference length to calibrate")
                            .clicked()
                        {
                            self.mode   = Mode::CalibP1;
                            self.status = "Click the first endpoint of your reference line.".into();
                        }
                    }
                }

                if let Some(s) = self.scale_px_per_cm {
                    ui.colored_label(
                        egui::Color32::from_rgb(100, 220, 100),
                        format!("✔ {:.3} px/cm", s),
                    );
                }

                ui.separator();

                let can_seg = self.image.is_some()
                    && self.scale_px_per_cm.is_some()
                    && !matches!(self.mode, Mode::CalibP1 | Mode::CalibP2 { .. } | Mode::CalibLen { .. });

                if ui.add_enabled(can_seg, egui::Button::new("⚙  Segment"))
                    .on_hover_text("Detect coloured regions and compute their areas")
                    .clicked()
                {
                    if let (Some(img), Some(scale)) = (&self.image, self.scale_px_per_cm) {
                        let processed = box_blur(img, self.blur_radius);
                        let (labels, regions) =
                            segment(&processed, self.tolerance, self.min_pixels, scale);
                        let n = regions.len();

                        let ci_seg  = build_seg_texture(&labels, self.img_w, self.img_h, n, &HashSet::new());
                        let ci_edge = sobel_texture(&processed);

                        self.seg_tex  = Some(ctx.load_texture("seg",  ci_seg,  TextureOptions::default()));
                        self.edge_tex = Some(ctx.load_texture("edge", ci_edge, TextureOptions::default()));

                        self.total_area_cm2 = regions.iter().map(|r| r.area_cm2).sum();
                        self.label_map      = labels;
                        self.regions        = regions;
                        self.selected.clear();
                        self.show_seg       = true;
                        self.show_edges     = false;
                        self.mode           = Mode::Segmented;
                        self.status         = format!(
                            "Done — {n} region(s) found. Click any region to select it."
                        );
                    }
                }

                ui.separator();

                if self.seg_tex.is_some() {
                    ui.checkbox(&mut self.show_seg,   "Segmented view");
                    ui.checkbox(&mut self.show_edges, "Edge overlay");
                    ui.separator();
                }

                ui.label("Unit:");
                egui::ComboBox::from_id_salt("unit_sel")
                    .selected_text(self.unit.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.unit, Unit::Cm2, "cm²");
                        ui.selectable_value(&mut self.unit, Unit::Mm2, "mm²");
                    });
            });

            ui.add_space(3.0);

            // Row 2: params + selection controls
            ui.horizontal_wrapped(|ui| {
                ui.label("Colour tol:");
                // ── BUMPED: 5..=150 → 5..=255 ──
                ui.add(egui::Slider::new(&mut self.tolerance,   5..=255).clamp_to_range(true));

                ui.label("Min px:");
                ui.add(egui::Slider::new(&mut self.min_pixels, 50..=50_000).clamp_to_range(true));

                ui.label("Blur:");
                // ── BUMPED: 0..=5 → 0..=15 ──
                ui.add(egui::Slider::new(&mut self.blur_radius, 0..=15).clamp_to_range(true))
                    .on_hover_text("Box blur radius applied before segmentation — reduces noise (0 = off)");

                if !self.regions.is_empty() {
                    ui.separator();

                    if ui.button("☑ Select All").clicked() {
                        self.selected = (0..self.regions.len()).collect();
                        let n  = self.regions.len();
                        let ci = build_seg_texture(&self.label_map, self.img_w, self.img_h, n, &self.selected);
                        self.seg_tex = Some(ctx.load_texture("seg", ci, TextureOptions::default()));
                    }

                    if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("✖ Clear Sel.")).clicked() {
                        self.selected.clear();
                        let n  = self.regions.len();
                        let ci = build_seg_texture(&self.label_map, self.img_w, self.img_h, n, &self.selected);
                        self.seg_tex = Some(ctx.load_texture("seg", ci, TextureOptions::default()));
                    }

                    ui.separator();

                    if ui.button("💾  Export CSV").clicked() {
                        self.status = export_csv(&self.regions, &self.unit);
                    }
                }
            });

            ui.add_space(4.0);
        });

        // ── COLOR FILTER SIDE PANEL (top-right) ──────────────────────────────
        egui::SidePanel::right("color_filter_panel")
            .resizable(false)
            .min_width(130.0)
            .max_width(130.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.label(egui::RichText::new("🎨 Color Filter").strong());
                ui.separator();

                let has_image = self.image.is_some();

                for (i, filter) in self.color_filters.clone().iter().enumerate() {
                    let is_active = self.active_color_filters.contains(&i);

                    let btn_text = egui::RichText::new(filter.label)
                        .strong()
                        .color(if is_active { egui::Color32::BLACK } else { egui::Color32::WHITE });

                    let bg_color = filter.swatch;
                    let frame_color = if is_active {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_black_alpha(0)
                    };

                    let btn = egui::Button::new(btn_text)
                        .fill(bg_color)
                        .stroke(egui::Stroke::new(if is_active { 2.5 } else { 0.0 }, frame_color))
                        .min_size(Vec2::new(115.0, 22.0));

                    let resp = ui.add_enabled(has_image, btn);

                    if resp.clicked() {
                        if is_active {
                            self.active_color_filters.remove(&i);
                        } else {
                            self.active_color_filters.insert(i);
                        }
                        // Rebuild texture from all currently active filters
                        if self.active_color_filters.is_empty() {
                            self.color_filter_tex = None;
                        } else if let Some(img) = &self.image {
                            let active_refs: Vec<&ColorFilter> = self.active_color_filters
                                .iter()
                                .map(|&idx| &self.color_filters[idx])
                                .collect();
                            let ci = build_color_filter_texture(img, &active_refs);
                            self.color_filter_tex = Some(
                                ctx.load_texture("cf", ci, TextureOptions::default())
                            );
                        }
                    }

                    ui.add_space(2.0);
                }

                ui.separator();
                if !self.active_color_filters.is_empty() {
                    if ui.button("✖  Clear filters").clicked() {
                        self.active_color_filters.clear();
                        self.color_filter_tex = None;
                    }
                } else {
                    ui.label(egui::RichText::new("No filter active").italics().small()
                        .color(egui::Color32::GRAY));
                }
            });

        // ── BOTTOM RESULTS PANEL ──────────────────────────────────────────────
        egui::TopBottomPanel::bottom("results").min_height(185.0).show(ctx, |ui| {
            ui.add_space(5.0);
            ui.label(egui::RichText::new(&self.status).italics().small());

            let filter_active = !self.active_color_filters.is_empty();

            if filter_active {
                // ── FILTER MODE: one clean row per selected color ──
                ui.separator();

                if let Some(img) = &self.image {
                    if let Some(scale) = self.scale_px_per_cm {
                        let factor   = self.unit.factor();
                        let unit_lbl = self.unit.label();

                        // Collect results for each active filter, sorted by filter index so order is stable
                        let mut filter_indices: Vec<usize> = self.active_color_filters.iter().copied().collect();
                        filter_indices.sort();

                        egui::Grid::new("filter_area_table")
                            .num_columns(3)
                            .striped(false)
                            .spacing([20.0, 6.0])
                            .show(ui, |ui| {
                                ui.strong("Color");
                                ui.strong(format!("Area ({})", unit_lbl));
                                ui.strong("Pixels");
                                ui.end_row();

                                for fi in &filter_indices {
                                    let f = &self.color_filters[*fi];
                                    let (px_count, area_cm2) = pixel_area_for_filter(img, f, scale);

                                    // Color swatch + label
                                    ui.horizontal(|ui| {
                                        let (sw, _) = ui.allocate_exact_size(Vec2::new(18.0, 18.0), egui::Sense::hover());
                                        ui.painter().rect_filled(sw, 3.0, f.swatch);
                                        ui.label(egui::RichText::new(f.label).strong().size(15.0));
                                    });

                                    ui.label(egui::RichText::new(
                                        format!("{:.4}", area_cm2 * factor)
                                    ).size(15.0).strong());

                                    ui.label(egui::RichText::new(
                                        px_count.to_string()
                                    ).size(13.0).color(egui::Color32::GRAY));

                                    ui.end_row();
                                }
                            });
                    }
                }
            } else if !self.regions.is_empty() {
                // ── NORMAL MODE: full region table ──
                let factor   = self.unit.factor();
                let unit_lbl = self.unit.label();

                let sel_area: Option<f64> = if self.selected.is_empty() {
                    None
                } else {
                    Some(
                        self.regions.iter()
                            .filter(|r| self.selected.contains(&(r.index - 1)))
                            .map(|r| r.area_cm2)
                            .sum(),
                    )
                };

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "Total area: {:.4} {}   |   {} region(s)",
                            self.total_area_cm2 * factor, unit_lbl, self.regions.len()
                        )).strong().size(14.0),
                    );
                    if let Some(sa) = sel_area {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(format!(
                                "Selected: {:.4} {}  ({} region(s))",
                                sa * factor, unit_lbl, self.selected.len()
                            ))
                                .strong().size(14.0)
                                .color(egui::Color32::from_rgb(255, 210, 60)),
                        );
                    }
                });

                ui.separator();

                ScrollArea::vertical()
                    .max_height(110.0)
                    .id_salt("rtbl")
                    .show(ui, |ui| {
                        egui::Grid::new("region_table")
                            .num_columns(5)
                            .striped(true)
                            .spacing([14.0, 4.0])
                            .show(ui, |ui| {
                                ui.strong("Region");
                                ui.strong("Colour");
                                ui.strong("Avg RGB");
                                ui.strong("Pixels");
                                ui.strong(format!("Area ({})", unit_lbl));
                                ui.end_row();

                                for r in &self.regions {
                                    let is_sel = self.selected.contains(&(r.index - 1));
                                    let [cr, cg, cb] = r.avg_color;

                                    let label_text = if is_sel {
                                        egui::RichText::new(format!("#{}", r.index))
                                            .color(egui::Color32::from_rgb(255, 210, 60)).strong()
                                    } else {
                                        egui::RichText::new(format!("#{}", r.index))
                                    };
                                    ui.label(label_text);

                                    let (sw, _) = ui.allocate_exact_size(Vec2::new(30.0, 16.0), egui::Sense::hover());
                                    ui.painter().rect_filled(sw, 3.0, egui::Color32::from_rgb(cr, cg, cb));

                                    ui.label(format!("({cr},{cg},{cb})"));
                                    ui.label(r.pixel_count.to_string());
                                    ui.label(format!("{:.4}", r.area_cm2 * factor));
                                    ui.end_row();
                                }
                            });
                    });
            }
        });

        // ── CENTRAL IMAGE PANEL ───────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            let tex_ref = if self.show_seg {
                self.seg_tex.as_ref().or(self.orig_tex.as_ref())
            } else {
                self.orig_tex.as_ref()
            };

            let tex = match tex_ref {
                None => {
                    ui.centered_and_justified(|ui| {
                        ui.label(egui::RichText::new(
                            "No image loaded.\n\nClick  📂 Load Image  to begin."
                        ).size(20.0).color(egui::Color32::GRAY));
                    });
                    return;
                }
                Some(t) => t,
            };

            let avail    = ui.available_size();
            let tex_size = tex.size_vec2();
            let fit      = (avail.x / tex_size.x).min(avail.y / tex_size.y);
            let disp     = tex_size * fit;
            let img_rect = Rect::from_center_size(ui.max_rect().center(), disp);
            self.img_rect = img_rect;

            let sense = match &self.mode {
                Mode::CalibP1 | Mode::CalibP2 { .. } | Mode::Segmented => egui::Sense::click(),
                _ => egui::Sense::hover(),
            };
            let response = ui.allocate_rect(img_rect, sense);

            match &self.mode {
                Mode::CalibP1 | Mode::CalibP2 { .. } => ctx.set_cursor_icon(egui::CursorIcon::Crosshair),
                Mode::Segmented => ctx.set_cursor_icon(egui::CursorIcon::PointingHand),
                _ => {}
            }

            // When color filters are active, show ONLY the filter texture (matched pixels in
            // original color, everything else black). Otherwise show the normal base image.
            if !self.active_color_filters.is_empty() {
                if let Some(cf_tex) = &self.color_filter_tex {
                    ui.painter().image(
                        cf_tex.id(), img_rect,
                        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
            } else {
                ui.painter().image(
                    tex.id(), img_rect,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }

            if self.show_edges {
                if let Some(et) = &self.edge_tex {
                    ui.painter().image(
                        et.id(), img_rect,
                        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
            }

            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let norm = screen_to_norm(pos, img_rect);

                    match self.mode.clone() {
                        Mode::CalibP1 => {
                            self.mode   = Mode::CalibP2 { p1: norm };
                            self.status = "Now click the second endpoint.".into();
                        }
                        Mode::CalibP2 { p1 } => {
                            self.mode   = Mode::CalibLen { p1, p2: norm };
                            self.status = "Enter the length of this line in the toolbar above.".into();
                        }
                        Mode::Segmented => {
                            let px  = ((norm.x * self.img_w as f32) as usize).min(self.img_w as usize - 1);
                            let py  = ((norm.y * self.img_h as f32) as usize).min(self.img_h as usize - 1);
                            let lbl = self.label_map.get(py * self.img_w as usize + px).copied();

                            if let Some(l) = lbl {
                                if l >= 0 {
                                    let ri = l as usize;
                                    if self.selected.contains(&ri) {
                                        self.selected.remove(&ri);
                                    } else {
                                        self.selected.insert(ri);
                                    }
                                    let n  = self.regions.len();
                                    let ci = build_seg_texture(
                                        &self.label_map, self.img_w, self.img_h, n, &self.selected,
                                    );
                                    self.seg_tex  = Some(ctx.load_texture("seg", ci, TextureOptions::default()));
                                    self.show_seg = true;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            let painter = ui.painter();
            let dot = |p: Pos2| {
                let s = norm_to_screen(p, img_rect);
                painter.circle_filled(s, 7.0, egui::Color32::from_rgb(255, 215, 0));
                painter.circle_stroke(s, 7.0, egui::Stroke::new(2.0, egui::Color32::BLACK));
            };

            match &self.mode {
                Mode::CalibP2 { p1 } => { dot(*p1); }
                Mode::CalibLen { p1, p2 } => {
                    let s1 = norm_to_screen(*p1, img_rect);
                    let s2 = norm_to_screen(*p2, img_rect);
                    painter.line_segment([s1, s2], egui::Stroke::new(2.5, egui::Color32::from_rgb(255, 215, 0)));
                    dot(*p1); dot(*p2);
                    let mid = Pos2::new((s1.x + s2.x) / 2.0, (s1.y + s2.y) / 2.0 - 18.0);
                    painter.rect_filled(
                        Rect::from_center_size(mid, Vec2::new(195.0, 22.0)),
                        4.0, egui::Color32::from_black_alpha(175),
                    );
                    painter.text(mid, egui::Align2::CENTER_CENTER,
                                 "Enter length in toolbar ↑",
                                 egui::FontId::proportional(13.0), egui::Color32::YELLOW);
                }
                _ => {}
            }

            if self.show_seg {
                let font = egui::FontId::proportional(14.0);
                for r in &self.regions {
                    let cx  = img_rect.min.x + r.centroid.0 * disp.x;
                    let cy  = img_rect.min.y + r.centroid.1 * disp.y;
                    let lbl = r.index.to_string();
                    painter.text(egui::pos2(cx+1.0, cy+1.0), egui::Align2::CENTER_CENTER,
                                 &lbl, font.clone(), egui::Color32::BLACK);
                    painter.text(egui::pos2(cx, cy), egui::Align2::CENTER_CENTER,
                                 &lbl, font.clone(), egui::Color32::WHITE);
                }
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Image Segmenter",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("Image Segmenter")
                .with_inner_size([1250.0, 860.0]),
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}