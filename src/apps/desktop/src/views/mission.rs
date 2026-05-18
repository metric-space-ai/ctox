//! Mission State dashboard with claims and verification runs.

use crate::db_reader::{MissionClaimRow, MissionStateRow, SecretRewriteRow, VerificationRunRow};
use eframe::egui::{self, Color32, Frame, RichText, ScrollArea, Stroke, Ui};

#[derive(Default)]
pub struct MissionUiState {
    pub selected_claim: Option<String>,
    pub selected_run: Option<String>,
}

const COL_OPEN: Color32 = Color32::from_rgb(86, 182, 214);
const COL_CLOSED: Color32 = Color32::from_rgb(94, 184, 116);
const COL_BLOCKER: Color32 = Color32::from_rgb(218, 106, 106);

pub fn render(
    ui: &mut Ui,
    missions: &[MissionStateRow],
    claims: &[MissionClaimRow],
    runs: &[VerificationRunRow],
    secret_rewrites: &[SecretRewriteRow],
    state: &mut MissionUiState,
) {
    ui.heading(
        RichText::new("Mission")
            .size(18.0)
            .color(Color32::from_gray(220)),
    );
    ui.add_space(8.0);

    if missions.is_empty() {
        ui.label(RichText::new("No active mission.").color(Color32::from_gray(120)));
    }

    for mission in missions {
        render_mission_card(ui, mission);
        ui.add_space(12.0);
    }

    // Claims
    if !claims.is_empty() {
        ui.label(
            RichText::new("Claims")
                .size(15.0)
                .strong()
                .color(Color32::from_gray(200)),
        );
        ui.add_space(6.0);

        ScrollArea::vertical()
            .max_height(250.0)
            .id_salt("mission-claims")
            .show(ui, |ui| {
                for claim in claims {
                    let is_selected = state.selected_claim.as_deref() == Some(&claim.claim_key);
                    let status_color = match claim.claim_status.as_str() {
                        "verified" | "confirmed" => COL_CLOSED,
                        "pending" | "open" => COL_OPEN,
                        "failed" | "rejected" => COL_BLOCKER,
                        _ => Color32::from_gray(130),
                    };
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
                                status_color
                            } else {
                                Color32::from_rgb(40, 44, 50)
                            },
                        ))
                        .corner_radius(8.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let icon = match claim.claim_status.as_str() {
                                    "verified" | "confirmed" => "v",
                                    "pending" | "open" => "?",
                                    "failed" | "rejected" => "x",
                                    _ => "-",
                                };
                                ui.label(RichText::new(icon).size(12.0).color(status_color));
                                ui.label(
                                    RichText::new(&claim.subject)
                                        .size(12.5)
                                        .color(Color32::from_gray(200)),
                                );
                                if claim.blocks_closure {
                                    badge(ui, "blocks closure", COL_BLOCKER);
                                }
                                badge(ui, &claim.claim_kind, Color32::from_gray(110));
                            });
                            ui.label(
                                RichText::new(&claim.summary)
                                    .size(11.5)
                                    .color(Color32::from_gray(160)),
                            );
                        });
                    if resp.response.interact(egui::Sense::click()).clicked() {
                        state.selected_claim = Some(claim.claim_key.clone());
                    }
                    ui.add_space(3.0);
                }
            });

        // Claim detail
        if let Some(key) = &state.selected_claim {
            if let Some(claim) = claims.iter().find(|c| c.claim_key == *key) {
                ui.add_space(8.0);
                Frame::default()
                    .fill(Color32::from_rgb(20, 24, 30))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(50, 55, 65)))
                    .corner_radius(8.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("Evidence:")
                                .size(12.0)
                                .color(Color32::from_gray(140)),
                        );
                        ui.label(
                            RichText::new(&claim.evidence_summary)
                                .size(11.5)
                                .color(Color32::from_gray(180)),
                        );
                    });
            }
        }
        ui.add_space(12.0);
    }

    // Verification Runs
    if !runs.is_empty() {
        ui.label(
            RichText::new("Verification Runs")
                .size(15.0)
                .strong()
                .color(Color32::from_gray(200)),
        );
        ui.add_space(6.0);

        ScrollArea::vertical()
            .max_height(200.0)
            .id_salt("verification-runs")
            .show(ui, |ui| {
                for run in runs {
                    let score_color = if run.review_score >= 80 {
                        COL_CLOSED
                    } else if run.review_score >= 50 {
                        Color32::from_rgb(220, 190, 90)
                    } else {
                        COL_BLOCKER
                    };
                    Frame::default()
                        .fill(Color32::from_rgb(24, 27, 32))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 50)))
                        .corner_radius(8.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                badge(ui, &format!("score {}", run.review_score), score_color);
                                badge(ui, &run.review_verdict, Color32::from_gray(130));
                                badge(
                                    ui,
                                    &format!(
                                        "{}/{} claims open",
                                        run.open_claim_count, run.claim_count
                                    ),
                                    Color32::from_gray(110),
                                );
                            });
                            ui.label(
                                RichText::new(&run.goal)
                                    .size(12.0)
                                    .color(Color32::from_gray(200)),
                            );
                            ui.label(
                                RichText::new(&run.created_at)
                                    .size(11.0)
                                    .color(Color32::from_gray(100)),
                            );
                        });
                    ui.add_space(3.0);
                }
            });
        ui.add_space(12.0);
    }

    // Secret Rewrites
    if !secret_rewrites.is_empty() {
        ui.label(
            RichText::new("Secret Rewrites")
                .size(15.0)
                .strong()
                .color(Color32::from_gray(200)),
        );
        ui.add_space(6.0);

        for rewrite in secret_rewrites {
            Frame::default()
                .fill(Color32::from_rgb(24, 27, 32))
                .stroke(Stroke::new(1.0, Color32::from_rgb(40, 44, 50)))
                .corner_radius(8.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&rewrite.secret_name)
                                .size(12.5)
                                .color(Color32::from_gray(200)),
                        );
                        badge(ui, &rewrite.secret_scope, Color32::from_gray(120));
                        badge(
                            ui,
                            &format!(
                                "{} msg, {} sum rows",
                                rewrite.message_rows_updated, rewrite.summary_rows_updated
                            ),
                            Color32::from_gray(100),
                        );
                    });
                    ui.label(
                        RichText::new(&rewrite.created_at)
                            .size(11.0)
                            .color(Color32::from_gray(100)),
                    );
                });
            ui.add_space(3.0);
        }
    }
}

