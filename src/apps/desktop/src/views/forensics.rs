//! Forensics view: surfaces the process-mining tables that already exist in
//! `runtime/ctox.sqlite3` — activity spectrum, directly-follows graph, spawn
//! edges, case inspector, and state violations.

use eframe::egui::{self, Color32, Frame, RichText, ScrollArea, Stroke, Ui};

use crate::db_reader::{
    PmActivityRow, PmCaseEventRow, PmCaseRow, PmDfgEdge, PmViolationRow, SpawnEdgeRow,
};

const CARD_BG: Color32 = Color32::from_rgb(22, 25, 30);
const CARD_BORDER: Color32 = Color32::from_rgb(38, 42, 50);
const HEADING_GREY: Color32 = Color32::from_gray(220);
const LABEL_GREY: Color32 = Color32::from_gray(140);
const VALUE_GREY: Color32 = Color32::from_gray(210);
const MUTED_GREY: Color32 = Color32::from_gray(110);
const ACCENT_GREEN: Color32 = Color32::from_rgb(110, 188, 120);
const ACCENT_RED: Color32 = Color32::from_rgb(218, 106, 106);
const ACCENT_YELLOW: Color32 = Color32::from_rgb(220, 190, 90);
const ACCENT_BLUE: Color32 = Color32::from_rgb(86, 182, 214);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ForensicsTab {
    #[default]
    Activities,
    Dfg,
    Cases,
    Spawns,
    Violations,
}

impl ForensicsTab {
    const ALL: [Self; 5] = [
        Self::Activities,
        Self::Dfg,
        Self::Cases,
        Self::Spawns,
        Self::Violations,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Activities => "Activities",
            Self::Dfg => "DFG",
            Self::Cases => "Cases",
            Self::Spawns => "Spawn tree",
            Self::Violations => "Violations",
        }
    }
}

#[derive(Default)]
pub struct ForensicsState {
    pub tab: ForensicsTab,
    pub selected_case: Option<String>,
    pub spawn_filter: SpawnFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpawnFilter {
    #[default]
    All,
    AcceptedOnly,
    RejectedOnly,
}

pub struct ForensicsData<'a> {
    pub activities: &'a [PmActivityRow],
    pub dfg: &'a [PmDfgEdge],
    pub cases: &'a [PmCaseRow],
    pub spawn_edges: &'a [SpawnEdgeRow],
    pub violations: &'a [PmViolationRow],
    /// Events for the currently selected case (or empty if none selected).
    pub case_events: &'a [PmCaseEventRow],
}

pub fn render(ui: &mut Ui, state: &mut ForensicsState, data: ForensicsData<'_>) {
    ui.heading(
        RichText::new("Forensics · process mining")
            .size(18.0)
            .color(HEADING_GREY),
    );
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        for t in ForensicsTab::ALL {
            if ui.selectable_label(state.tab == t, t.label()).clicked() {
                state.tab = t;
            }
        }
    });
    ui.add_space(8.0);

    match state.tab {
        ForensicsTab::Activities => render_activities(ui, data.activities),
        ForensicsTab::Dfg => render_dfg(ui, data.dfg),
        ForensicsTab::Cases => render_cases(ui, state, data.cases, data.case_events),
        ForensicsTab::Spawns => render_spawn_edges(ui, state, data.spawn_edges),
        ForensicsTab::Violations => render_violations(ui, data.violations),
    }
}

fn render_activities(ui: &mut Ui, activities: &[PmActivityRow]) {
    if activities.is_empty() {
        empty(ui, "No activities recorded.");
        return;
    }
    let max = activities.iter().map(|a| a.count).max().unwrap_or(1).max(1);
    card(ui, "Activity spectrum", |ui| {
        ScrollArea::vertical()
            .id_salt("forensics-activities")
            .show(ui, |ui| {
                for a in activities {
                    ui.horizontal(|ui| {
                        ui.allocate_ui(egui::vec2(280.0, 0.0), |ui| {
                            ui.label(
                                RichText::new(clip(&a.activity, 56))
                                    .size(12.0)
                                    .color(VALUE_GREY),
                            );
                        });
                        let frac = (a.count as f32 / max as f32).clamp(0.0, 1.0);
                        let bar_width = (ui.available_width() - 200.0).max(40.0);
                        bar(ui, bar_width, frac, ACCENT_BLUE);
                        ui.label(
                            RichText::new(format!(
                                "{}  ·  {}",
                                a.count,
                                short_time(&a.last_seen_at)
                            ))
                            .size(11.5)
                            .color(LABEL_GREY),
                        );
                    });
                    ui.add_space(2.0);
                }
            });
    });
}

