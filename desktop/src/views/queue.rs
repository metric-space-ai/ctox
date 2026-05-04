//! Queue view: communication messages (email, teams, tui) with channel/status filter.

use eframe::egui::{self, Color32, Frame, RichText, ScrollArea, Stroke, Ui};
use crate::db_reader::CommMessageRow;

#[derive(Default)]
pub struct QueueState {
    pub channel_filter: String,
    pub status_filter: String,
    pub selected_key: Option<String>,
}

fn status_color(status: &str) -> Color32 {
    match status {
        "pending" => Color32::from_rgb(86, 182, 214),
        "leased" => Color32::from_rgb(220, 190, 90),
        "handled" => Color32::from_rgb(94, 184, 116),
        "blocked_sender" | "blocked" => Color32::from_rgb(218, 106, 106),
        "failed" => Color32::from_rgb(180, 80, 80),
        _ => Color32::from_gray(120),
    }
}

fn channel_icon(channel: &str) -> &'static str {
    match channel {
        "email" => "E",
        "teams" => "T",
        "tui" => "C",
        "whatsapp" => "W",
        _ => "?",
    }
}

fn channel_color(channel: &str) -> Color32 {
    match channel {
        "email" => Color32::from_rgb(130, 170, 210),
        "teams" => Color32::from_rgb(120, 100, 200),
        "tui" => Color32::from_rgb(120, 210, 170),
        "whatsapp" => Color32::from_rgb(80, 190, 120),
        _ => Color32::from_gray(130),
    }
}

pub fn render(ui: &mut Ui, messages: &[CommMessageRow], state: &mut QueueState) {
    ui.heading(RichText::new("Queue").size(18.0).color(Color32::from_gray(220)));
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("{} messages", messages.len()))
            .size(12.5)
            .color(Color32::from_gray(130)),
    );
    ui.add_space(8.0);

    // Filters
    ui.horizontal(|ui| {
        ui.label(RichText::new("Channel:").size(12.0).color(Color32::from_gray(140)));
        for ch in &["", "email", "teams", "tui"] {
            let label = if ch.is_empty() { "All" } else { ch };
            let selected = state.channel_filter == *ch;
            if ui.selectable_label(selected, label).clicked() {
                state.channel_filter = ch.to_string();
            }
        }
        ui.add_space(16.0);
        ui.label(RichText::new("Status:").size(12.0).color(Color32::from_gray(140)));
        for st in &["", "pending", "leased", "handled", "blocked_sender", "failed"] {
            let label = if st.is_empty() { "All" } else { *st };
            let selected = state.status_filter == *st;
            if ui.selectable_label(selected, label).clicked() {
                state.status_filter = st.to_string();
            }
        }
    });
    ui.add_space(8.0);

    let filtered: Vec<&CommMessageRow> = messages
        .iter()
        .filter(|m| state.channel_filter.is_empty() || m.channel == state.channel_filter)
        .filter(|m| {
            state.status_filter.is_empty()
                || m.route_status.as_deref() == Some(state.status_filter.as_str())
        })
        .collect();

    if filtered.is_empty() {
        ui.label(RichText::new("No messages match the filter.").color(Color32::from_gray(120)));
        return;
    }

    ui.label(
        RichText::new(format!("{} shown", filtered.len()))
            .size(11.5)
            .color(Color32::from_gray(110)),
    );
    ui.add_space(4.0);

    ScrollArea::vertical()
        .id_salt("queue-scroll")
        .show(ui, |ui| {
            for msg in &filtered {
                let is_selected = state.selected_key.as_deref() == Some(&msg.message_key);
                let route = msg.route_status.as_deref().unwrap_or("unknown");
                let sc = status_color(route);
                let fill = if is_selected {
                    Color32::from_rgb(28, 34, 44)
                } else {
                    Color32::from_rgb(22, 25, 30)
                };
                let border = if is_selected { sc } else { Color32::from_rgb(38, 42, 50) };

                let resp = Frame::default()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, border))
                    .corner_radius(8.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Channel icon
                            Frame::default()
                                .fill(channel_color(&msg.channel))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::symmetric(6, 2))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(channel_icon(&msg.channel))
                                            .size(11.0)
                                            .strong()
                                            .color(Color32::BLACK),
                                    );
                                });

                            // Status badge
                            Frame::default()
                                .fill(Color32::from_rgb(36, 40, 48))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::symmetric(6, 2))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(route).size(11.0).color(sc));
                                });

                            // Subject
                            let subject = if msg.subject.is_empty() { "(no subject)" } else { &msg.subject };
                            ui.label(
                                RichText::new(truncate(subject, 60))
                                    .size(12.5)
                                    .color(Color32::from_gray(210)),
                            );
                        });

                        // Expanded detail
                        if is_selected {
                            ui.add_space(6.0);
                            if let Some(sender) = &msg.sender_display {
                                ui.label(
                                    RichText::new(format!("From: {}", sender))
                                        .size(11.5)
                                        .color(Color32::from_gray(150)),
                                );
                            }
                            ui.label(
                                RichText::new(format!("Direction: {}  Observed: {}", msg.direction, msg.observed_at))
                                    .size(11.0)
                                    .color(Color32::from_gray(110)),
                            );
                            if !msg.body_text.is_empty() {
                                ui.add_space(4.0);
                                Frame::default()
                                    .fill(Color32::from_rgb(16, 18, 22))
                                    .corner_radius(6.0)
                                    .inner_margin(8.0)
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(truncate(&msg.body_text, 400))
                                                .size(11.5)
                                                .color(Color32::from_gray(170)),
                                        );
                                    });
                            }
                        }
                    });

                if resp.response.interact(egui::Sense::click()).clicked() {
                    if state.selected_key.as_deref() == Some(&msg.message_key) {
                        state.selected_key = None;
                    } else {
                        state.selected_key = Some(msg.message_key.clone());
                    }
                }
                ui.add_space(3.0);
            }
        });
}

fn truncate(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or(s);
    if line.chars().count() <= max {
        line.to_owned()
    } else {
        line.chars().take(max).collect::<String>() + "..."
    }
}
