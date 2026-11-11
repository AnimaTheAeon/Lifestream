//! Green dashboard layout (ported from Jenova; see the project doc
//! claude/themes/green-dashboard.md): a home screen with a glowing jelly orb
//! and a curved column of ring nodes wired to slanted menu tabs; each entry
//! opens a full page with a slanted title bar and a framed panel.

use std::f32::consts::TAU;

use eframe::egui::{
    self, pos2, Color32, Key, LayerId, Pos2, Rect, Sense, Shape, Stroke,
    Vec2,
};

use super::theme::{self as ff, pal};
use super::{App, Page, MENU};
use crate::core::Cmd;

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Background: dark green fade, faint grid rings and spokes around `hub`.
pub fn paint_background(ctx: &egui::Context, hub: Pos2) {
    let screen = ctx.content_rect();
    let p = ctx.layer_painter(LayerId::background());
    p.rect_filled(screen, 0.0, ff::BACKDROP());
    let far = screen.width().max(screen.height()) * 1.1;
    ff::radial_disc(&p, hub, far, &[
        (0.0, Color32::from_rgb(22, 92, 16)),
        (0.25, Color32::from_rgb(10, 54, 8)),
        (0.55, Color32::from_rgb(4, 28, 4)),
        (1.0, Color32::from_rgb(1, 10, 1)),
    ]);
    let line = Stroke::new(1.4, rgba(120, 220, 60, 34));
    for k in 1..=6 {
        let rx = 230.0 * k as f32 * 0.85 + 60.0;
        p.add(Shape::ellipse_stroke(hub, Vec2::new(rx, rx * 0.92), line));
    }
    for i in 0..12 {
        let a = i as f32 / 12.0 * TAU + 0.2;
        p.line_segment([hub, hub + Vec2::new(a.cos(), a.sin()) * far], line);
    }
}

// Gentle on purpose: a slow dip into the dark background, never a bright flash.
const ZOOM_SECS: f32 = 0.6;
const REVEAL_SECS: f32 = 0.5;