fn render_dfg(ui: &mut Ui, dfg: &[PmDfgEdge]) {
    if dfg.is_empty() {
        empty(ui, "No directly-follows edges yet.");
        return;
    }
    let max = dfg.iter().map(|e| e.count).max().unwrap_or(1).max(1);
    card(ui, "Directly-follows graph (top edges)", |ui| {
        ui.label(
            RichText::new("from  →  to  ·  weight = transitions")
                .size(11.0)
                .color(MUTED_GREY),
        );
        ui.add_space(4.0);
        ScrollArea::vertical()
            .id_salt("forensics-dfg")
            .show(ui, |ui| {
                for e in dfg {
                    ui.horizontal(|ui| {
                        ui.allocate_ui(egui::vec2(220.0, 0.0), |ui| {
                            ui.label(
                                RichText::new(clip(&e.from_activity, 42))
                                    .size(11.5)
                                    .color(VALUE_GREY),
                            );
                        });
                        ui.label(RichText::new("→").size(11.5).color(MUTED_GREY));
                        ui.allocate_ui(egui::vec2(220.0, 0.0), |ui| {
                            ui.label(
                                RichText::new(clip(&e.to_activity, 42))
                                    .size(11.5)
                                    .color(VALUE_GREY),
                            );
                        });
                        let frac = (e.count as f32 / max as f32).clamp(0.0, 1.0);
                        let bar_width = (ui.available_width() - 80.0).max(40.0);
                        bar(ui, bar_width, frac, ACCENT_BLUE);
                        ui.label(
                            RichText::new(e.count.to_string())
                                .size(11.5)
                                .color(LABEL_GREY),
                        );
                    });
                    ui.add_space(2.0);
                }
            });
    });
}

fn render_cases(
    ui: &mut Ui,
    state: &mut ForensicsState,
    cases: &[PmCaseRow],
    case_events: &[PmCaseEventRow],
) {
    if cases.is_empty() {
        empty(ui, "No cases recorded.");
        return;
    }

    let avail = ui.available_width();
    let left_width = (avail * 0.42).max(280.0).min(420.0);
    let right_width = (avail - left_width - 10.0).max(280.0);

    ui.horizontal_top(|ui| {
        ui.allocate_ui(egui::vec2(left_width, 0.0), |ui| {
            card(ui, &format!("Cases ({})", cases.len()), |ui| {
                ScrollArea::vertical()
                    .id_salt("forensics-cases")
                    .max_height(ui.available_height())
                    .show(ui, |ui| {
                        for c in cases {
                            let is_selected = state.selected_case.as_deref() == Some(&c.case_id);
                            let border = if is_selected {
                                ACCENT_BLUE
                            } else {
                                CARD_BORDER
                            };
                            let fill = if is_selected {
                                Color32::from_rgb(28, 34, 44)
                            } else {
                                CARD_BG
                            };
                            let resp = Frame::default()
                                .fill(fill)
                                .stroke(Stroke::new(1.0, border))
                                .corner_radius(6.0)
                                .inner_margin(8.0)
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(clip(&c.case_id, 58))
                                            .size(11.5)
                                            .color(VALUE_GREY),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "{} events · {} activities · {}",
                                            c.event_count,
                                            c.activity_count,
                                            short_time(&c.last_seen_at)
                                        ))
                                        .size(10.5)
                                        .color(MUTED_GREY),
                                    );
                                });
                            if resp.response.interact(egui::Sense::click()).clicked() {
                                state.selected_case = Some(c.case_id.clone());
                            }
                            ui.add_space(2.0);
                        }
                    });
            });
        });
        ui.add_space(10.0);
        ui.allocate_ui(egui::vec2(right_width, 0.0), |ui| {
            let title = match &state.selected_case {
                Some(id) => format!("Case · {}", clip(id, 48)),
                None => "Case inspector".to_string(),
            };
            card(ui, &title, |ui| {
                if state.selected_case.is_none() {
                    ui.label(
                        RichText::new("Pick a case on the left to see its event timeline.")
                            .size(12.0)
                            .color(MUTED_GREY),
                    );
                    return;
                }
                if case_events.is_empty() {
                    ui.label(
                        RichText::new("No events for this case.")
                            .size(12.0)
                            .color(MUTED_GREY),
                    );
                    return;
                }
                ScrollArea::vertical()
                    .id_salt("forensics-case-events")
                    .max_height(ui.available_height())
                    .show(ui, |ui| {
                        for ev in case_events {
                            Frame::default()
                                .fill(Color32::from_rgb(18, 21, 26))
                                .corner_radius(5.0)
                                .inner_margin(7.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(format!("#{}", ev.event_seq))
                                                .size(11.0)
                                                .color(MUTED_GREY),
                                        );
                                        ui.label(
                                            RichText::new(short_time(&ev.timestamp))
                                                .size(11.0)
                                                .color(MUTED_GREY),
                                        );
                                        badge(
                                            ui,
                                            &ev.lifecycle_transition,
                                            lifecycle_color(&ev.lifecycle_transition),
                                        );
                                        ui.label(
                                            RichText::new(clip(&ev.activity, 48))
                                                .size(12.0)
                                                .color(VALUE_GREY),
                                        );
                                    });
                                    let from = ev.from_state.as_deref().unwrap_or("·");
                                    let to = ev.to_state.as_deref().unwrap_or("·");
                                    ui.label(
                                        RichText::new(format!(
                                            "{}  →  {}     ·  {}.{}",
                                            from, to, ev.table_name, ev.operation
                                        ))
                                        .size(10.5)
                                        .color(LABEL_GREY),
                                    );
                                    if let Some(cmd) = &ev.command_name {
                                        if !cmd.is_empty() {
                                            ui.label(
                                                RichText::new(format!("cmd: {}", clip(cmd, 80)))
                                                    .size(10.5)
                                                    .color(MUTED_GREY),
                                            );
                                        }
                                    }
                                });
                            ui.add_space(3.0);
                        }
                    });
            });
        });
    });
}

