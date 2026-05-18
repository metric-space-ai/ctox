//! Continuity documents and context items viewer.

use crate::db_reader::{ContextItemRow, ContinuityCommitRow, ContinuityDocRow};
use eframe::egui::{self, Color32, Frame, RichText, ScrollArea, Stroke, Ui};

#[derive(Default)]
pub struct ContinuityState {
    pub selected_doc: Option<String>,
    pub commits: Vec<ContinuityCommitRow>,
    pub selected_commit: Option<String>,
    pub root_for_commits: Option<std::path::PathBuf>,
}

pub fn render(
    ui: &mut Ui,
    docs: &[ContinuityDocRow],
    context_items: &[ContextItemRow],
    state: &mut ContinuityState,
    root: Option<&std::path::Path>,
) {
    ui.heading(
        RichText::new("Continuity")
            .size(18.0)
            .color(Color32::from_gray(220)),
    );
    ui.add_space(8.0);

    // Documents
    if !docs.is_empty() {
        ui.label(
            RichText::new("Documents")
                .size(15.0)
                .strong()
                .color(Color32::from_gray(200)),
        );
        ui.add_space(6.0);

        let available = ui.available_width();
        let list_width = (available * 0.4).max(200.0);

        ui.horizontal_top(|ui| {
            // Left: doc list
            Frame::default()
                .fill(Color32::from_rgb(16, 18, 22))
                .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 50)))
                .corner_radius(10.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_width(list_width);
                    ScrollArea::vertical()
                        .max_height(200.0)
                        .id_salt("continuity-docs")
                        .show(ui, |ui| {
                            for doc in docs {
                                let is_selected =
                                    state.selected_doc.as_deref() == Some(&doc.document_id);
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
                                    .corner_radius(8.0)
                                    .inner_margin(8.0)
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(&doc.kind)
                                                .size(13.0)
                                                .strong()
                                                .color(Color32::from_rgb(180, 120, 200)),
                                        );
                                        ui.label(
                                            RichText::new(format!("conv {}", doc.conversation_id))
                                                .size(11.0)
                                                .color(Color32::from_gray(120)),
                                        );
                                        ui.label(
                                            RichText::new(&doc.updated_at)
                                                .size(11.0)
                                                .color(Color32::from_gray(100)),
                                        );
                                    });
                                if resp.response.interact(egui::Sense::click()).clicked() {
                                    state.selected_doc = Some(doc.document_id.clone());
                                    // Load commits for this doc
                                    if let Some(root) = root {
                                        state.commits = crate::db_reader::query_continuity_commits(
                                            root,
                                            &doc.document_id,
                                        );
                                        state.root_for_commits = Some(root.to_path_buf());
                                    }
                                    state.selected_commit = None;
                                }
                                ui.add_space(3.0);
                            }
                        });
                });

            // Right: commits and rendered text
            Frame::default()
                .fill(Color32::from_rgb(20, 22, 26))
                .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 50)))
                .corner_radius(10.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.set_width((available - list_width - 24.0).max(80.0));
                    render_commits(ui, state);
                });
        });
        ui.add_space(12.0);
    }

    // Context Items
    if !context_items.is_empty() {
        ui.label(
            RichText::new(format!("Context Window ({} items)", context_items.len()))
                .size(15.0)
                .strong()
                .color(Color32::from_gray(200)),
        );
        ui.add_space(6.0);

        Frame::default()
            .fill(Color32::from_rgb(16, 18, 22))
            .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 50)))
            .corner_radius(10.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ScrollArea::vertical()
                    .max_height(250.0)
                    .id_salt("context-items")
                    .show(ui, |ui| {
                        for item in context_items {
                            ui.horizontal(|ui| {
                                let type_color = match item.item_type.as_str() {
                                    "message" => Color32::from_rgb(120, 210, 170),
                                    "summary" => Color32::from_rgb(180, 120, 200),
                                    _ => Color32::from_gray(150),
                                };
                                ui.label(
                                    RichText::new(format!("#{}", item.ordinal))
                                        .size(11.0)
                                        .color(Color32::from_gray(100)),
                                );
                                ui.label(
                                    RichText::new(&item.item_type).size(11.5).color(type_color),
                                );
                                if let Some(mid) = item.message_id {
                                    ui.label(
                                        RichText::new(format!("msg:{}", mid))
                                            .size(11.0)
                                            .color(Color32::from_gray(130)),
                                    );
                                }
                                if let Some(sid) = &item.summary_id {
                                    ui.label(
                                        RichText::new(format!("sum:{}", truncate(sid, 12)))
                                            .size(11.0)
                                            .color(Color32::from_gray(130)),
                                    );
                                }
                            });
                        }
                    });
            });
    }

    if docs.is_empty() && context_items.is_empty() {
        ui.label(RichText::new("No continuity data found.").color(Color32::from_gray(120)));
    }
}

fn render_commits(ui: &mut Ui, state: &mut ContinuityState) {
    let Some(_doc_id) = &state.selected_doc else {
        ui.add_space(40.0);
        ui.label(
            RichText::new("Select a document to see its commits.").color(Color32::from_gray(120)),
        );
        return;
    };

    if state.commits.is_empty() {
        ui.label(RichText::new("No commits for this document.").color(Color32::from_gray(120)));
        return;
    }

    // Commit list
    ui.label(
        RichText::new(format!("{} commits", state.commits.len()))
            .size(12.5)
            .color(Color32::from_gray(130)),
    );
    ui.add_space(4.0);

    for commit in &state.commits {
        let is_selected = state.selected_commit.as_deref() == Some(&commit.commit_id);
        let fill = if is_selected {
            Color32::from_rgb(30, 36, 46)
        } else {
            Color32::TRANSPARENT
        };
        let resp = Frame::default()
            .fill(fill)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(truncate(&commit.commit_id, 10))
                            .size(11.0)
                            .family(egui::FontFamily::Monospace)
                            .color(Color32::from_rgb(86, 182, 214)),
                    );
                    ui.label(
                        RichText::new(&commit.created_at)
                            .size(11.0)
                            .color(Color32::from_gray(110)),
                    );
                    if commit.parent_commit_id.is_some() {
                        ui.label(
                            RichText::new("(has parent)")
                                .size(10.0)
                                .color(Color32::from_gray(90)),
                        );
                    }
                });
            });
        if resp.response.interact(egui::Sense::click()).clicked() {
            state.selected_commit = Some(commit.commit_id.clone());
        }
    }

    // Show rendered text of selected commit
    if let Some(cid) = &state.selected_commit {
        if let Some(commit) = state.commits.iter().find(|c| c.commit_id == *cid) {
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            ScrollArea::vertical()
                .max_height(ui.available_height().max(200.0))
                .id_salt("commit-content")
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(&commit.rendered_text)
                            .size(12.0)
                            .color(Color32::from_gray(190)),
                    );
                });
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else {
        s.chars().take(max).collect::<String>() + "..."
    }
}
