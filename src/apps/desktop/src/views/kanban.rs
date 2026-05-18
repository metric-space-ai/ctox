//! Kanban board for CTOX tickets with popup detail and approval.

use std::collections::BTreeMap;

use crate::db_reader::{ExecutionActionRow, TicketCaseRow, TicketItemRow};
use eframe::egui::{self, Color32, Frame, RichText, ScrollArea, Stroke, Ui};

#[derive(Default)]
pub struct KanbanState {
    pub popup_ticket: Option<String>,
    pub deny_rationale: String,
    pub approval_result: Option<String>, // feedback message after approve/deny
}

const KANBAN_COLUMNS: &[(&str, &str, Color32)] = &[
    ("open", "Open", Color32::from_rgb(86, 182, 214)),
    (
        "approval_pending",
        "Approval Pending",
        Color32::from_rgb(220, 190, 90),
    ),
    (
        "writeback_pending",
        "Writeback",
        Color32::from_rgb(180, 140, 220),
    ),
    ("closed", "Closed", Color32::from_rgb(94, 184, 116)),
];

pub fn render(
    ui: &mut Ui,
    tickets: &[TicketItemRow],
    cases: &[TicketCaseRow],
    actions: &[ExecutionActionRow],
    state: &mut KanbanState,
    root: Option<&std::path::Path>,
) {
    ui.heading(
        RichText::new("Tickets")
            .size(18.0)
            .color(Color32::from_gray(220)),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("{} tickets, {} cases", tickets.len(), cases.len()))
            .size(12.5)
            .color(Color32::from_gray(130)),
    );
    ui.add_space(10.0);

    if tickets.is_empty() {
        ui.add_space(20.0);
        ui.label(
            RichText::new(
                "No tickets yet. Tickets appear when the agent receives support requests.",
            )
            .size(13.0)
            .color(Color32::from_gray(120)),
        );
        return;
    }

    // Latest case state per ticket
    let latest_case_state: BTreeMap<&str, &str> = {
        let mut m: BTreeMap<&str, &TicketCaseRow> = BTreeMap::new();
        for case in cases {
            let existing = m.get(case.ticket_key.as_str());
            if existing.is_none() || existing.unwrap().updated_at < case.updated_at {
                m.insert(&case.ticket_key, case);
            }
        }
        m.into_iter().map(|(k, c)| (k, c.state.as_str())).collect()
    };

    let cases_by_ticket: BTreeMap<&str, Vec<&TicketCaseRow>> = {
        let mut m: BTreeMap<&str, Vec<&TicketCaseRow>> = BTreeMap::new();
        for case in cases {
            m.entry(&case.ticket_key).or_default().push(case);
        }
        m
    };

    let col_count = KANBAN_COLUMNS.len();

    // Board: full width columns
    ui.columns(col_count, |columns| {
        for (i, (col_state, col_label, col_color)) in KANBAN_COLUMNS.iter().enumerate() {
            let ui = &mut columns[i];

            let col_tickets: Vec<&TicketItemRow> = tickets
                .iter()
                .filter(|t| {
                    let s = latest_case_state
                        .get(t.ticket_key.as_str())
                        .unwrap_or(&"open");
                    *s == *col_state
                })
                .collect();

            // Column header
            ui.horizontal(|ui| {
                Frame::default()
                    .fill(*col_color)
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(*col_label)
                                .size(12.0)
                                .strong()
                                .color(Color32::BLACK),
                        );
                    });
                ui.label(
                    RichText::new(format!("{}", col_tickets.len()))
                        .size(12.0)
                        .color(Color32::from_gray(120)),
                );
            });
            ui.add_space(6.0);

            // Cards
            for ticket in &col_tickets {
                let is_popup = state.popup_ticket.as_deref() == Some(&ticket.ticket_key);
                let fill = if is_popup {
                    Color32::from_rgb(32, 38, 50)
                } else {
                    Color32::from_rgb(24, 27, 33)
                };
                let border = if is_popup {
                    *col_color
                } else {
                    Color32::from_rgb(40, 44, 52)
                };

                let resp = Frame::default()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, border))
                    .corner_radius(8.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(&ticket.remote_ticket_id)
                                .size(10.5)
                                .family(egui::FontFamily::Monospace)
                                .color(Color32::from_gray(110)),
                        );
                        ui.label(
                            RichText::new(truncate(&ticket.title, 40))
                                .size(12.5)
                                .color(Color32::from_gray(220)),
                        );
                        ui.horizontal(|ui| {
                            if let Some(prio) = &ticket.priority {
                                let pc = match prio.as_str() {
                                    "high" | "urgent" => Color32::from_rgb(218, 106, 106),
                                    _ => Color32::from_gray(120),
                                };
                                badge(ui, prio, pc);
                            }
                            if let Some(tc) = cases_by_ticket.get(ticket.ticket_key.as_str()) {
                                if let Some(latest) = tc.first() {
                                    badge(ui, &latest.label, Color32::from_rgb(160, 130, 80));
                                }
                            }
                        });
                    });

                if resp.response.interact(egui::Sense::click()).clicked() {
                    state.popup_ticket = Some(ticket.ticket_key.clone());
                }
                ui.add_space(5.0);
            }
        }
    });

    // Popup detail window
    render_popup(ui, tickets, cases, actions, &cases_by_ticket, state, root);
}

