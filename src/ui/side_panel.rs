use egui::{TextureOptions, Vec2};

use crate::app::App;
use crate::color::build_color_filter_texture;

pub fn show(app: &mut App, ctx: &egui::Context) {
    egui::SidePanel::right("color_filter_panel")
        .resizable(false)
        .min_width(140.0)
        .max_width(140.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("🎨 Color Filter").strong());
            ui.separator();

            let has_image = app.image.is_some();

            if !has_image {
                ui.label(
                    egui::RichText::new("Load an image to\ndetect colors.")
                        .italics()
                        .small()
                        .color(egui::Color32::GRAY),
                );
                return;
            }

            // Decide which filter indices to show in the main list
            let indices_to_show: Vec<usize> = if app.show_all_colors {
                (0..app.color_filters.len()).collect()
            } else {
                app.prominent_filter_indices.clone()
            };

            if indices_to_show.is_empty() && !app.show_all_colors {
                ui.label(
                    egui::RichText::new("No dominant colors\ndetected (< 5%).")
                        .italics()
                        .small()
                        .color(egui::Color32::GRAY),
                );
            } else {
                for i in indices_to_show {
                    let filter    = app.color_filters[i].clone();
                    let is_active = app.active_color_filters.contains(&i);

                    let btn_text = egui::RichText::new(filter.label)
                        .strong()
                        .color(if is_active { egui::Color32::BLACK } else { egui::Color32::WHITE });

                    let btn = egui::Button::new(btn_text)
                        .fill(filter.swatch)
                        .stroke(egui::Stroke::new(
                            if is_active { 2.5 } else { 0.0 },
                            egui::Color32::WHITE,
                        ))
                        .min_size(Vec2::new(125.0, 22.0));

                    if ui.add(btn).clicked() {
                        if is_active {
                            app.active_color_filters.remove(&i);
                        } else {
                            app.active_color_filters.insert(i);
                        }
                        rebuild_filter_texture(app, ctx);
                    }

                    ui.add_space(2.0);
                }
            }

            ui.add_space(4.0);
            ui.separator();

            let toggle_label = if app.show_all_colors {
                "▲ Show detected only"
            } else {
                "▼ Show all colors"
            };
            if ui.button(egui::RichText::new(toggle_label).small()).clicked() {
                app.show_all_colors = !app.show_all_colors;
            }

            ui.add_space(4.0);
            ui.separator();

            if !app.active_color_filters.is_empty() {
                if ui.button("✖  Clear filters").clicked() {
                    app.active_color_filters.clear();
                    app.color_filter_tex = None;
                }
            } else {
                ui.label(
                    egui::RichText::new("No filter active")
                        .italics()
                        .small()
                        .color(egui::Color32::GRAY),
                );
            }
        });
}

pub fn rebuild_filter_texture(app: &mut App, ctx: &egui::Context) {
    if app.active_color_filters.is_empty() {
        app.color_filter_tex = None;
        return;
    }
    if let Some(img) = &app.image {
        let active_refs: Vec<&_> = app.active_color_filters
            .iter()
            .map(|&idx| &app.color_filters[idx])
            .collect();
        let ci = build_color_filter_texture(img, &active_refs);
        app.color_filter_tex = Some(ctx.load_texture("cf", ci, TextureOptions::default()));
    }
}
