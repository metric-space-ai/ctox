//! Threads table view with filtering and detail panel.

use crate::db_reader::ThreadRow;
use eframe::egui::{self, Color32, Frame, RichText, ScrollArea, Stroke, Ui};

#[derive(Default)]
pub struct ThreadsState {
    pub selected_thread: Option<String>,
    pub show_archived: bool,
    pub provider_filter: String,
}

pub fn render(ui: &mut Ui, threads: &[ThreadRow], state: &mut ThreadsState) {
    ui.heading(
        RichText::new("Threads")
            .size(18.0)
            .color(Color32::from_gray(220)),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("{} threads", threads.len()))
            .size(12.5)
            .color(Color32::from_gray(130)),
    );
    ui.add_space(8.0);

    // Filters
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.show_archived, "Show archived");
        ui.add_space(16.0);
        ui.label(
            RichText::new("Provider:")
                .size(12.0)
                .color(Color32::from_gray(140)),
        );
        egui::ComboBox::from_id_salt("thread-provider-filter")
            .selected_text(if state.provider_filter.is_empty() {
                "All"
            } else {
                &state.provider_filter
            })
            .width(120.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.provider_filter.is_empty(), "All")
                    .clicked()
                {
                    state.provider_filter.clear();
                }
                for provider in &["local", "openai", "anthropic", "openrouter"] {
                    if ui
                        .selectable_label(state.provider_filter == *provider, *provider)
                        .clicked()
                    {
                        state.provider_filter = provider.to_string();
                    }
                }
            });
    });
    ui.add_space(8.0);

    let filtered: Vec<&ThreadRow> = threads
        .iter()
        .filter(|t| state.show_archived || !t.archived)
        .filter(|t| state.provider_filter.is_empty() || t.model_provider == state.provider_filter)
        .collect();

    if filtered.is_empty() {
        ui.label(
            RichText::new("No threads match the current filter.").color(Color32::from_gray(120)),
        );
        return;
    }

    // Thread list (vertical, each row clickable)
    ScrollArea::vertical()
        .max_height(ui.available_height().max(300.0))
        .id_salt("threads-list")
        .show(ui, |ui| {
            for thread in &filtered {
                let is_selected = state.selected_thread.as_deref() == Some(&thread.id);
                let fill = if is_selected {
                    Color32::from_rgb(30, 38, 50)
                } else {
                    Color32::from_rgb(22, 25, 30)
                };
                let border = if is_selected {
                    Color32::from_rgb(86, 182, 214)
                } else {
                    Color32::from_rgb(38, 42, 48)
                };
                let resp = Frame::default()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, border))
                    .corner_radius(8.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Status dot
                            let dot_color = if thread.archived {
                                Color32::from_gray(80)
                            } else {
                                Color32::from_rgb(94, 184, 116)
                            };
                            ui.label(RichText::new("●").size(10.0).color(dot_color));
                            ui.add_space(4.0);

                            // Title
                            ui.label(
                                RichText::new(truncate(&thread.title, 40))
                                    .size(13.0)
                                    .color(Color32::from_gray(220)),
                            );

                            ui.add_space(8.0);

                            // Provider badge
                            badge(ui, &thread.model_provider, Color32::from_gray(130));

                            // Tokens
                            badge(
                                ui,
                                &format_tokens(thread.tokens_used),
                                Color32::from_gray(110),
                            );

                            // Branch
                            if let Some(branch) = &thread.git_branch {
                                badge(ui, &truncate(branch, 16), Color32::from_rgb(123, 164, 126));
                            }
                        });

                        // Second line: details when selected
                        if is_selected {
                            ui.add_space(6.0);
                            render_thread_detail_inline(ui, thread);
                        }
                    });
                if resp.response.interact(egui::Sense::click()).clicked() {
                    if state.selected_thread.as_deref() == Some(&thread.id) {
                        state.selected_thread = None; // toggle off
                    } else {
                        state.selected_thread = Some(thread.id.clone());
                    }
                }
                ui.add_space(3.0);
            }
        });
}

fn render_thread_detail_inline(ui: &mut Ui, thread: &ThreadRow) {
    let detail_color = Color32::from_gray(160);
    let label_color = Color32::from_gray(110);

    egui::Grid::new(format!("thread-detail-{}", thread.id))
        .num_columns(2)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            row(ui, "ID", &thread.id, label_color, detail_color);
            row(ui, "Source", &thread.source, label_color, detail_color);
            row(
                ui,
                "Model",
                thread.model.as_deref().unwrap_or("-"),
                label_color,
                detail_color,
            );
            row(
                ui,
                "Created",
                &format_ts(thread.created_at),
                label_color,
                detail_color,
            );
            row(
                ui,
                "Updated",
                &format_ts(thread.updated_at),
                label_color,
                detail_color,
            );
            if let Some(nick) = &thread.agent_nickname {
                row(ui, "Agent", nick, label_color, detail_color);
            }
            if let Some(role) = &thread.agent_role {
                row(ui, "Role", role, label_color, detail_color);
            }
            if let Some(cli) = &thread.cli_version {
                row(ui, "CLI", cli, label_color, detail_color);
            }
            if let Some(effort) = &thread.reasoning_effort {
                row(ui, "Reasoning", effort, label_color, detail_color);
            }
        });

    if let Some(msg) = &thread.first_user_message {
        ui.add_space(4.0);
        ui.label(
            RichText::new("First message:")
                .size(11.0)
                .color(Color32::from_gray(110)),
        );
        ui.label(
            RichText::new(truncate(msg, 200))
                .size(11.5)
                .color(Color32::from_gray(170)),
        );
    }
}

fn row(ui: &mut Ui, label: &str, value: &str, label_color: Color32, value_color: Color32) {
    ui.label(RichText::new(label).size(11.5).color(label_color));
    ui.label(RichText::new(value).size(11.5).color(value_color));
    ui.end_row();
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
    let s = s.lines().next().unwrap_or(s);
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        s.chars().take(max).collect::<String>() + "..."
    }
}

fn format_tokens(tokens: i64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1000 {
        format!("{:.1}K", tokens as f64 / 1000.0)
    } else {
        tokens.to_string()
    }
}

fn format_ts(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| ts.to_string())
}
