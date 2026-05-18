//! LCM Memory Tree Explorer with content viewer.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::db_reader::{LcmMessageRow, LcmSummaryRow, SummaryEdge, SummaryMessageLink};
use eframe::egui::{self, Color32, Frame, RichText, ScrollArea, Stroke, Ui};

#[derive(Default)]
pub struct LcmTreeState {
    pub expanded: BTreeSet<String>,
    pub selected_node: Option<SelectedNode>,
}

#[derive(Debug, Clone)]
pub enum SelectedNode {
    Summary(String),
    Message(i64),
}

const COL_CONDENSED: Color32 = Color32::from_rgb(180, 120, 200);
const COL_LEAF: Color32 = Color32::from_rgb(86, 182, 214);
const COL_USER: Color32 = Color32::from_rgb(120, 210, 170);
const COL_ASSISTANT: Color32 = Color32::from_rgb(210, 200, 120);

pub fn render(
    ui: &mut Ui,
    messages: &[LcmMessageRow],
    summaries: &[LcmSummaryRow],
    edges: &[SummaryEdge],
    summary_msgs: &[SummaryMessageLink],
    state: &mut LcmTreeState,
) {
    ui.heading(
        RichText::new("Memory Tree")
            .size(18.0)
            .color(Color32::from_gray(220)),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "{} summaries, {} messages",
            summaries.len(),
            messages.len()
        ))
        .size(12.5)
        .color(Color32::from_gray(130)),
    );
    ui.add_space(8.0);

    if summaries.is_empty() && messages.is_empty() {
        ui.label(RichText::new("No LCM data found.").color(Color32::from_gray(120)));
        return;
    }

    // Build tree structure
    let children_of: BTreeMap<String, Vec<String>> = {
        let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for edge in edges {
            map.entry(edge.parent_summary_id.clone())
                .or_default()
                .push(edge.child_summary_id.clone());
        }
        map
    };

    let msgs_of: BTreeMap<String, Vec<i64>> = {
        let mut map: BTreeMap<String, Vec<i64>> = BTreeMap::new();
        for link in summary_msgs {
            map.entry(link.summary_id.clone())
                .or_default()
                .push(link.message_id);
        }
        map
    };

    let msg_map: HashMap<i64, &LcmMessageRow> =
        messages.iter().map(|m| (m.message_id, m)).collect();
    let summary_map: HashMap<&str, &LcmSummaryRow> = summaries
        .iter()
        .map(|s| (s.summary_id.as_str(), s))
        .collect();

    // Find root summaries (those that are not children of any other)
    let all_children: BTreeSet<&str> = edges.iter().map(|e| e.child_summary_id.as_str()).collect();
    let roots: Vec<&LcmSummaryRow> = summaries
        .iter()
        .filter(|s| !all_children.contains(s.summary_id.as_str()))
        .collect();

    // Split: tree on left, viewer on right
    let available = ui.available_width();
    let tree_width = (available * 0.42).max(200.0);

    ui.horizontal_top(|ui| {
        // Left: Tree
        Frame::default()
            .fill(Color32::from_rgb(16, 18, 22))
            .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 50)))
            .corner_radius(10.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(tree_width);
                ScrollArea::vertical()
                    .max_height(ui.available_height().max(400.0))
                    .id_salt("lcm-tree-scroll")
                    .show(ui, |ui| {
                        if roots.is_empty() {
                            // No tree structure, show flat messages
                            for msg in messages {
                                render_message_leaf(ui, msg, state);
                            }
                        } else {
                            for root in &roots {
                                render_summary_node(
                                    ui,
                                    root,
                                    &children_of,
                                    &msgs_of,
                                    &summary_map,
                                    &msg_map,
                                    state,
                                    0,
                                );
                            }
                            // Orphan messages (not linked to any summary)
                            let linked_msgs: BTreeSet<i64> =
                                summary_msgs.iter().map(|l| l.message_id).collect();
                            let orphans: Vec<&LcmMessageRow> = messages
                                .iter()
                                .filter(|m| !linked_msgs.contains(&m.message_id))
                                .collect();
                            if !orphans.is_empty() {
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new("Recent messages")
                                        .size(12.0)
                                        .color(Color32::from_gray(130)),
                                );
                                for msg in orphans {
                                    render_message_leaf(ui, msg, state);
                                }
                            }
                        }
                    });
            });

        // Right: Viewer
        Frame::default()
            .fill(Color32::from_rgb(20, 22, 26))
            .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 50)))
            .corner_radius(10.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width((available - tree_width - 24.0).max(80.0));
                render_viewer(ui, &summary_map, &msg_map, state);
            });
    });
}