fn render_spawn_edges(ui: &mut Ui, state: &mut ForensicsState, edges: &[SpawnEdgeRow]) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Filter:").size(12.0).color(LABEL_GREY));
        for (label, value) in [
            ("All", SpawnFilter::All),
            ("Accepted", SpawnFilter::AcceptedOnly),
            ("Rejected", SpawnFilter::RejectedOnly),
        ] {
            if ui
                .selectable_label(state.spawn_filter == value, label)
                .clicked()
            {
                state.spawn_filter = value;
            }
        }
    });
    ui.add_space(6.0);

    let filtered: Vec<&SpawnEdgeRow> = edges
        .iter()
        .filter(|e| match state.spawn_filter {
            SpawnFilter::All => true,
            SpawnFilter::AcceptedOnly => e.accepted,
            SpawnFilter::RejectedOnly => !e.accepted,
        })
        .collect();

    if filtered.is_empty() {
        empty(ui, "No spawn edges in this filter.");
        return;
    }

    card(ui, &format!("Spawn edges ({})", filtered.len()), |ui| {
        ScrollArea::vertical()
            .id_salt("forensics-spawn-edges")
            .show(ui, |ui| {
                for e in filtered {
                    let color = if e.accepted { ACCENT_GREEN } else { ACCENT_RED };
                    Frame::default()
                        .fill(Color32::from_rgb(18, 21, 26))
                        .stroke(Stroke::new(1.0, color))
                        .corner_radius(6.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                badge(ui, if e.accepted { "accept" } else { "reject" }, color);
                                ui.label(
                                    RichText::new(clip(&e.spawn_kind, 36))
                                        .size(12.0)
                                        .strong()
                                        .color(VALUE_GREY),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(short_time(&e.updated_at))
                                                .size(11.0)
                                                .color(MUTED_GREY),
                                        );
                                    },
                                );
                            });
                            ui.label(
                                RichText::new(format!(
                                    "{}({}) → {}({})",
                                    e.parent_entity_type,
                                    clip(&e.parent_entity_id, 28),
                                    e.child_entity_type,
                                    clip(&e.child_entity_id, 28)
                                ))
                                .size(11.0)
                                .color(LABEL_GREY),
                            );
                            if !e.spawn_reason.trim().is_empty() {
                                ui.label(
                                    RichText::new(format!(
                                        "reason: {}",
                                        clip(&e.spawn_reason, 110)
                                    ))
                                    .size(11.0)
                                    .color(MUTED_GREY),
                                );
                            }
                            let budget = e.budget_key.as_deref().unwrap_or("·");
                            let max = e
                                .max_attempts
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| "·".to_string());
                            ui.label(
                                RichText::new(format!("budget: {}  ·  max: {}", budget, max))
                                    .size(10.5)
                                    .color(MUTED_GREY),
                            );
                            if !e.accepted && e.violation_codes_json != "[]" {
                                ui.label(
                                    RichText::new(format!(
                                        "violations: {}",
                                        clip(&e.violation_codes_json, 110)
                                    ))
                                    .size(10.5)
                                    .color(ACCENT_RED),
                                );
                            }
                        });
                    ui.add_space(3.0);
                }
            });
    });
}

