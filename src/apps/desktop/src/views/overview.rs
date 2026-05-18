//! Overview view: dashboard for a single CTOX instance.
//!
//! Aggregates the read-only data already provided by `db_reader` into a small
//! set of cards so the operator can see the runtime state at a glance —
//! mission, tokens, tickets, reviews, communications, and the process-mining
//! snapshot.

use eframe::egui::{self, Color32, Frame, RichText, ScrollArea, Stroke, Ui};

use crate::db_reader::{
    CommCounts, MissionStateRow, PmCounts, ReviewCounts, ThreadTokenSummary, TicketCounts,
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

pub struct OverviewData<'a> {
    pub missions: &'a [MissionStateRow],
    pub tokens: &'a ThreadTokenSummary,
    pub tickets: &'a TicketCounts,
    pub reviews: &'a ReviewCounts,
    pub comms: &'a CommCounts,
    pub pm: &'a PmCounts,
}

pub fn render(ui: &mut Ui, data: OverviewData<'_>) {
    ScrollArea::vertical()
        .id_salt("overview-scroll")
        .show(ui, |ui| {
            ui.heading(
                RichText::new("Instance Overview")
                    .size(18.0)
                    .color(HEADING_GREY),
            );
            ui.add_space(8.0);

            render_mission_now(ui, data.missions);
            ui.add_space(10.0);

            render_top_row(ui, &data);
            ui.add_space(10.0);

            render_token_breakdown(ui, data.tokens);
            ui.add_space(10.0);

            ui.horizontal_top(|ui| {
                let width = (ui.available_width() - 10.0) * 0.5;
                ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
                    render_process_mining_card(ui, data.pm);
                });
                ui.add_space(10.0);
                ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
                    render_review_gate_card(ui, data.reviews);
                });
            });
            ui.add_space(10.0);

            ui.horizontal_top(|ui| {
                let width = (ui.available_width() - 10.0) * 0.5;
                ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
                    render_pairs_card(ui, "Tickets by state", &data.tickets.by_state);
                });
                ui.add_space(10.0);
                ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
                    render_pairs_card(ui, "Comms by channel", &data.comms.by_channel);
                });
            });
            ui.add_space(10.0);

            ui.horizontal_top(|ui| {
                let width = (ui.available_width() - 10.0) * 0.5;
                ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
                    render_pairs_card(ui, "Tickets by risk", &data.tickets.by_risk);
                });
                ui.add_space(10.0);
                ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
                    render_pairs_card(ui, "Queue by route status", &data.comms.by_route_status);
                });
            });
        });
}

fn render_mission_now(ui: &mut Ui, missions: &[MissionStateRow]) {
    card(ui, "Mission · now", |ui| {
        let Some(m) = missions
            .iter()
            .find(|m| m.is_open)
            .or_else(|| missions.first())
        else {
            ui.label(
                RichText::new("No mission state yet.")
                    .size(12.5)
                    .color(MUTED_GREY),
            );
            return;
        };
        ui.horizontal_wrapped(|ui| {
            badge(ui, &m.mission_status, status_color(&m.mission_status));
            badge(ui, &format!("conf {}", m.closure_confidence), ACCENT_BLUE);
            badge(ui, &m.continuation_mode, LABEL_GREY);
            if !m.blocker.trim().is_empty() && m.blocker != "none" {
                badge(ui, "blocker", ACCENT_RED);
            }
        });
        ui.add_space(6.0);
        kv(ui, "Mission", &clip(&m.mission, 200));
        if !m.next_slice.trim().is_empty() {
            kv(ui, "Next slice", &clip(&m.next_slice, 200));
        }
        if !m.blocker.trim().is_empty() && m.blocker != "none" {
            kv(ui, "Blocker", &clip(&m.blocker, 200));
        }
        if !m.done_gate.trim().is_empty() {
            kv(ui, "Done gate", &clip(&m.done_gate, 200));
        }
    });
}

