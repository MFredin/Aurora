//! A native window drawn with egui at roughly the complexity of the Pulse design (rail, gauge,
//! stat tiles, settings rows with switches, app grid). It does no scanning: it only exists so
//! `scripts/measure.ps1` can compare a native UI's idle cost with the WebView2 app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2,
};
use std::time::Instant;

const BG: Color32 = Color32::from_rgb(10, 11, 16);
const CARD: Color32 = Color32::from_rgb(20, 22, 30);
const LINE: Color32 = Color32::from_rgb(34, 37, 48);
const TEXT: Color32 = Color32::from_rgb(238, 240, 246);
const MUTED: Color32 = Color32::from_rgb(138, 144, 166);
const VIOLET: Color32 = Color32::from_rgb(124, 92, 255);
const CYAN: Color32 = Color32::from_rgb(34, 211, 238);

struct Probe {
    started: Instant,
    reported: bool,
    switches: [bool; 4],
    apps: [bool; 4],
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(16)
        .show(ui, add);
}

fn gauge(ui: &mut egui::Ui, done: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(170.0), Sense::hover());
    let c = rect.center();
    let r = 72.0;
    let arc = |from: f32, to: f32| -> Vec<Pos2> {
        (0..=64)
            .map(|i| {
                let a = from + (to - from) * i as f32 / 64.0 - std::f32::consts::FRAC_PI_2;
                c + Vec2::angled(a) * r
            })
            .collect()
    };
    let p = ui.painter();
    p.add(Shape::line(
        arc(0.0, std::f32::consts::TAU),
        Stroke::new(12.0, LINE),
    ));
    p.add(Shape::line(
        arc(0.0, std::f32::consts::TAU * done),
        Stroke::new(12.0, VIOLET),
    ));
    p.text(
        c - Vec2::new(0.0, 8.0),
        egui::Align2::CENTER_CENTER,
        "5",
        egui::FontId::proportional(40.0),
        TEXT,
    );
    p.text(
        c + Vec2::new(0.0, 22.0),
        egui::Align2::CENTER_CENTER,
        "to tune",
        egui::FontId::proportional(13.0),
        MUTED,
    );
}

fn switch(ui: &mut egui::Ui, on: &mut bool) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(40.0, 20.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(10), if *on { CYAN } else { LINE });
    let x = if *on {
        rect.right() - 10.0
    } else {
        rect.left() + 10.0
    };
    p.circle_filled(Pos2::new(x, rect.center().y), 6.0, BG);
}

impl eframe::App for Probe {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.reported {
            self.reported = true;
            if let Some(path) = std::env::var_os("FORGE_PERF_LOG") {
                let _ = std::fs::write(path, self.started.elapsed().as_millis().to_string());
            }
        }
        let full = ui.max_rect();
        ui.painter().rect_filled(full, CornerRadius::ZERO, BG);
        let rail = Rect::from_min_size(full.min, Vec2::new(76.0, full.height()));
        ui.painter().line_segment(
            [rail.right_top(), rail.right_bottom()],
            Stroke::new(1.0, LINE),
        );
        for i in 0..4 {
            let r = Rect::from_center_size(
                Pos2::new(rail.center().x, 60.0 + i as f32 * 60.0),
                Vec2::splat(44.0),
            );
            ui.painter().rect(
                r,
                CornerRadius::same(12),
                if i == 0 { CARD } else { BG },
                Stroke::new(1.0, LINE),
                StrokeKind::Inside,
            );
        }
        let content = Rect::from_min_max(
            Pos2::new(rail.right() + 28.0, full.top() + 24.0),
            full.max - Vec2::splat(24.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
            ui.heading(
                egui::RichText::new("Overview")
                    .color(TEXT)
                    .size(24.0)
                    .strong(),
            );
            ui.add_space(12.0);
            ui.horizontal_top(|ui| {
                card(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        gauge(ui, 1.0 / 6.0);
                        ui.label(egui::RichText::new("Intel Core 5 320 · 16 GB").color(MUTED));
                        let _ = ui.add_sized(
                            [260.0, 42.0],
                            egui::Button::new(
                                egui::RichText::new("Apply 3 safe fixes").color(BG).strong(),
                            )
                            .fill(VIOLET),
                        );
                    });
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        for (k, v) in [
                            ("Refresh rate", "60 / 120 Hz"),
                            ("Startup apps", "2 flagged"),
                            ("Bloatware", "4 apps"),
                            ("Memory", "5600 MT/s"),
                        ] {
                            card(ui, |ui| {
                                ui.set_width(150.0);
                                ui.label(egui::RichText::new(k).color(MUTED).size(12.0));
                                ui.label(egui::RichText::new(v).color(TEXT).size(20.0).strong());
                            });
                        }
                    });
                    ui.add_space(12.0);
                    card(ui, |ui| {
                        ui.set_width(700.0);
                        ui.label(egui::RichText::new("Bloatware").color(TEXT).strong());
                        ui.horizontal(|ui| {
                            for (i, name) in [
                                "Solitaire Collection",
                                "Bing News",
                                "Bing Weather",
                                "Clipchamp",
                            ]
                            .iter()
                            .enumerate()
                            {
                                ui.checkbox(
                                    &mut self.apps[i],
                                    egui::RichText::new(*name).color(TEXT),
                                );
                            }
                        });
                    });
                });
            });
            ui.add_space(14.0);
            for (i, (title, sub)) in [
                (
                    "Windows installs promoted apps on its own",
                    "Keeps removed apps from coming back",
                ),
                ("Mouse acceleration", "Same hand movement, same aim"),
                (
                    "Game Bar background recording",
                    "Screenshots still work when off",
                ),
                ("OneDrive starts with Windows", "Keep it if you sync files"),
            ]
            .iter()
            .enumerate()
            {
                card(ui, |ui| {
                    ui.set_width(content.width() - 34.0);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(*title).color(TEXT));
                            ui.label(egui::RichText::new(*sub).color(MUTED).size(12.0));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            switch(ui, &mut self.switches[i]);
                        });
                    });
                });
                ui.add_space(6.0);
            }
        });
    }
}

fn main() -> eframe::Result {
    let started = Instant::now();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Forge native probe")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Forge native probe",
        options,
        Box::new(move |_| {
            Ok(Box::new(Probe {
                started,
                reported: false,
                switches: [true; 4],
                apps: [true, true, true, false],
            }))
        }),
    )
}