fn render_violations(ui: &mut Ui, violations: &[PmViolationRow]) {
    if violations.is_empty() {
        empty(ui, "No state violations recorded.");
        return;
    }
    card(
        ui,
        &format!("State violations ({})", violations.len()),
        |ui| {
            ScrollArea::vertical()
                .id_salt("forensics-violations")
                .show(ui, |ui| {
                    for v in violations {
                        let color = severity_color(&v.severity);
                        Frame::default()
                            .fill(Color32::from_rgb(18, 21, 26))
                            .stroke(Stroke::new(1.0, color))
                            .corner_radius(6.0)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    badge(ui, &v.severity, color);
                                    ui.label(
                                        RichText::new(&v.violation_code)
                                            .size(12.0)
                                            .strong()
                                            .color(VALUE_GREY),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                RichText::new(short_time(&v.detected_at))
                                                    .size(11.0)
                                                    .color(MUTED_GREY),
                                            );
                                        },
                                    );
                                });
                                ui.label(
                                    RichText::new(clip(&v.message, 200))
                                        .size(11.5)
                                        .color(LABEL_GREY),
                                );
                                ui.label(
                                    RichText::new(format!("case: {}", clip(&v.case_id, 100)))
                                        .size(10.5)
                                        .color(MUTED_GREY),
                                );
                            });
                        ui.add_space(3.0);
                    }
                });
        },
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn card<R>(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    let inner = Frame::default()
        .fill(CARD_BG)
        .stroke(Stroke::new(1.0, CARD_BORDER))
        .corner_radius(8.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.label(RichText::new(title).size(12.5).strong().color(HEADING_GREY));
            ui.add_space(6.0);
            add_contents(ui)
        });
    inner.inner
}

fn empty(ui: &mut Ui, msg: &str) {
    ui.label(RichText::new(msg).size(12.0).color(MUTED_GREY));
}

fn badge(ui: &mut Ui, text: &str, color: Color32) {
    Frame::default()
        .fill(Color32::from_rgb(36, 40, 48))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(color));
        });
}

fn bar(ui: &mut Ui, width: f32, frac: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 8.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, Color32::from_rgb(28, 32, 38));
    let mut filled = rect;
    filled.set_width(width * frac);
    painter.rect_filled(filled, 2.0, color);
}

fn lifecycle_color(transition: &str) -> Color32 {
    match transition {
        "complete" | "completed" => ACCENT_GREEN,
        "start" | "started" => ACCENT_BLUE,
        "abort" | "fail" | "failed" => ACCENT_RED,
        _ => LABEL_GREY,
    }
}

fn severity_color(sev: &str) -> Color32 {
    match sev {
        "critical" | "error" | "high" => ACCENT_RED,
        "warning" | "medium" => ACCENT_YELLOW,
        "info" | "low" => ACCENT_BLUE,
        _ => LABEL_GREY,
    }
}

fn short_time(value: &str) -> String {
    if let Some((_date, rest)) = value.split_once('T') {
        rest.trim_end_matches('Z')
            .chars()
            .take(8)
            .collect::<String>()
    } else {
        value.to_string()
    }
}

fn clip(value: &str, max: usize) -> String {
    let line = value.lines().next().unwrap_or(value);
    if line.chars().count() <= max {
        line.to_string()
    } else {
        line.chars().take(max.saturating_sub(3)).collect::<String>() + "..."
    }
}