fn render_summary_node(
    ui: &mut Ui,
    summary: &LcmSummaryRow,
    children_of: &BTreeMap<String, Vec<String>>,
    msgs_of: &BTreeMap<String, Vec<i64>>,
    summary_map: &HashMap<&str, &LcmSummaryRow>,
    msg_map: &HashMap<i64, &LcmMessageRow>,
    state: &mut LcmTreeState,
    indent: usize,
) {
    let id = &summary.summary_id;
    let has_children = children_of.contains_key(id);
    let has_msgs = msgs_of.contains_key(id);
    let expanded = state.expanded.contains(id);
    let is_selected = matches!(&state.selected_node, Some(SelectedNode::Summary(s)) if s == id);

    let color = if summary.kind == "condensed" {
        COL_CONDENSED
    } else {
        COL_LEAF
    };

    let indent_px = indent as f32 * 16.0;
    ui.horizontal(|ui| {
        ui.add_space(indent_px);
        let arrow = if has_children || has_msgs {
            if expanded {
                "▼"
            } else {
                "▶"
            }
        } else {
            "  "
        };
        if ui
            .add(
                egui::Label::new(
                    RichText::new(arrow)
                        .size(11.0)
                        .color(Color32::from_gray(140)),
                )
                .sense(egui::Sense::click()),
            )
            .clicked()
        {
            if expanded {
                state.expanded.remove(id);
            } else {
                state.expanded.insert(id.clone());
            }
        }

        let label = format!(
            "{} [d={}] ({} msgs, {}K tok)",
            capitalize_kind(&summary.kind),
            summary.depth,
            summary.descendant_count,
            summary.token_count / 1000,
        );
        let label_color = if is_selected { Color32::WHITE } else { color };
        if ui
            .add(
                egui::Label::new(RichText::new(label).size(12.0).color(label_color))
                    .sense(egui::Sense::click()),
            )
            .clicked()
        {
            state.selected_node = Some(SelectedNode::Summary(id.clone()));
        }
    });

    if expanded {
        // Child summaries
        if let Some(child_ids) = children_of.get(id) {
            for child_id in child_ids {
                if let Some(child) = summary_map.get(child_id.as_str()) {
                    render_summary_node(
                        ui,
                        child,
                        children_of,
                        msgs_of,
                        summary_map,
                        msg_map,
                        state,
                        indent + 1,
                    );
                }
            }
        }
        // Messages under this summary
        if let Some(msg_ids) = msgs_of.get(id) {
            let mut sorted_ids = msg_ids.clone();
            sorted_ids.sort();
            for msg_id in sorted_ids {
                if let Some(msg) = msg_map.get(&msg_id) {
                    let indent_px = (indent + 1) as f32 * 16.0;
                    let is_selected = matches!(&state.selected_node, Some(SelectedNode::Message(m)) if *m == msg.message_id);
                    let role_color = if msg.role == "user" {
                        COL_USER
                    } else {
                        COL_ASSISTANT
                    };
                    let label_color = if is_selected {
                        Color32::WHITE
                    } else {
                        role_color
                    };
                    ui.horizontal(|ui| {
                        ui.add_space(indent_px);
                        ui.add_space(14.0); // align with arrow
                        let preview = truncate_content(&msg.content, 40);
                        let label = format!("[{}] {}", msg.role, preview);
                        if ui
                            .add(
                                egui::Label::new(
                                    RichText::new(label).size(11.5).color(label_color),
                                )
                                .sense(egui::Sense::click()),
                            )
                            .clicked()
                        {
                            state.selected_node = Some(SelectedNode::Message(msg.message_id));
                        }
                    });
                }
            }
        }
    }
}