/// Smooth start and end (no sudden changes in brightness).
fn ease(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

/// The dark veil pages fade through: the theme's own backdrop green.
fn veil(alpha: f32) -> Color32 {
    rgba(3, 18, 3, (alpha.clamp(0.0, 1.0) * 245.0) as u8)
}

/// Painter above everything else, for the zoom into the orb.
fn zoom_painter(ctx: &egui::Context) -> egui::Painter {
    ctx.layer_painter(LayerId::new(egui::Order::Foreground, egui::Id::new("green_zoom")))
}

/// The big animated orb with an emblem in it.
fn paint_big_orb(painter: &egui::Painter, c: Pos2, r: f32, t: f32) {
    // Outer haze and shells.
    painter.add(Shape::circle_stroke(c, r * 1.22, Stroke::new(2.0, rgba(120, 220, 50, 80))));
    let spin = t * 0.35;
    let arc: Vec<Pos2> = (0..=40)
        .map(|i| {
            let a = spin + i as f32 / 40.0 * 1.6;
            c + Vec2::new(a.cos(), a.sin()) * r * 1.12
        })
        .collect();
    painter.add(Shape::line(arc, Stroke::new(r * 0.07, rgba(140, 240, 60, 70))));
    let w = (t * 0.7).sin();
    let w2 = (t * 0.55 + 1.3).sin();
    painter.add(Shape::ellipse_filled(
        c + Vec2::new(w * 4.0, w2 * 3.0),
        Vec2::new(r * (1.04 + 0.03 * w), r * (0.98 - 0.03 * w)),
        rgba(60, 170, 24, 70),
    ));
    painter.add(Shape::ellipse_stroke(
        c + Vec2::new(w * 4.0, w2 * 3.0),
        Vec2::new(r * (1.04 + 0.03 * w), r * (0.98 - 0.03 * w)),
        Stroke::new(3.0, rgba(160, 255, 80, 110)),
    ));
    ff::paint_orb(painter, c, r * 0.84, true, t * 0.15);
    // Translucent jelly lobes drifting over the core.
    for (i, (ox, oy, sx, sy)) in [(-0.5, 0.25, 0.42, 0.3), (0.55, -0.1, 0.32, 0.45), (0.1, 0.62, 0.5, 0.22)]
        .iter()
        .enumerate()
    {
        let ph = t * (0.5 + i as f32 * 0.17) + i as f32;
        let center = c + Vec2::new(*ox + 0.05 * ph.sin(), *oy + 0.05 * ph.cos()) * r;
        painter.add(Shape::ellipse_filled(center, Vec2::new(sx * r, sy * r), rgba(140, 240, 60, 45)));
        painter.add(Shape::ellipse_stroke(center, Vec2::new(sx * r, sy * r), Stroke::new(1.5, rgba(200, 255, 120, 70))));
    }
    // Emblem: a slowly twisting double helix (the Lifestream), in pale light.
    // Strands dim and thin as they turn away; motes of light ride along them.
    let h = r * 0.44; // half height
    let amp = r * 0.24;
    let turn = std::f32::consts::TAU / (h * 1.3);
    let phase = t * 0.7;
    let at = |y: f32, strand: f32| {
        let a = (y + h) * turn + phase + strand * std::f32::consts::PI;
        (c.x + amp * a.sin(), a.cos())
    };
    let ink = |depth: f32, alpha: f32| rgba(255, 255, 230, (alpha * (0.45 + 0.55 * (depth + 1.0) / 2.0) * 255.0) as u8);
    // Rungs between the strands.
    let mut y = -h + h * 0.08;
    while y < h * 0.96 {
        let (x0, _) = at(y, 0.0);
        let (x1, _) = at(y, 1.0);
        painter.line_segment([pos2(x0, c.y + y), pos2(x1, c.y + y)], Stroke::new((r * 0.025).max(1.5), rgba(255, 255, 230, 85)));
        y += h * 0.16;
    }
    // Strands, in short pieces so each can take its own depth.
    let step = h / 28.0;
    for strand in [0.0, 1.0] {
        let mut y = -h;
        while y < h {
            let (x0, d0) = at(y, strand);
            let (x1, d1) = at(y + step, strand);
            let d = (d0 + d1) / 2.0;
            let w = r * (0.03 + 0.028 * (d + 1.0) / 2.0);
            painter.line_segment([pos2(x0, c.y + y), pos2(x1, c.y + y + step)], Stroke::new(w.max(1.5), ink(d, 0.95)));
            y += step;
        }
    }
    // Motes drifting down the strands.
    for i in 0..5 {
        let f = ((t * 0.12 + i as f32 * 0.2) % 1.0) * 2.0 - 1.0;
        let y = f * h;
        let (x, d) = at(y, (i % 2) as f32);
        painter.circle_filled(pos2(x, c.y + y), (r * 0.03).max(1.5) * (0.8 + 0.4 * (d + 1.0) / 2.0), ink(d, 1.0));
    }
}

/// Ring node beside each tab.
fn paint_node(painter: &egui::Painter, c: Pos2, r: f32, lit: bool) {
    let glow = if lit { 140 } else { 70 };
    ff::radial_disc(painter, c, r * 1.45, &[(0.0, rgba(150, 255, 60, glow)), (0.7, rgba(150, 255, 60, glow / 3)), (1.0, Color32::TRANSPARENT)]);
    ff::radial_disc(painter, c, r, &[
        (0.0, Color32::from_rgb(4, 20, 3)),
        (0.5, Color32::from_rgb(10, 40, 6)),
        (0.78, Color32::from_rgb(44, 138, 18)),
        (0.88, if lit { Color32::from_rgb(220, 255, 110) } else { Color32::from_rgb(141, 232, 58) }),
        (1.0, Color32::from_rgb(28, 90, 10)),
    ]);
    painter.circle_stroke(c, r * 0.36, Stroke::new(1.5, rgba(160, 255, 80, 90)));
}

/// Slanted tab with its label; `lit` = selected (yellow-green fill).
pub fn paint_tab(painter: &egui::Painter, rect: Rect, label: &str, lit: bool, size: f32) {
    let outer = ff::chamfer_points(rect, 16.0);
    let inner = ff::chamfer_points(rect.shrink(2.0), 15.0);
    painter.add(Shape::convex_polygon(outer, if lit { Color32::from_rgb(242, 255, 154) } else { rgba(150, 240, 70, 150) }, Stroke::NONE));
    let (top, bottom) = if lit {
        (Color32::from_rgb(226, 255, 106), Color32::from_rgb(140, 200, 28))
    } else {
        (Color32::from_rgb(36, 112, 14), Color32::from_rgb(12, 52, 6))
    };
    // Vertical gradient inside the clipped outline.
    let mut mesh = egui::Mesh::default();
    for p in &inner {
        let v = ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * v) as u8;
        mesh.colored_vertex(*p, Color32::from_rgb(mix(top.r(), bottom.r()), mix(top.g(), bottom.g()), mix(top.b(), bottom.b())));
    }
    for i in 1..inner.len() as u32 - 1 {
        mesh.add_triangle(0, i, i + 1);
    }
    painter.add(Shape::mesh(mesh));
    let font = ff::display_font(size);
    let pos = pos2(rect.left() + 26.0, rect.center().y);
    let text = label.to_uppercase();
    if lit {
        painter.text(pos, egui::Align2::LEFT_CENTER, &text, font, pal().on_accent);
    } else {
        painter.text(pos + Vec2::new(0.0, 1.5), egui::Align2::LEFT_CENTER, &text, font.clone(), rgba(0, 0, 0, 160));
        painter.text(pos, egui::Align2::LEFT_CENTER, &text, font, Color32::from_rgb(190, 242, 126));
    }
}

