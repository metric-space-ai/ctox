//! Harness flow view: a single monospaced flow trace with no side panels.

use eframe::egui::{self, Color32, RichText, ScrollArea, Ui};

pub fn render(ui: &mut Ui, flow_text: &str) {
    ui.heading(
        RichText::new("Harness Flow")
            .size(18.0)
            .color(Color32::from_gray(220)),
    );
    ui.add_space(6.0);

    ScrollArea::both()
        .id_salt("harness-flow-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(flow_text)
                        .family(egui::FontFamily::Monospace)
                        .size(12.0)
                        .color(Color32::from_gray(220)),
                )
                .selectable(true),
            );
        });
}
