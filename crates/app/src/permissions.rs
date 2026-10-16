//! The permissions window: what ScreenCraft needs from the operating system, with a button for
//! each (a port of `permissions.html`).

use crate::kit;
use crate::recorder::Recorder;
use crate::theme::*;
use eframe::egui::{self, Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, ViewportBuilder, ViewportId};

fn open_pane(url: &str) {
    let _ = crate::editor::open_url(url);
}

/// Draw the window; returns false once the user is done with it.
pub fn show(ctx: &egui::Context, rec: &mut Recorder) -> bool {
    let mut keep = true;
    let p = rec.permissions;
    let builder = ViewportBuilder::default().with_title("ScreenCraft permissions").with_inner_size([600.0, 760.0]).with_resizable(false);
    ctx.show_viewport_immediate(ViewportId::from_hash_of("permissions"), builder, |ui, _| {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin { left: 34, right: 34, top: 28, bottom: 24 })).show_inside(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 10.0);
            ui.label(egui::RichText::new("Let's set up permissions").size(21.0).color(TEXT));
            ui.add(egui::Label::new(egui::RichText::new("ScreenCraft needs a couple of macOS permissions before it can record. Grant them below — this window checks automatically.").size(13.0).color(MUTED)).wrap());
            ui.add_space(4.0);
            let (line, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(line, 0.0, LINE);
            let mut perm = |ui: &mut egui::Ui, icon: &str, name: &str, required: bool, desc: &str, state: Option<bool>, button: &str| -> bool {
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 64.0), Sense::hover());
                let edge = match state {
                    Some(true) => OK,
                    Some(false) => DANGER,
                    None => LINE_2,
                };
                ui.painter().rect_filled(r, CornerRadius::same(8), PANEL);
                ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, LINE), StrokeKind::Inside);
                ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(3.0, r.height())), CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, edge);
                let ic = Rect::from_min_size(Pos2::new(r.min.x + 17.0, r.center().y - 18.0), Vec2::splat(36.0));
                ui.painter().rect_filled(ic, CornerRadius::same(8), PANEL_2);
                crate::icons::paint(ui, icon, Rect::from_center_size(ic.center(), Vec2::splat(20.0)), Color32::from_rgb(0xb9, 0xc4, 0xd9));
                ui.painter().text(Pos2::new(r.min.x + 66.0, r.center().y - 9.0), Align2::LEFT_CENTER, name, kit::font(13.0), TEXT);
                let nw = kit::text_width(ui, name, 13.0);
                let tag = if required { "REQUIRED" } else { "OPTIONAL" };
                let (tbg, tfg) = if required { (alpha(DANGER, 0.14), Color32::from_rgb(0xff, 0x9a, 0xa6)) } else { (Color32::from_rgba_unmultiplied(134, 147, 170, 38), Color32::from_rgb(0x9f, 0xb0, 0xd0)) };
                let tr = Rect::from_min_size(Pos2::new(r.min.x + 66.0 + nw + 8.0, r.center().y - 17.0), Vec2::new(kit::text_width(ui, tag, 9.5) + 14.0, 16.0));
                ui.painter().rect_filled(tr, CornerRadius::same(4), tbg);
                ui.painter().text(tr.center(), Align2::CENTER_CENTER, tag, kit::font(9.5), tfg);
                ui.painter().text(Pos2::new(r.min.x + 66.0, r.center().y + 10.0), Align2::LEFT_CENTER, desc, kit::font(12.0), MUTED);
                // state + button on the right
                let bw = kit::text_width(ui, button, 13.0) + 24.0;
                let br = Rect::from_min_size(Pos2::new(r.max.x - 14.0 - bw, r.center().y - 15.0), Vec2::new(bw, 30.0));
                let resp = ui.interact(br, ui.id().with(("perm", name)), Sense::click());
                ui.painter().rect_filled(br, CornerRadius::same(7), if resp.hovered() { RAISE } else { PANEL_2 });
                ui.painter().rect_stroke(br, CornerRadius::same(7), Stroke::new(1.0, LINE_2), StrokeKind::Inside);
                ui.painter().text(br.center(), Align2::CENTER_CENTER, button, kit::font(13.0), TEXT);
                let st = match state {
                    Some(true) => "Granted",
                    Some(false) => "Not granted",
                    None => "",
                };
                let sx = br.min.x - 12.0;
                ui.painter().text(Pos2::new(sx, r.center().y), Align2::RIGHT_CENTER, st, kit::font(12.0), MUTED);
                let dx = sx - kit::text_width(ui, st, 12.0) - 14.0;
                ui.painter().rect_filled(Rect::from_center_size(Pos2::new(dx, r.center().y), Vec2::splat(8.0)), CornerRadius::same(2), match state {
                    Some(true) => OK,
                    Some(false) => DANGER,
                    None => Color32::from_rgb(0x56, 0x62, 0x7a),
                });
                resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
            };
            if perm(ui, "monitor", "Screen Recording", true, "Lets ScreenCraft capture your display, windows and areas.", Some(p.screen), "Grant") {
                screencraft_capture::request_screen_access();
                open_pane("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture");
            }
            if perm(ui, "camera", "Camera", false, "Only needed for the camera bubble.", None, "Allow") {
                open_pane("x-apple.systempreferences:com.apple.preference.security?Privacy_Camera");
            }
            if perm(ui, "mic", "Microphone", false, "Only needed to record narration.", None, "Allow") {
                open_pane("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
            }
            if perm(ui, "annotate", "Accessibility", true, "Lets the camera find your text cursor and follow it while you type.", Some(p.accessibility), "Open Settings") {
                open_pane("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
            }
            if perm(ui, "captions", "Input Monitoring", true, "Lets ScreenCraft notice when you start typing so it can focus there.", Some(p.input_monitoring), "Allow") {
                screencraft_capture::request_input_monitoring();
                open_pane("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent");
            }
            ui.add(egui::Label::new(egui::RichText::new("A permission was just enabled. macOS only applies Screen Recording and Input Monitoring after ScreenCraft restarts — quit and open it again to finish.").size(12.5).color(Color32::from_rgb(0xcf, 0xe3, 0xf7))).wrap());
            ui.add_space(ui.available_height() - 52.0);
            ui.horizontal(|ui| {
                if kit::pill(ui, None, "Check again", false, false, false, None).clicked() {
                    rec.permissions = screencraft_capture::permissions();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ready = p.screen;
                    let r = ui.add_enabled_ui(ready, |ui| kit::primary(ui, None, "Continue")).inner;
                    if r.clicked() && ready {
                        keep = false;
                    }
                    if !p.accessibility || !p.input_monitoring {
                        if kit::pill(ui, None, "Continue without typing focus", false, false, false, None).clicked() && p.screen {
                            keep = false;
                        }
                    }
                });
            });
        });
        if ui.ctx().input(|i| i.viewport().close_requested()) {
            keep = false;
        }
    });
    keep
}
