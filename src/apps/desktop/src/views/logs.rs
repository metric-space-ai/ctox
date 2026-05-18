//! Log stream viewer with level and thread filtering.

use crate::db_reader::LogRow;
use eframe::egui::{self, Color32, Frame, RichText, ScrollArea, Stroke, Ui};

#[derive(Default)]
pub struct LogsState {
    pub level_filter: String,
    pub thread_filter: String,
    pub auto_scroll: bool,
}

const COL_ERROR: Color32 = Color32::from_rgb(218, 106, 106);
const COL_WARN: Color32 = Color32::from_rgb(220, 190, 90);
const COL_INFO: Color32 = Color32::from_rgb(200, 200, 200);
const COL_DEBUG: Color32 = Color32::from_rgb(120, 120, 120);
const COL_TRACE: Color32 = Color32::from_rgb(90, 90, 90);

fn level_color(level: &str) -> Color32 {
    match level.to_uppercase().as_str() {
        "ERROR" => COL_ERROR,
        "WARN" => COL_WARN,
        "INFO" => COL_INFO,
        "DEBUG" => COL_DEBUG,
        "TRACE" => COL_TRACE,
        _ => COL_INFO,
    }
}

pub fn render(ui: &mut Ui, logs: &[LogRow], state: &mut LogsState) {
    ui.heading(
        RichText::new("Logs")
            .size(18.0)
            .color(Color32::from_gray(220)),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("{} entries (newest first)", logs.len()))
            .size(12.5)
            .color(Color32::from_gray(130)),
    );
    ui.add_space(8.0);

    // Filters
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Level:")
                .size(12.0)
                .color(Color32::from_gray(140)),
        );
        for level in &["", "ERROR", "WARN", "INFO", "DEBUG"] {
            let label = if level.is_empty() { "All" } else { level };
            let selected = state.level_filter == *level;
            if ui.selectable_label(selected, label).clicked() {
                state.level_filter = level.to_string();
            }
        }
        ui.add_space(16.0);
        ui.label(
            RichText::new("Thread:")
                .size(12.0)
                .color(Color32::from_gray(140)),
        );
        ui.add(
            egui::TextEdit::singleline(&mut state.thread_filter)
                .desired_width(120.0)
                .hint_text("thread ID"),
        );
        ui.add_space(16.0);
        ui.checkbox(&mut state.auto_scroll, "Auto-scroll");
    });
    ui.add_space(8.0);

    if logs.is_empty() {
        ui.label(RichText::new("No logs found.").color(Color32::from_gray(120)));
        return;
    }

    Frame::default()
        .fill(Color32::from_rgb(12, 14, 18))
        .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 50)))
        .corner_radius(10.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ScrollArea::vertical()
                .max_height(ui.available_height().max(400.0))
                .stick_to_bottom(state.auto_scroll)
                .id_salt("logs-scroll")
                .show(ui, |ui| {
                    for log in logs {
                        let ts = format_ts(log.ts, log.ts_nanos);
                        let color = level_color(&log.level);
                        let message = log.message.as_deref().unwrap_or("");

                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(&ts)
                                    .size(11.0)
                                    .family(egui::FontFamily::Monospace)
                                    .color(Color32::from_gray(100)),
                            );
                            ui.label(
                                RichText::new(format!("{:<5}", log.level))
                                    .size(11.0)
                                    .family(egui::FontFamily::Monospace)
                                    .color(color),
                            );
                            ui.label(
                                RichText::new(&log.target)
                                    .size(11.0)
                                    .family(egui::FontFamily::Monospace)
                                    .color(Color32::from_gray(130)),
                            );
                            ui.label(
                                RichText::new(message)
                                    .size(11.0)
                                    .family(egui::FontFamily::Monospace)
                                    .color(Color32::from_gray(190)),
                            );
                        });
                    }
                });
        });
}

fn format_ts(ts: i64, ts_nanos: i64) -> String {
    chrono::DateTime::from_timestamp(ts, ts_nanos as u32)
        .map(|d| d.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| format!("{}.{}", ts, ts_nanos))
}