fn render_top_row(ui: &mut Ui, data: &OverviewData<'_>) {
    ui.horizontal_top(|ui| {
        let gap = 10.0;
        let count = 4.0;
        let width = (ui.available_width() - gap * (count - 1.0)) / count;

        ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
            metric_card(
                ui,
                "Tokens",
                &fmt_thousands(data.tokens.total_tokens),
                &format!("{} active threads", data.tokens.active_thread_count),
                ACCENT_BLUE,
            );
        });
        ui.add_space(gap);
        ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
            let high_risk = data
                .tickets
                .by_risk
                .iter()
                .filter(|(k, _)| matches!(k.as_str(), "high" | "critical"))
                .map(|(_, n)| *n)
                .sum::<i64>();
            metric_card(
                ui,
                "Tickets",
                &data.tickets.total.to_string(),
                &format!("{} high-risk", high_risk),
                if high_risk > 0 {
                    ACCENT_YELLOW
                } else {
                    ACCENT_GREEN
                },
            );
        });
        ui.add_space(gap);
        ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
            let blocking = data.reviews.open_blocking_claims;
            metric_card(
                ui,
                "Reviews",
                &data.reviews.verification_total.to_string(),
                &format!("{} blocking claims", blocking),
                if blocking > 0 {
                    ACCENT_RED
                } else {
                    ACCENT_GREEN
                },
            );
        });
        ui.add_space(gap);
        ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
            let pending = data
                .comms
                .by_route_status
                .iter()
                .find(|(k, _)| k == "pending")
                .map(|(_, n)| *n)
                .unwrap_or(0);
            let total = data.comms.by_channel.iter().map(|(_, n)| *n).sum::<i64>();
            metric_card(
                ui,
                "Comms",
                &total.to_string(),
                &format!("{} pending", pending),
                if pending > 0 {
                    ACCENT_YELLOW
                } else {
                    ACCENT_GREEN
                },
            );
        });
    });
}

fn render_token_breakdown(ui: &mut Ui, tokens: &ThreadTokenSummary) {
    card(ui, "Tokens by model", |ui| {
        if tokens.by_model.is_empty() {
            ui.label(
                RichText::new("No active threads.")
                    .size(12.0)
                    .color(MUTED_GREY),
            );
            return;
        }
        let max = tokens
            .by_model
            .iter()
            .map(|(_, n)| *n)
            .max()
            .unwrap_or(1)
            .max(1);
        let total = tokens.total_tokens.max(1);
        for (model, n) in &tokens.by_model {
            ui.horizontal(|ui| {
                ui.allocate_ui(egui::vec2(180.0, 0.0), |ui| {
                    ui.label(RichText::new(model).size(12.0).color(VALUE_GREY));
                });
                let frac = (*n as f32 / max as f32).clamp(0.0, 1.0);
                let bar_width = (ui.available_width() - 140.0).max(40.0);
                bar(ui, bar_width, frac, ACCENT_BLUE);
                ui.label(
                    RichText::new(format!(
                        "{} ({:.1}%)",
                        fmt_thousands(*n),
                        (*n as f32 / total as f32) * 100.0
                    ))
                    .size(11.5)
                    .color(LABEL_GREY),
                );
            });
            ui.add_space(2.0);
        }
    });
}