fn render_mission_card(ui: &mut Ui, mission: &MissionStateRow) {
    let status_color = if mission.is_open {
        COL_OPEN
    } else {
        COL_CLOSED
    };
    Frame::default()
        .fill(Color32::from_rgb(20, 24, 30))
        .stroke(Stroke::new(1.5, status_color))
        .corner_radius(12.0)
        .inner_margin(14.0)
        .show(ui, |ui| {
            // Objective
            ui.label(
                RichText::new(&mission.mission)
                    .size(16.0)
                    .strong()
                    .color(Color32::from_gray(230)),
            );
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                badge(ui, &mission.mission_status, status_color);
                badge(ui, &mission.continuation_mode, Color32::from_gray(120));
                badge(
                    ui,
                    &format!("trigger: {}", mission.trigger_intensity),
                    Color32::from_gray(110),
                );
                if mission.allow_idle {
                    badge(ui, "idle ok", Color32::from_gray(100));
                }
            });
            ui.add_space(6.0);

            // Blocker
            if !mission.blocker.is_empty() && mission.blocker != "null" && mission.blocker != "\"\""
            {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Blocker:").size(12.0).color(COL_BLOCKER));
                    ui.label(
                        RichText::new(&mission.blocker)
                            .size(12.0)
                            .color(Color32::from_gray(190)),
                    );
                });
            }

            // Next slice
            if !mission.next_slice.is_empty() && mission.next_slice != "null" {
                ui.label(
                    RichText::new("Next slice:")
                        .size(12.0)
                        .color(Color32::from_gray(140)),
                );
                ui.label(
                    RichText::new(&mission.next_slice)
                        .size(11.5)
                        .color(Color32::from_gray(180)),
                );
            }

            // Done gate
            if !mission.done_gate.is_empty() && mission.done_gate != "null" {
                ui.label(
                    RichText::new("Done gate:")
                        .size(12.0)
                        .color(Color32::from_gray(140)),
                );
                ui.label(
                    RichText::new(&mission.done_gate)
                        .size(11.5)
                        .color(Color32::from_gray(180)),
                );
            }

            // Closure confidence
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Confidence:")
                        .size(12.0)
                        .color(Color32::from_gray(130)),
                );
                ui.label(
                    RichText::new(&mission.closure_confidence)
                        .size(12.0)
                        .color(Color32::from_gray(180)),
                );
            });

            ui.label(
                RichText::new(format!("Synced: {}", mission.last_synced_at))
                    .size(11.0)
                    .color(Color32::from_gray(100)),
            );
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