fn render_message_leaf(ui: &mut Ui, msg: &LcmMessageRow, state: &mut LcmTreeState) {
    let is_selected =
        matches!(&state.selected_node, Some(SelectedNode::Message(m)) if *m == msg.message_id);
    let role_color = if msg.role == "user" {
        COL_USER
    } else {
        COL_ASSISTANT
    };
    let label_color = if is_selected {
        Color32::WHITE
    } else {
        role_color
    };
    let preview = truncate_content(&msg.content, 50);
    let label = format!("[{}] seq={} {}", msg.role, msg.seq, preview);
    if ui
        .add(
            egui::Label::new(RichText::new(label).size(11.5).color(label_color))
                .sense(egui::Sense::click()),
        )
        .clicked()
    {
        state.selected_node = Some(SelectedNode::Message(msg.message_id));
    }
}

fn render_viewer(
    ui: &mut Ui,
    summary_map: &HashMap<&str, &LcmSummaryRow>,
    msg_map: &HashMap<i64, &LcmMessageRow>,
    state: &LcmTreeState,
) {
    match &state.selected_node {
        Some(SelectedNode::Summary(id)) => {
            if let Some(summary) = summary_map.get(id.as_str()) {
                let color = if summary.kind == "condensed" {
                    COL_CONDENSED
                } else {
                    COL_LEAF
                };
                ui.label(
                    RichText::new(format!("{} Summary", capitalize_kind(&summary.kind)))
                        .size(15.0)
                        .strong()
                        .color(color),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    badge(
                        ui,
                        &format!("depth {}", summary.depth),
                        Color32::from_gray(100),
                    );
                    badge(
                        ui,
                        &format!("{} msgs", summary.descendant_count),
                        Color32::from_gray(100),
                    );
                    badge(
                        ui,
                        &format!("{}K tok", summary.token_count / 1000),
                        Color32::from_gray(100),
                    );
                    badge(
                        ui,
                        &format!("{}K desc tok", summary.descendant_token_count / 1000),
                        Color32::from_gray(100),
                    );
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(&summary.created_at)
                        .size(11.0)
                        .color(Color32::from_gray(110)),
                );
                ui.add_space(8.0);
                ScrollArea::vertical()
                    .max_height(ui.available_height().max(300.0))
                    .id_salt("lcm-viewer")
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(&summary.content)
                                .size(12.5)
                                .color(Color32::from_gray(200)),
                        );
                    });
            } else {
                ui.label(RichText::new("Summary not found.").color(Color32::from_gray(120)));
            }
        }
        Some(SelectedNode::Message(id)) => {
            if let Some(msg) = msg_map.get(id) {
                let role_color = if msg.role == "user" {
                    COL_USER
                } else {
                    COL_ASSISTANT
                };
                ui.label(
                    RichText::new(format!("Message #{} ({})", msg.seq, msg.role))
                        .size(15.0)
                        .strong()
                        .color(role_color),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    badge(
                        ui,
                        &format!("{} tok", msg.token_count),
                        Color32::from_gray(100),
                    );
                });
                ui.label(
                    RichText::new(&msg.created_at)
                        .size(11.0)
                        .color(Color32::from_gray(110)),
                );
                ui.add_space(8.0);
                ScrollArea::vertical()
                    .max_height(ui.available_height().max(300.0))
                    .id_salt("lcm-viewer")
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(&msg.content)
                                .size(12.5)
                                .color(Color32::from_gray(200)),
                        );
                    });
            } else {
                ui.label(RichText::new("Message not found.").color(Color32::from_gray(120)));
            }
        }
        None => {
            ui.add_space(40.0);
            ui.label(
                RichText::new("Select a node in the tree to view its content.")
                    .color(Color32::from_gray(120)),
            );
        }
    }
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

fn capitalize_kind(kind: &str) -> &str {
    match kind {
        "condensed" => "Condensed",
        "leaf" => "Leaf",
        _ => kind,
    }
}

fn truncate_content(s: &str, max: usize) -> String {
    let first_line = s.lines().next().unwrap_or("");
    if first_line.len() <= max {
        first_line.to_owned()
    } else {
        first_line.chars().take(max).collect::<String>() + "..."
    }
}