/// A small chamfered tab for the page bar along the bottom, label centered.
fn paint_nav_tab(painter: &egui::Painter, rect: Rect, label: &str, lit: bool, hover: bool) {
    let cut = 9.0;
    let edge = if lit {
        Color32::from_rgb(242, 255, 154)
    } else if hover {
        Color32::from_rgb(200, 255, 110)
    } else {
        rgba(150, 240, 70, 140)
    };
    painter.add(Shape::convex_polygon(ff::chamfer_points(rect, cut), edge, Stroke::NONE));
    let inner = ff::chamfer_points(rect.shrink(1.5), cut - 0.5);
    let (top, bottom) = if lit {
        (Color32::from_rgb(226, 255, 106), Color32::from_rgb(140, 200, 28))
    } else if hover {
        (Color32::from_rgb(52, 140, 20), Color32::from_rgb(18, 70, 8))
    } else {
        (Color32::from_rgb(30, 96, 12), Color32::from_rgb(10, 44, 5))
    };
    let mut mesh = egui::Mesh::default();
    for p in &inner {
        let v = ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * v) as u8;
        mesh.colored_vertex(*p, Color32::from_rgb(mix(top.r(), bottom.r()), mix(top.g(), bottom.g()), mix(top.b(), bottom.b())));
    }
    for i in 1..inner.len() as u32 - 1 {
        mesh.add_triangle(0, i, i + 1);
    }
    painter.add(Shape::mesh(mesh));
    let text = label.to_uppercase();
    let font = ff::display_font(12.0);
    if lit {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, &text, font, pal().on_accent);
    } else {
        painter.text(rect.center() + Vec2::new(0.0, 1.2), egui::Align2::CENTER_CENTER, &text, font.clone(), rgba(0, 0, 0, 160));
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, &text, font, Color32::from_rgb(190, 242, 126));
    }
}

/// A round hint button like "ENTER  SELECT". Returns its response.
fn hint(ui: &mut egui::Ui, at: Pos2, key: &str, label: &str) -> egui::Response {
    let r = 24.0;
    let c = at + Vec2::new(r, 0.0);
    let rect = Rect::from_center_size(c + Vec2::new(55.0, 0.0), Vec2::new(r * 2.0 + 120.0, r * 2.0 + 4.0));
    let resp = ui.interact(rect, ui.id().with(("hint", key)), Sense::click());
    let p = ui.painter();
    paint_node(p, c, r, resp.hovered());
    p.text(c, egui::Align2::CENTER_CENTER, key, ff::display_font(10.0), Color32::from_rgb(216, 255, 154));
    p.text(c + Vec2::new(r + 12.0, 0.0), egui::Align2::LEFT_CENTER, label, ff::display_font(17.0), Color32::from_rgb(190, 242, 126));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Bordered status box with a small orb.
fn status_box(ui: &mut egui::Ui, rect: Rect, top: &str, bottom: &str, badge: ff::Badge) {
    let p = ui.painter();
    p.rect_filled(rect, 0.0, rgba(0, 10, 0, 200));
    p.rect_stroke(rect, 0.0, Stroke::new(1.0, rgba(170, 255, 90, 140)), egui::StrokeKind::Inside);
    // The VPN protection badge sits where the orb light would be.
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_center_size(pos2(rect.left() + 26.0, rect.center().y), Vec2::splat(26.0))));
    ff::badge(&mut child, badge, 24.0);
    let p = ui.painter();
    let x = rect.left() + 50.0;
    let room = rect.right() - x - 10.0;
    // Shorten with an ellipsis rather than cutting text off mid-letter.
    let fit = |text: &str, size: f32| -> String {
        let text = text.to_uppercase();
        let per_char = size * 0.78;
        let max = (room / per_char).floor() as usize;
        if text.chars().count() <= max {
            text
        } else {
            let cut: String = text.chars().take(max.saturating_sub(1)).collect();
            format!("{}…", cut.trim_end())
        }
    };
    p.text(pos2(x, rect.center().y - 9.0), egui::Align2::LEFT_CENTER, fit(top, 10.5), ff::display_font(10.5), Color32::from_rgb(168, 224, 112));
    p.text(pos2(x, rect.center().y + 10.0), egui::Align2::LEFT_CENTER, fit(bottom, 12.5), ff::display_font(12.5), Color32::from_rgb(240, 255, 216));
}


