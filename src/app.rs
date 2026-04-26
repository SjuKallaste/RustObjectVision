use egui::{Rect, TextureHandle};
use image::DynamicImage;
use std::collections::HashSet;

use crate::color::{all_color_filters, ColorFilter};
use crate::types::{Mode, Region, Unit};

pub struct App {
    pub image:  Option<DynamicImage>,
    pub img_w:  u32,
    pub img_h:  u32,

    pub orig_tex:         Option<TextureHandle>,
    pub seg_tex:          Option<TextureHandle>,
    pub edge_tex:         Option<TextureHandle>,
    pub color_filter_tex: Option<TextureHandle>,

    pub img_rect: Rect,

    pub show_seg:   bool,
    pub show_edges: bool,

    pub mode:            Mode,
    pub calib_len_buf:   String,
    pub scale_px_per_cm: Option<f64>,

    pub tolerance:   u32,
    pub min_pixels:  usize,
    pub blur_radius: u32,

    pub label_map:      Vec<i32>,
    pub regions:        Vec<Region>,
    pub selected:       HashSet<usize>,
    pub total_area_cm2: f64,
    pub unit:           Unit,

    pub color_filters:        Vec<ColorFilter>,
    pub active_color_filters: HashSet<usize>,

    pub status: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            image:  None,
            img_w:  0,
            img_h:  0,
            orig_tex:         None,
            seg_tex:          None,
            edge_tex:         None,
            color_filter_tex: None,
            img_rect:         Rect::NOTHING,
            show_seg:         false,
            show_edges:       false,
            mode:             Mode::Idle,
            calib_len_buf:    String::new(),
            scale_px_per_cm:  None,
            tolerance:        30,
            min_pixels:       200,
            blur_radius:      0,
            label_map:        Vec::new(),
            regions:          Vec::new(),
            selected:         HashSet::new(),
            total_area_cm2:   0.0,
            unit:             Unit::Cm2,
            color_filters:        all_color_filters(),
            active_color_filters: HashSet::new(),
            status: "Step 1: Load an image.".into(),
        }
    }
}
