//! Dynamic tools inventory grouped by thread.

use std::collections::BTreeMap;

use crate::db_reader::DynamicToolRow;
use eframe::egui::{self, Color32, Frame, RichText, ScrollArea, Stroke, Ui};

#[derive(Default)]
pub struct ToolsState {
    pub search: String,
    pub expanded_threads: std::collections::BTreeSet<String>,
    pub selected_tool: Option<(String, String)>, // (thread_id, tool_name)
}

pub fn render(ui: &mut Ui, tools: &[DynamicToolRow], state: &mut ToolsState) {
    ui.heading(
        RichText::new("Dynamic Tools")
            .size(18.0)
            .color(Color32::from_gray(220)),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("{} tools", tools.len()))
            .size(12.5)
            .color(Color32::from_gray(130)),
    );
    ui.add_space(8.0);

    // Search
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Search:")
                .size(12.0)
                .color(Color32::from_gray(140)),
        );
        ui.add(
            egui::TextEdit::singleline(&mut state.search)
                .desired_width(200.0)
                .hint_text("filter by tool name"),
        );
    });
    ui.add_space(8.0);

    if tools.is_empty() {
        ui.label(RichText::new("No dynamic tools found.").color(Color32::from_gray(120)));
        return;
    }

    let search_lower = state.search.to_lowercase();
    let filtered: Vec<&DynamicToolRow> = if search_lower.is_empty() {
        tools.iter().collect()
    } else {
        tools
            .iter()
            .filter(|t| t.name.to_lowercase().contains(&search_lower))
            .collect()
    };

    // Group by thread
    let mut grouped: BTreeMap<String, Vec<&DynamicToolRow>> = BTreeMap::new();
    for tool in &filtered {
        let key = tool.thread_id.clone();
        grouped.entry(key).or_default().push(tool);
    }

    let available = ui.available_width();
    let list_width = (available * 0.5).max(250.0);

    ui.horizontal_top(|ui| {
        // Left: grouped list
        Frame::default()
            .fill(Color32::from_rgb(16, 18, 22))
            .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 50)))
            .corner_radius(10.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(list_width);
                ScrollArea::vertical()
                    .max_height(ui.available_height().max(400.0))
                    .id_salt("tools-list")
                    .show(ui, |ui| {
                        for (thread_id, thread_tools) in &grouped {
                            let thread_title = thread_tools
                                .first()
                                .and_then(|t| t.thread_title.as_deref())
                                .unwrap_or(thread_id);
                            let expanded = state.expanded_threads.contains(thread_id);
                            let arrow = if expanded { "▼" } else { "▶" };

                            ui.horizontal(|ui| {
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
                                        state.expanded_threads.remove(thread_id);
                                    } else {
                                        state.expanded_threads.insert(thread_id.clone());
                                    }
                                }
                                ui.label(
                                    RichText::new(format!(
                                        "{} ({} tools)",
                                        truncate(thread_title, 30),
                                        thread_tools.len()
                                    ))
                                    .size(12.5)
                                    .color(Color32::from_gray(200)),
                                );
                            });

                            if expanded {
                                for tool in thread_tools {
                                    let is_selected = state.selected_tool.as_ref()
                                        == Some(&(tool.thread_id.clone(), tool.name.clone()));
                                    let fill = if is_selected {
                                        Color32::from_rgb(34, 40, 50)
                                    } else {
                                        Color32::TRANSPARENT
                                    };

                                    let resp = Frame::default()
                                        .fill(fill)
                                        .corner_radius(4.0)
                                        .inner_margin(egui::Margin::symmetric(4, 2))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.add_space(20.0);
                                                ui.label(
                                                    RichText::new(format!("#{}", tool.position))
                                                        .size(11.0)
                                                        .color(Color32::from_gray(100)),
                                                );
                                                ui.label(
                                                    RichText::new(&tool.name)
                                                        .size(12.0)
                                                        .color(Color32::from_rgb(86, 182, 214)),
                                                );
                                            });
                                            ui.horizontal(|ui| {
                                                ui.add_space(40.0);
                                                ui.label(
                                                    RichText::new(truncate(&tool.description, 50))
                                                        .size(11.0)
                                                        .color(Color32::from_gray(140)),
                                                );
                                            });
                                        });
                                    if resp.response.interact(egui::Sense::click()).clicked() {
                                        state.selected_tool =
                                            Some((tool.thread_id.clone(), tool.name.clone()));
                                    }
                                }
                            }
                            ui.add_space(4.0);
                        }
                    });
            });

        // Right: Detail
        Frame::default()
            .fill(Color32::from_rgb(20, 22, 26))
            .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 50)))
            .corner_radius(10.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width((available - list_width - 24.0).max(80.0));
                render_tool_detail(ui, tools, state);
            });
    });
}

fn render_tool_detail(ui: &mut Ui, tools: &[DynamicToolRow], state: &ToolsState) {
    let Some((thread_id, name)) = &state.selected_tool else {
        ui.add_space(40.0);
        ui.label(RichText::new("Select a tool to see details.").color(Color32::from_gray(120)));
        return;
    };
    let Some(tool) = tools
        .iter()
        .find(|t| t.thread_id == *thread_id && t.name == *name)
    else {
        ui.label(RichText::new("Tool not found.").color(Color32::from_gray(120)));
        return;
    };

    ui.label(
        RichText::new(&tool.name)
            .size(16.0)
            .strong()
            .color(Color32::from_rgb(86, 182, 214)),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(&tool.description)
            .size(12.5)
            .color(Color32::from_gray(190)),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Position:")
                .size(12.0)
                .color(Color32::from_gray(130)),
        );
        ui.label(
            RichText::new(tool.position.to_string())
                .size(12.0)
                .color(Color32::from_gray(200)),
        );
    });

    ui.add_space(8.0);
    ui.label(
        RichText::new("Input Schema:")
            .size(13.0)
            .strong()
            .color(Color32::from_gray(180)),
    );
    ui.add_space(4.0);
    ScrollArea::vertical()
        .max_height(ui.available_height().max(200.0))
        .id_salt("tool-schema")
        .show(ui, |ui| {
            // Pretty-print JSON
            let pretty = serde_json::from_str::<serde_json::Value>(&tool.input_schema)
                .ok()
                .and_then(|v| serde_json::to_string_pretty(&v).ok())
                .unwrap_or_else(|| tool.input_schema.clone());
            ui.label(
                RichText::new(pretty)
                    .size(11.5)
                    .family(egui::FontFamily::Monospace)
                    .color(Color32::from_gray(170)),
            );
        });
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else {
        s.chars().take(max).collect::<String>() + "..."
    }
}