impl App {
    fn green_menu(&self) -> Vec<(Page, &'static str)> {
        MENU.iter().filter(|m| self.settings.advanced_mode || !m.2).map(|m| (m.0, m.1)).collect()
    }

    /// Back to the dashboard, with the cursor on the page just left.
    fn green_back(&mut self) {
        if self.settings.reduce_motion {
            self.green_home_now();
            self.green_unzoom = None;
            return;
        }
        // The page first sinks into the orb's glow (see green_layout).
        if self.green_leave.is_none() {
            self.green_reveal = None;
            self.green_leave = Some(std::time::Instant::now());
        }
    }

    /// Actually back on the dashboard; the orb shrinks back into place.
    fn green_home_now(&mut self) {
        self.green_in_page = false;
        self.green_unzoom = Some(std::time::Instant::now());
        if let Some(i) = self.green_menu().iter().position(|m| m.0 == self.page) {
            self.green_cursor = i;
        }
    }

    /// Opens a page from home: first the camera zooms into the orb.
    fn green_open(&mut self, page: Page) {
        if self.settings.reduce_motion {
            self.menu_touched = true;
            self.page = page;
            self.green_in_page = true;
            return;
        }
        if self.green_zoom.is_none() {
            self.green_zoom = Some((page, std::time::Instant::now()));
        }
    }

    /// (top line, bottom line, badge) for the status box.
    fn green_status(&self) -> (String, String, ff::Badge) {
        let (badge, prot, _) = self.protection();
        let held = self.snap.kill_hold || self.snap.user_paused;
        let (dl, ul) = if held { (0, 0) } else { (self.snap.dl_rate, self.snap.ul_rate) };
        (
            format!("VPN {} · Down {} · Up {}", prot, crate::core::human_rate(dl), crate::core::human_rate(ul)),
            self.status_line(),
            badge,
        )
    }

    pub(crate) fn green_layout(&mut self, ui: &mut egui::Ui) {
        let full = ui.max_rect();
        let ctx = ui.ctx().clone();
        let t = ctx.input(|i| i.time) as f32;
        let main;
        // Zoom finished: open the page this frame (no flash of the menu in between).
        if let Some((page, at)) = self.green_zoom {
            if at.elapsed().as_secs_f32() >= ZOOM_SECS {
                self.green_zoom = None;
                self.menu_touched = true;
                self.page = page;
                self.green_in_page = true;
                self.green_reveal = Some(std::time::Instant::now());
            }
        }
        if let Some(at) = self.green_leave {
            if at.elapsed().as_secs_f32() >= REVEAL_SECS {
                self.green_leave = None;
                self.green_home_now();
            }
        }
        if self.green_in_page {
            paint_background(&ctx, pos2(full.left() + full.width() * 0.2, full.top() + full.height() * 0.15));
            main = self.green_page(ui, full);
            // The page comes out of the orb's glow.
            if let Some(at) = self.green_reveal {
                let k = at.elapsed().as_secs_f32() / REVEAL_SECS;
                if k >= 1.0 {
                    self.green_reveal = None;
                } else {
                    zoom_painter(&ctx).rect_filled(full.expand(20.0), 0.0, veil(1.0 - ease(k)));
                    ctx.request_repaint();
                }
            }
            // Going back: the page sinks into the glow (the reveal, reversed).
            if let Some(at) = self.green_leave {
                let k = (at.elapsed().as_secs_f32() / REVEAL_SECS).min(1.0);
                zoom_painter(&ctx).rect_filled(full.expand(20.0), 0.0, veil(ease(k)));
                ctx.request_repaint();
            }
        } else {
            let r = (full.height() * 0.24).min(full.width() * 0.16).max(90.0);
            let orb_c = pos2(full.left() + full.width() * 0.3, full.center().y - 24.0);
            paint_background(&ctx, orb_c);
            self.green_home(ui, full, orb_c, r, t);
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
            // Zooming into the orb before a page opens.
            // Zooming into the orb before a page opens, or (reversed) back out of it.
            let zoom_k = if let Some((_, at)) = self.green_zoom {
                Some((at.elapsed().as_secs_f32() / ZOOM_SECS).min(1.0))
            } else if let Some(at) = self.green_unzoom {
                let k = 1.0 - at.elapsed().as_secs_f32() / ZOOM_SECS;
                if k <= 0.0 {
                    self.green_unzoom = None;
                    None
                } else {
                    Some(k)
                }
            } else {
                None
            };
            if let Some(k) = zoom_k {
                {
                    // The orb drifts a little closer while everything eases into the
                    // dark backdrop: a soft dissolve, no bright frames.
                    let e = ease(k);
                    let p = zoom_painter(&ctx);
                    paint_big_orb(&p, orb_c, r * (1.0 + 0.6 * e), t);
                    p.rect_filled(full.expand(20.0), 0.0, veil(e));
                }
                ctx.request_repaint();
            }
            main = full;
        }
        self.update_prompt(&ctx);
        self.files_popup(&ctx, main);
        self.remove_dialog(&ctx);
        self.toasts(&ctx, pos2(full.right() - 14.0, full.top() + 80.0));
    }

