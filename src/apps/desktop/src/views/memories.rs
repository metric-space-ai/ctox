//! Memories (Stage1 Outputs) card grid with detail viewer.

use crate::db_reader::Stage1Row;
use eframe::egui::{self, Color32, Frame, RichText, ScrollArea, Stroke, Ui};

#[derive(Default)]
pub struct MemoriesState {
    pub selected_index: Option<usize>,
}

pub fn render(ui: &mut Ui, memories: &[Stage1Row], state: &mut MemoriesState) {
    ui.heading(
        RichText::new("Summaries (Stage 1)")
            .size(18.0)
            .color(Color32::from_gray(220)),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("{} memory outputs", memories.len()))
            .size(12.5)
            .color(Color32::from_gray(130)),
    );
    ui.add_space(8.0);

    if memories.is_empty() {
        ui.label(RichText::new("No stage1 outputs found.").color(Color32::from_gray(120)));
        return;
    }

    let available = ui.available_width();
    let grid_width = (available * 0.45).max(250.0);

    ui.horizontal_top(|ui| {
        // Left: Card grid
        Frame::default()
            .fill(Color32::from_rgb(16, 18, 22))
            .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 50)))
            .corner_radius(10.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(grid_width);
                ScrollArea::vertical()
                    .max_height(ui.available_height().max(400.0))
                    .id_salt("memories-grid")
                    .show(ui, |ui| {
                        for (i, memory) in memories.iter().enumerate() {
                            let is_selected = state.selected_index == Some(i);
                            let fill = if is_selected {
                                Color32::from_rgb(34, 40, 50)
                            } else {
                                Color32::from_rgb(24, 27, 32)
                            };
                            let resp = Frame::default()
                                .fill(fill)
                                .stroke(Stroke::new(
                                    1.0,
                                    if is_selected {
                                        Color32::from_rgb(86, 182, 214)
                                    } else {
                                        Color32::from_rgb(40, 44, 50)
                                    },
                                ))
                                .corner_radius(10.0)
                                .inner_margin(10.0)
                                .show(ui, |ui| {
                                    let title =
                                        memory.thread_title.as_deref().unwrap_or(&memory.thread_id);
                                    ui.label(
                                        RichText::new(truncate(title, 36))
                                            .size(13.0)
                                            .strong()
                                            .color(Color32::from_gray(210)),
                                    );
                                    ui.horizontal(|ui| {
                                        if let Some(slug) = &memory.rollout_slug {
                                            badge(ui, slug, Color32::from_gray(120));
                                        }
                                        if memory.selected_for_phase2 {
                                            badge(ui, "Phase 2", Color32::from_rgb(86, 182, 214));
                                        }
                                        if let Some(usage) = memory.usage_count {
                                            badge(
                                                ui,
                                                &format!("used {}", usage),
                                                Color32::from_gray(100),
                                            );
                                        }
                                    });
                                    ui.label(
                                        RichText::new(format_ts(memory.generated_at))
                                            .size(11.0)
                                            .color(Color32::from_gray(100)),
                                    );
                                });
                            if resp.response.interact(egui::Sense::click()).clicked() {
                                state.selected_index = Some(i);
                            }
                            ui.add_space(4.0);
                        }
                    });
            });

        // Right: Detail viewer
        Frame::default()
            .fill(Color32::from_rgb(20, 22, 26))
            .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 50)))
            .corner_radius(10.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width((available - grid_width - 24.0).max(80.0));
                render_detail(ui, memories, state);
            });
    });
}

fn render_detail(ui: &mut Ui, memories: &[Stage1Row], state: &MemoriesState) {
    let Some(idx) = state.selected_index else {
        ui.add_space(40.0);
        ui.label(RichText::new("Select a memory to see details.").color(Color32::from_gray(120)));
        return;
    };
    let Some(memory) = memories.get(idx) else {
        ui.label(RichText::new("Memory not found.").color(Color32::from_gray(120)));
        return;
    };

    let title = memory.thread_title.as_deref().unwrap_or(&memory.thread_id);
    ui.label(
        RichText::new(title)
            .size(15.0)
            .strong()
            .color(Color32::from_gray(220)),
    );
    ui.add_space(8.0);

    ScrollArea::vertical()
        .max_height(ui.available_height().max(300.0))
        .id_salt("memory-detail")
        .show(ui, |ui| {
            ui.label(
                RichText::new("Rollout Summary")
                    .size(13.0)
                    .strong()
                    .color(Color32::from_rgb(86, 182, 214)),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(&memory.rollout_summary)
                    .size(12.0)
                    .color(Color32::from_gray(190)),
            );
            ui.add_space(12.0);
            ui.label(
                RichText::new("Raw Memory")
                    .size(13.0)
                    .strong()
                    .color(Color32::from_rgb(180, 120, 200)),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(&memory.raw_memory)
                    .size(12.0)
                    .color(Color32::from_gray(190)),
            );
        });
}

fn badge(ui: &mut Ui, text: &str, color: Color32) {
    Frame::default()
        .fill(Color32::from_rgb(36, 40, 48))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(color));
        });
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else {
        s.chars().take(max).collect::<String>() + "..."
    }
}

fn format_ts(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| ts.to_string())
}