fn render_popup(
    ui: &mut Ui,
    tickets: &[TicketItemRow],
    _cases: &[TicketCaseRow],
    actions: &[ExecutionActionRow],
    cases_by_ticket: &BTreeMap<&str, Vec<&TicketCaseRow>>,
    state: &mut KanbanState,
    root: Option<&std::path::Path>,
) {
    let Some(ticket_key) = state.popup_ticket.clone() else {
        return;
    };
    let Some(ticket) = tickets.iter().find(|t| t.ticket_key == ticket_key) else {
        state.popup_ticket = None;
        return;
    };

    let mut open = true;
    egui::Window::new(format!(
        "{} - {}",
        ticket.remote_ticket_id,
        truncate(&ticket.title, 50)
    ))
    .open(&mut open)
    .default_width(500.0)
    .default_height(400.0)
    .resizable(true)
    .collapsible(false)
    .show(ui.ctx(), |ui| {
        // Header
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(&ticket.remote_ticket_id)
                    .size(14.0)
                    .family(egui::FontFamily::Monospace)
                    .color(Color32::from_gray(180)),
            );
            badge(ui, &ticket.remote_status, Color32::from_rgb(86, 182, 214));
            if let Some(prio) = &ticket.priority {
                let pc = match prio.as_str() {
                    "high" | "urgent" => Color32::from_rgb(218, 106, 106),
                    _ => Color32::from_gray(120),
                };
                badge(ui, prio, pc);
            }
            badge(ui, &ticket.source_system, Color32::from_gray(100));
        });

        ui.label(
            RichText::new(&ticket.title)
                .size(16.0)
                .strong()
                .color(Color32::from_gray(230)),
        );

        if let Some(req) = &ticket.requester {
            ui.label(
                RichText::new(format!("Requester: {}", req))
                    .size(12.0)
                    .color(Color32::from_gray(150)),
            );
        }
        ui.add_space(6.0);

        // Body
        if !ticket.body_text.is_empty() {
            Frame::default()
                .fill(Color32::from_rgb(16, 18, 22))
                .corner_radius(6.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ScrollArea::vertical()
                        .max_height(100.0)
                        .id_salt("popup-body")
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(&ticket.body_text)
                                    .size(12.0)
                                    .color(Color32::from_gray(180)),
                            );
                        });
                });
            ui.add_space(8.0);
        }

        // Cases
        if let Some(ticket_cases) = cases_by_ticket.get(ticket_key.as_str()) {
            ui.label(
                RichText::new(format!("Cases ({})", ticket_cases.len()))
                    .size(13.0)
                    .strong()
                    .color(Color32::from_gray(200)),
            );
            ui.add_space(4.0);
            for case in ticket_cases {
                let sc = case_state_color(&case.state);
                Frame::default()
                    .fill(Color32::from_rgb(22, 26, 32))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 52)))
                    .corner_radius(6.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            badge(ui, &case.state, sc);
                            badge(ui, &case.label, Color32::from_rgb(160, 130, 80));
                            badge(ui, &case.approval_mode, Color32::from_gray(110));
                            badge(
                                ui,
                                &format!("risk: {}", case.risk_level),
                                Color32::from_gray(100),
                            );
                        });
                        // Approval buttons for pending cases
                        if case.state == "approval_pending" {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new("Needs approval")
                                    .size(11.0)
                                    .color(Color32::from_rgb(220, 190, 90)),
                            );
                            ui.horizontal(|ui| {
                                if ui
                                    .button(
                                        RichText::new("Approve")
                                            .color(Color32::from_rgb(94, 184, 116)),
                                    )
                                    .clicked()
                                {
                                    if let Some(root) = root {
                                        match crate::db_writer::approve_case(
                                            root,
                                            &case.case_id,
                                            "",
                                        ) {
                                            Ok(()) => {
                                                state.approval_result =
                                                    Some(format!("Approved: {}", case.label))
                                            }
                                            Err(e) => {
                                                state.approval_result =
                                                    Some(format!("Error: {}", e))
                                            }
                                        }
                                    }
                                }
                                if ui
                                    .button(
                                        RichText::new("Deny")
                                            .color(Color32::from_rgb(218, 106, 106)),
                                    )
                                    .clicked()
                                {
                                    let rationale = if state.deny_rationale.is_empty() {
                                        "Denied by owner".to_owned()
                                    } else {
                                        state.deny_rationale.clone()
                                    };
                                    if let Some(root) = root {
                                        match crate::db_writer::deny_case(
                                            root,
                                            &case.case_id,
                                            &rationale,
                                        ) {
                                            Ok(()) => {
                                                state.approval_result =
                                                    Some(format!("Denied: {}", case.label));
                                                state.deny_rationale.clear();
                                            }
                                            Err(e) => {
                                                state.approval_result =
                                                    Some(format!("Error: {}", e))
                                            }
                                        }
                                    }
                                }
                            });
                            ui.add(
                                egui::TextEdit::singleline(&mut state.deny_rationale)
                                    .desired_width(300.0)
                                    .hint_text("Reason for denial (optional for approve)"),
                            );
                        }
                    });
                ui.add_space(3.0);
            }
            ui.add_space(6.0);
        }

        // Execution actions for this ticket
        let ticket_actions: Vec<&ExecutionActionRow> = actions
            .iter()
            .filter(|a| a.ticket_key == ticket_key)
            .collect();
        if !ticket_actions.is_empty() {
            ui.label(
                RichText::new("Actions taken")
                    .size(13.0)
                    .strong()
                    .color(Color32::from_gray(200)),
            );
            ui.add_space(4.0);
            for action in &ticket_actions {
                Frame::default()
                    .fill(Color32::from_rgb(18, 24, 18))
                    .corner_radius(6.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(&action.summary)
                                .size(12.0)
                                .color(Color32::from_rgb(160, 210, 160)),
                        );
                        ui.label(
                            RichText::new(&action.created_at)
                                .size(10.5)
                                .color(Color32::from_gray(100)),
                        );
                    });
                ui.add_space(2.0);
            }
        }

        // Approval result feedback
        if let Some(result) = &state.approval_result {
            ui.add_space(6.0);
            let color = if result.starts_with("Error") {
                Color32::from_rgb(218, 106, 106)
            } else {
                Color32::from_rgb(94, 184, 116)
            };
            ui.label(RichText::new(result).size(12.0).color(color));
        }

        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "Created: {}  Updated: {}",
                ticket.created_at, ticket.updated_at
            ))
            .size(10.5)
            .color(Color32::from_gray(90)),
        );
    });

    if !open {
        state.popup_ticket = None;
    }
}

fn case_state_color(state: &str) -> Color32 {
    match state {
        "closed" => Color32::from_rgb(94, 184, 116),
        "approval_pending" => Color32::from_rgb(220, 190, 90),
        "writeback_pending" => Color32::from_rgb(180, 140, 220),
        _ => Color32::from_rgb(86, 182, 214),
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

fn truncate(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or(s);
    if line.chars().count() <= max {
        line.to_owned()
    } else {
        line.chars().take(max).collect::<String>() + "..."
    }
}