    fn green_home(&mut self, ui: &mut egui::Ui, full: Rect, orb_c: Pos2, r: f32, t: f32) {
        paint_big_orb(ui.painter(), orb_c, r, t);
        let menu = self.green_menu();
        let n = menu.len();
        self.green_cursor = self.green_cursor.min(n - 1);
        let row_h = ((full.height() - 160.0) / n as f32).clamp(44.0, 66.0);
        let node_r = (row_h * 0.42).min(28.0);
        let arc_r = (r * 1.5).max(n as f32 * row_h * 0.62);
        let tab_w = (full.right() - orb_c.x - arc_r - 140.0).clamp(220.0, 460.0);

        let typing = ui.ctx().egui_wants_keyboard_input();
        let (up, down, enter) = ui.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Enter)));
        if !typing {
            if up {
                self.green_cursor = (self.green_cursor + n - 1) % n;
            }
            if down {
                self.green_cursor = (self.green_cursor + 1) % n;
            }
            if enter {
                self.green_open(menu[self.green_cursor].0);
            }
        }

        let mut open = None;
        for (i, (page, label)) in menu.iter().enumerate() {
            let dy = (i as f32 - (n as f32 - 1.0) / 2.0) * row_h;
            let dx = (arc_r * arc_r - dy * dy).max(0.0).sqrt();
            let node_c = pos2(orb_c.x + dx, orb_c.y + dy);
            let tab = Rect::from_min_size(pos2(node_c.x + node_r + 26.0, node_c.y - row_h * 0.38), Vec2::new(tab_w, row_h * 0.76));
            let hit = Rect::from_min_max(pos2(node_c.x - node_r, tab.top()), tab.right_bottom());
            let resp = ui.interact(hit, ui.id().with(("green_menu", i)), Sense::click());
            if resp.hovered() {
                self.green_cursor = i;
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                open = Some(*page);
            }
            let lit = self.green_cursor == i;
            let p = ui.painter();
            let wire = Stroke::new(2.0, rgba(150, 255, 70, if lit { 230 } else { 140 }));
            let a = node_c + Vec2::new(node_r, 0.0);
            let b = pos2(tab.left() - 6.0, node_c.y);
            p.line_segment([a, b], wire);
            p.line_segment([pos2(a.x + 8.0, a.y), pos2(a.x + 14.0, a.y - row_h * 0.3)], Stroke::new(1.0, rgba(150, 255, 70, 90)));
            paint_node(p, node_c, node_r, lit);
            paint_tab(p, tab, label, lit, (row_h * 0.42).min(25.0));
        }
        if let Some(page) = open {
            self.green_open(page);
        }

        let (top, bottom, badge) = self.green_status();
        let box_rect = Rect::from_min_size(pos2(full.left() + 6.0, full.bottom() - 64.0), Vec2::new((full.width() * 0.5).min(600.0), 54.0));
        status_box(ui, box_rect, &top, &bottom, badge);
        if hint(ui, pos2(full.right() - 210.0, full.bottom() - 36.0), "ENTER", "SELECT").clicked() {
            self.green_open(menu[self.green_cursor].0);
        }
    }

    /// Returns the content panel's rect (pop-ups anchor to it).
    fn green_page(&mut self, ui: &mut egui::Ui, full: Rect) -> Rect {
        // Esc (or the mouse's side "back" button) backs out one level:
        // a confirm dialog, then the files pop-up, then the page itself.
        // While typing, Esc just leaves the text box (egui handles that).
        let typing = ui.ctx().egui_wants_keyboard_input();
        let esc = !typing && ui.input(|i| i.key_pressed(Key::Escape));
        let mouse_back = ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra1));
        if esc || mouse_back {
            if self.confirm_remove.is_some() {
                self.confirm_remove = None;
            } else if self.popup.is_some() {
                self.popup = None;
                self.core.send(Cmd::WatchFiles(None));
            } else {
                self.green_back();
            }
        }
        if !self.settings.advanced_mode && self.page == Page::Log {
            self.page = Page::Torrents;
        }
        let label = MENU.iter().find(|m| m.0 == self.page).map(|m| m.1).unwrap_or("Torrents");

        let bar = Rect::from_min_size(full.min, Vec2::new((full.width() * 0.42).clamp(300.0, 520.0), 56.0));
        let p = ui.painter();
        let slant = 22.0;
        p.add(Shape::convex_polygon(
            vec![pos2(bar.left() + slant, bar.top()), bar.right_top(), pos2(bar.right() - slant, bar.bottom()), bar.left_bottom()],
            pal().accent,
            Stroke::NONE,
        ));
        p.text(pos2(bar.left() + 34.0, bar.center().y), egui::Align2::LEFT_CENTER, label.to_uppercase(), ff::display_font(28.0), pal().on_accent);

        let (top, bottom, badge) = self.green_status();
        let sb_w = (full.width() - bar.width() - 30.0).clamp(0.0, 520.0);
        if sb_w > 200.0 {
            let r = Rect::from_min_size(pos2(full.right() - sb_w, full.top() + 2.0), Vec2::new(sb_w, 52.0));
            status_box(ui, r, &top, &bottom, badge);
            // The badge box doubles as a shortcut to the Network page.
            if ui.interact(r, ui.id().with("green_status"), Sense::click()).on_hover_text("Open Network").clicked() {
                self.page = Page::Network;
            }
        }

        let frame = Rect::from_min_max(pos2(full.left(), bar.bottom() + 18.0), pos2(full.right(), full.bottom() - 64.0));
        ff::set_hide_headings(true);
        ff::window_at(ui, frame, "green_main", |ui| self.render_page_body(ui));
        ff::set_hide_headings(false);

        if hint(ui, pos2(full.right() - 190.0, full.bottom() - 30.0), "ESC", "BACK").clicked() {
            self.green_back();
        }
        self.green_page_bar(ui, Rect::from_min_max(pos2(full.left(), full.bottom() - 46.0), pos2(full.right() - 210.0, full.bottom() - 14.0)));
        frame
    }

    /// Small tabs along the bottom of a page: jump straight to another page
    /// without going back to the dashboard.
    fn green_page_bar(&mut self, ui: &mut egui::Ui, area: Rect) {
        let menu = self.green_menu();
        if menu.is_empty() || area.width() < 200.0 {
            return;
        }
        let gap = 6.0;
        let w = ((area.width() - gap * (menu.len() as f32 - 1.0)) / menu.len() as f32).min(118.0);
        if w < 60.0 {
            return;
        }
        let busy = self.green_leave.is_some() || self.green_zoom.is_some();
        for (i, (page, label)) in menu.iter().enumerate() {
            let rect = Rect::from_min_size(pos2(area.left() + i as f32 * (w + gap), area.top()), Vec2::new(w, area.height()));
            let resp = ui.interact(rect, ui.id().with(("page_bar", i)), Sense::click());
            let lit = *page == self.page;
            paint_nav_tab(ui.painter(), rect, label, lit, resp.hovered());
            if resp.hovered() && !lit {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() && !lit && !busy {
                // Close anything open on the old page, then flash into the new one.
                self.popup = None;
                self.confirm_remove = None;
                self.core.send(Cmd::WatchFiles(None));
                self.page = *page;
                self.menu_touched = true;
                self.green_cursor = i;
                if !self.settings.reduce_motion {
                    self.green_reveal = Some(std::time::Instant::now());
                }
            }
        }
    }
}