fn render_process_mining_card(ui: &mut Ui, pm: &PmCounts) {
    card(ui, "Process-mining snapshot", |ui| {
        if pm.events_total == 0 && pm.cases_total == 0 {
            ui.label(
                RichText::new("No process-mining events recorded yet.")
                    .size(12.0)
                    .color(MUTED_GREY),
            );
            return;
        }
        stat_row(ui, "Events", &fmt_thousands(pm.events_total), VALUE_GREY);
        stat_row(ui, "Cases", &fmt_thousands(pm.cases_total), VALUE_GREY);
        stat_row(
            ui,
            "Violations",
            &fmt_thousands(pm.violations_open),
            if pm.violations_open > 0 {
                ACCENT_RED
            } else {
                ACCENT_GREEN
            },
        );
        stat_row(
            ui,
            "Spawn edges",
            &format!(
                "{} ({} rejected)",
                fmt_thousands(pm.spawn_edges_total),
                pm.spawn_edges_rejected
            ),
            if pm.spawn_edges_rejected > 0 {
                ACCENT_YELLOW
            } else {
                VALUE_GREY
            },
        );
        stat_row(
            ui,
            "Transition proofs",
            &format!(
                "{} accepted / {} rejected",
                fmt_thousands(pm.proofs_accepted),
                pm.proofs_rejected
            ),
            if pm.proofs_rejected > 0 {
                ACCENT_RED
            } else {
                ACCENT_GREEN
            },
        );
    });
}

fn render_review_gate_card(ui: &mut Ui, reviews: &ReviewCounts) {
    card(ui, "Review gate", |ui| {
        if reviews.verification_total == 0 && reviews.by_verdict.is_empty() {
            ui.label(
                RichText::new("No verification runs yet.")
                    .size(12.0)
                    .color(MUTED_GREY),
            );
            return;
        }
        stat_row(
            ui,
            "Total runs",
            &reviews.verification_total.to_string(),
            VALUE_GREY,
        );
        for (verdict, n) in &reviews.by_verdict {
            let color = match verdict.as_str() {
                "pass" | "accepted" => ACCENT_GREEN,
                "fail" | "rejected" => ACCENT_RED,
                "wording" | "stale" => ACCENT_YELLOW,
                _ => LABEL_GREY,
            };
            stat_row(ui, verdict, &n.to_string(), color);
        }
        stat_row(
            ui,
            "Blocking claims open",
            &reviews.open_blocking_claims.to_string(),
            if reviews.open_blocking_claims > 0 {
                ACCENT_RED
            } else {
                ACCENT_GREEN
            },
        );
    });
}

fn render_pairs_card(ui: &mut Ui, title: &str, pairs: &[(String, i64)]) {
    card(ui, title, |ui| {
        if pairs.is_empty() {
            ui.label(RichText::new("No data.").size(12.0).color(MUTED_GREY));
            return;
        }
        for (k, n) in pairs {
            stat_row(ui, k, &n.to_string(), VALUE_GREY);
        }
    });
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

fn metric_card(ui: &mut Ui, label: &str, value: &str, sublabel: &str, value_color: Color32) {
    card(ui, label, |ui| {
        ui.label(RichText::new(value).size(22.0).strong().color(value_color));
        ui.add_space(2.0);
        ui.label(RichText::new(sublabel).size(11.5).color(MUTED_GREY));
    });
}

fn stat_row(ui: &mut Ui, label: &str, value: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(12.0).color(LABEL_GREY));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).size(12.0).color(color));
        });
    });
}

fn kv(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!("{label}:"))
                .size(11.5)
                .color(LABEL_GREY),
        );
        ui.label(RichText::new(value).size(11.5).color(VALUE_GREY));
    });
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

fn status_color(status: &str) -> Color32 {
    match status {
        "working" | "open" | "in_progress" => ACCENT_GREEN,
        "blocked" => ACCENT_RED,
        "deferred" | "paused" => ACCENT_YELLOW,
        "completed" | "closed" => ACCENT_BLUE,
        _ => LABEL_GREY,
    }
}

fn fmt_thousands(n: i64) -> String {
    let neg = n < 0;
    let abs = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(abs.len() + abs.len() / 3);
    for (idx, ch) in abs.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if neg {
        out.push('-');
    }
    out.chars().rev().collect()
}

fn clip(value: &str, max: usize) -> String {
    let line = value.lines().next().unwrap_or(value);
    if line.chars().count() <= max {
        line.to_string()
    } else {
        line.chars().take(max.saturating_sub(3)).collect::<String>() + "..."
    }
}
