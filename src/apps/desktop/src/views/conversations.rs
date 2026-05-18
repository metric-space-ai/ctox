//! Conversations view: all LCM conversations with their messages,
//! mission status and continuity documents in one place.

use std::collections::BTreeMap;

use crate::db_reader::{ContinuityCommitRow, ContinuityDocRow, LcmMessageRow, MissionStateRow};
use eframe::egui::{self, Color32, Frame, RichText, ScrollArea, Stroke, Ui};

#[derive(Default)]
pub struct ConversationsState {
    pub selected_conversation: Option<i64>,
    pub loaded_commits: Vec<ContinuityCommitRow>,
    pub selected_doc_id: Option<String>,
}

const COL_USER: Color32 = Color32::from_rgb(120, 210, 170);
const COL_ASSISTANT: Color32 = Color32::from_rgb(210, 200, 120);
const COL_BLOCKED: Color32 = Color32::from_rgb(218, 106, 106);
const COL_ACTIVE: Color32 = Color32::from_rgb(86, 182, 214);

pub fn render(
    ui: &mut Ui,
    messages: &[LcmMessageRow],
    missions: &[MissionStateRow],
    continuity_docs: &[ContinuityDocRow],
    state: &mut ConversationsState,
    root: Option<&std::path::Path>,
) {
    // Group messages by conversation
    let mut convs: BTreeMap<i64, Vec<&LcmMessageRow>> = BTreeMap::new();
    for msg in messages {
        convs.entry(msg.conversation_id).or_default().push(msg);
    }

    // Mission lookup
    let mission_map: BTreeMap<i64, &MissionStateRow> =
        missions.iter().map(|m| (m.conversation_id, m)).collect();

    // Continuity docs grouped by conversation
    let mut cont_map: BTreeMap<i64, Vec<&ContinuityDocRow>> = BTreeMap::new();
    for doc in continuity_docs {
        cont_map.entry(doc.conversation_id).or_default().push(doc);
    }

    let conv_count = convs.len();

    ui.heading(
        RichText::new("Conversations")
            .size(18.0)
            .color(Color32::from_gray(220)),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "{} conversations, {} messages total",
            conv_count,
            messages.len()
        ))
        .size(12.5)
        .color(Color32::from_gray(130)),
    );
    ui.add_space(8.0);

    if convs.is_empty() {
        ui.add_space(20.0);
        ui.label(
            RichText::new("No conversations yet. Send a message via the TUI to start.")
                .size(13.0)
                .color(Color32::from_gray(120)),
        );
        return;
    }

    ScrollArea::vertical()
        .id_salt("conversations-scroll")
        .show(ui, |ui| {
            for (conv_id, conv_msgs) in convs.iter().rev() {
                let is_selected = state.selected_conversation == Some(*conv_id);
                let mission = mission_map.get(conv_id);
                let docs = cont_map.get(conv_id);

                // Derive a title from the first user message
                let title = conv_msgs
                    .iter()
                    .find(|m| m.role == "user")
                    .map(|m| truncate_first_line(&m.content, 80))
                    .unwrap_or_else(|| format!("Conversation {}", conv_id));

                let total_tokens: i64 = conv_msgs.iter().map(|m| m.token_count).sum();
                let msg_count = conv_msgs.len();

                // Status from mission or last assistant message
                let status_text = if let Some(m) = mission {
                    if !m.mission.is_empty() {
                        m.mission_status.clone()
                    } else if !m.blocker.is_empty() && m.blocker != "null" && m.blocker != "\"\"" {
                        "blocked".to_owned()
                    } else {
                        m.mission_status.clone()
                    }
                } else {
                    String::new()
                };

                // Check if blocked from last assistant message
                let is_blocked = conv_msgs
                    .iter()
                    .filter(|m| m.role == "assistant")
                    .last()
                    .map(|m| m.content.contains("blocked"))
                    .unwrap_or(false);

                let border_color = if is_blocked {
                    COL_BLOCKED
                } else if is_selected {
                    COL_ACTIVE
                } else {
                    Color32::from_rgb(40, 44, 52)
                };

                let fill = if is_selected {
                    Color32::from_rgb(26, 30, 38)
                } else {
                    Color32::from_rgb(20, 23, 28)
                };

                let resp = Frame::default()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, border_color))
                    .corner_radius(10.0)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        // Header row
                        ui.horizontal(|ui| {
                            if is_blocked {
                                ui.label(RichText::new("BLOCKED").size(11.0).color(COL_BLOCKED));
                            } else if !status_text.is_empty() {
                                ui.label(RichText::new(&status_text).size(11.0).color(COL_ACTIVE));
                            }
                            ui.label(
                                RichText::new(&title)
                                    .size(13.5)
                                    .color(Color32::from_gray(220)),
                            );
                        });

                        // Meta line
                        ui.horizontal(|ui| {
                            badge(ui, &format!("{} msgs", msg_count), Color32::from_gray(120));
                            badge(ui, &format_tokens(total_tokens), Color32::from_gray(110));
                            if let Some(docs) = docs {
                                let non_empty = docs.len();
                                if non_empty > 0 {
                                    badge(
                                        ui,
                                        &format!("{} docs", non_empty),
                                        Color32::from_rgb(180, 120, 200),
                                    );
                                }
                            }
                        });

                        // Expanded: show messages
                        if is_selected {
                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(4.0);

                            for msg in conv_msgs {
                                let role_color = if msg.role == "user" { COL_USER } else { COL_ASSISTANT };
                                let role_label = if msg.role == "user" { "You" } else { "CTOX" };

                                ui.horizontal_top(|ui| {
                                    ui.label(
                                        RichText::new(role_label)
                                            .size(11.5)
                                            .strong()
                                            .color(role_color),
                                    );
                                    ui.add_space(8.0);
                                    ui.label(
                                        RichText::new(truncate_content(&msg.content, 500))
                                            .size(12.0)
                                            .color(Color32::from_gray(190)),
                                    );
                                });
                                ui.add_space(4.0);
                            }

                            // Continuity docs if any have content
                            if let Some(docs) = docs {
                                let doc_kinds: Vec<&str> = docs.iter().map(|d| d.kind.as_str()).collect();
                                if !doc_kinds.is_empty() {
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new("Continuity:")
                                                .size(11.0)
                                                .color(Color32::from_gray(110)),
                                        );
                                        for kind in &doc_kinds {
                                            if ui
                                                .add(
                                                    egui::Label::new(
                                                        RichText::new(*kind)
                                                            .size(11.0)
                                                            .color(Color32::from_rgb(180, 120, 200)),
                                                    )
                                                    .sense(egui::Sense::click()),
                                                )
                                                .clicked()
                                            {
                                                // Load commits for this doc
                                                if let Some(doc) = docs.iter().find(|d| d.kind == *kind) {
                                                    if let Some(root) = root {
                                                        state.loaded_commits = crate::db_reader::query_continuity_commits(root, &doc.document_id);
                                                        state.selected_doc_id = Some(doc.document_id.clone());
                                                    }
                                                }
                                            }
                                        }
                                    });

                                    // Show loaded commit content
                                    if let Some(doc_id) = &state.selected_doc_id {
                                        if docs.iter().any(|d| d.document_id == *doc_id) {
                                            if let Some(commit) = state.loaded_commits.first() {
                                                let text = &commit.rendered_text;
                                                if text.len() > 50 {
                                                    ui.add_space(4.0);
                                                    Frame::default()
                                                        .fill(Color32::from_rgb(16, 18, 22))
                                                        .corner_radius(6.0)
                                                        .inner_margin(8.0)
                                                        .show(ui, |ui| {
                                                            ui.label(
                                                                RichText::new(truncate_content(text, 600))
                                                                    .size(11.5)
                                                                    .color(Color32::from_gray(170)),
                                                            );
                                                        });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    });

                if resp.response.interact(egui::Sense::click()).clicked() {
                    if state.selected_conversation == Some(*conv_id) {
                        state.selected_conversation = None;
                        state.selected_doc_id = None;
                        state.loaded_commits.clear();
                    } else {
                        state.selected_conversation = Some(*conv_id);
                        state.selected_doc_id = None;
                        state.loaded_commits.clear();
                    }
                }
                ui.add_space(4.0);
            }
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

fn truncate_first_line(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or(s);
    if line.chars().count() <= max {
        line.to_owned()
    } else {
        line.chars().take(max).collect::<String>() + "..."
    }
}

fn truncate_content(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        s.chars().take(max).collect::<String>() + "..."
    }
}

fn format_tokens(tokens: i64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M tok", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1000 {
        format!("{:.1}K tok", tokens as f64 / 1000.0)
    } else {
        format!("{} tok", tokens)
    }
}
