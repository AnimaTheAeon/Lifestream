//! Start screen: the app's emblem over its name, laid over the window for a
//! moment at launch and then faded away. Slow fades and slow motion, nothing
//! flashes. Callers pass the Reduce motion setting; with it on, it is skipped.

use std::time::{Duration, Instant};

use eframe::egui::{self, pos2, vec2, Align2, Color32, Context, FontId, Painter, Rect, Sense, Shape, Stroke};

use super::theme as ff;

/// How long the screen holds before it starts to fade.
const HOLD: Duration = Duration::from_millis(2300);
/// How long the fade-out takes.
const FADE_OUT: f32 = 0.6;

pub struct Splash {
    started: Instant,
    leaving: Option<Instant>,
    done: bool,
}

impl Default for Splash {
    fn default() -> Self {
        Self { started: Instant::now(), leaving: None, done: false }
    }
}

/// The three colors the screen is drawn with, per theme.
struct Ink {
    text: Color32,
    accent: Color32,
    dim: Color32,
}

fn ink() -> Ink {
    if ff::is_blades() {
        Ink { text: Color32::from_rgb(0x2f, 0x2f, 0x2f), accent: Color32::from_rgb(0x5b, 0x5b, 0x5b), dim: Color32::from_rgb(0x6a, 0x6a, 0x6a) }
    } else {
        Ink { text: ff::TEXT(), accent: ff::LABEL(), dim: ff::TEXT_DIM() }
    }
}

fn ease(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

impl Splash {
    /// A splash that never shows (the window starts hidden in the tray).
    pub fn skipped() -> Self {
        Self { started: Instant::now(), leaving: None, done: true }
    }

    /// Paints the screen over everything else; call it last in the frame.
    /// `calm` is the Reduce motion setting.
    pub fn overlay(&mut self, ctx: &Context, calm: bool) {
        if self.done {
            return;
        }
        if calm {
            self.done = true;
            return;
        }
        if self.leaving.is_none() && self.started.elapsed() >= HOLD {
            self.leaving = Some(Instant::now());
        }
        let out = match self.leaving {
            None => 1.0,
            Some(at) => {
                let k = at.elapsed().as_secs_f32() / FADE_OUT;
                if k >= 1.0 {
                    self.done = true;
                    return;
                }
                1.0 - ease(k)
            }
        };
        let t = self.started.elapsed().as_secs_f32();
        let rect = ctx.content_rect();
        egui::Area::new(egui::Id::new("splash"))
            .order(egui::Order::Tooltip)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                // Swallow clicks and keys' pointer input while it shows.
                ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
                paint(ui.painter(), rect, t, out);
            });
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

fn paint(p: &Painter, rect: Rect, t: f32, out: f32) {
    let ink = ink();
    backdrop(p, rect, out);

    let fade_in = ease(t / 1.0);
    let sub_in = ease((t - 0.7) / 0.9);
    let k = fade_in * out;
    let cx = rect.center().x;
    let top = rect.center().y - 150.0;

    // Emblem.
    let rise = (1.0 - fade_in) * 16.0;
    emblem(p, pos2(cx, top + 70.0 + rise), t, k, &ink);

    // Title, letter-spaced.
    let font = ff::display_font(TITLE_SIZE);
    p.text(pos2(cx + 2.0, top + 186.0 + rise + 3.0), Align2::CENTER_CENTER, TITLE, font.clone(), Color32::BLACK.gamma_multiply(0.55 * k * if ff::is_blades() { 0.0 } else { 1.0 }));
    p.text(pos2(cx, top + 186.0 + rise), Align2::CENTER_CENTER, TITLE, font, ink.text.gamma_multiply(k));

    // Underline bar with a soft highlight gliding along it.
    let bar_w = 320.0 * fade_in;
    let bar = Rect::from_center_size(pos2(cx, top + 232.0), vec2(bar_w, 3.0));
    p.rect_filled(bar, egui::CornerRadius::same(1), ink.dim.gamma_multiply(0.45 * k));
    if fade_in >= 1.0 {
        let glide = ((t * 0.31).fract() * 1.6 - 0.3).clamp(0.0, 1.0);
        let seg = 80.0;
        let x0 = bar.left() + (bar_w - seg) * glide;
        let part = Rect::from_min_max(pos2(x0, bar.top()), pos2(x0 + seg, bar.bottom())).intersect(bar);
        p.rect_filled(part, egui::CornerRadius::same(1), ink.accent.gamma_multiply(0.9 * k));
    }

    // Status line.
    p.text(pos2(cx, top + 262.0), Align2::CENTER_CENTER, STATUS, FontId::proportional(20.0), ink.dim.gamma_multiply(sub_in * out));
}

/// The theme's own backdrop: navy for the FFVII menu, a green glow for Xbox,
/// brushed silver for Blades.
fn backdrop(p: &Painter, rect: Rect, out: f32) {
    let a = |c: Color32| c.gamma_multiply(out);
    if ff::is_glass() {
        ff::paint_glass_backdrop(p, rect, out);
    } else if ff::is_blades() {
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(rect.left_top(), a(Color32::from_rgb(0xcf, 0xcf, 0xcf)));
        mesh.colored_vertex(rect.right_top(), a(Color32::from_rgb(0xf2, 0xf2, 0xf2)));
        mesh.colored_vertex(rect.right_bottom(), a(Color32::from_rgb(0xc8, 0xc8, 0xc8)));
        mesh.colored_vertex(rect.left_bottom(), a(Color32::from_rgb(0xf2, 0xf2, 0xf2)));
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        p.add(Shape::mesh(mesh));
    } else if ff::is_green() {
        p.rect_filled(rect, egui::CornerRadius::ZERO, a(Color32::from_rgb(0x01, 0x0a, 0x01)));
        let c = pos2(rect.center().x, rect.top() + rect.height() * 0.42);
        ff::radial_disc(
            p,
            c,
            rect.size().length() * 0.55,
            &[
                (0.0, a(Color32::from_rgb(0x16, 0x5c, 0x10))),
                (0.38, a(Color32::from_rgb(0x0a, 0x36, 0x08))),
                (0.7, a(Color32::from_rgb(0x04, 0x1c, 0x04))),
                (1.0, a(Color32::from_rgb(0x01, 0x0a, 0x01))),
            ],
        );
    } else {
        p.rect_filled(rect, egui::CornerRadius::ZERO, a(Color32::from_rgb(0x04, 0x04, 0x0c)));
        let c = pos2(rect.center().x, rect.top() + rect.height() * 0.42);
        ff::radial_disc(
            p,
            c,
            rect.size().length() * 0.55,
            &[(0.0, a(Color32::from_rgb(0x10, 0x1a, 0x6e))), (0.45, a(Color32::from_rgb(0x08, 0x0c, 0x34))), (1.0, a(Color32::from_rgb(0x04, 0x04, 0x0c)))],
        );
    }
}


const TITLE: &str = "L I F E S T R E A M";
const TITLE_SIZE: f32 = 52.0;
const STATUS: &str = "Torrent client";

/// A slowly twisting double helix with a few motes drifting along it.
fn emblem(p: &Painter, c: egui::Pos2, t: f32, k: f32, ink: &Ink) {
    let height = 150.0;
    let amp = 34.0;
    let turns = 2.5;
    let steps = 56;
    let twist = t * 0.7;
    let at = |i: usize, strand: f32| {
        let u = i as f32 / steps as f32;
        let ph = u * turns * std::f32::consts::TAU + twist + strand * std::f32::consts::PI;
        let y = c.y - height / 2.0 + u * height;
        // Thin out toward the ends.
        let edge = (u * (1.0 - u) * 4.0).sqrt().clamp(0.0, 1.0);
        (pos2(c.x + ph.sin() * amp * edge.max(0.35), y), ph.cos())
    };

    // Rungs first, so the strands sit on top.
    for i in (0..=steps).step_by(4) {
        let (a, da) = at(i, 0.0);
        let (b, _) = at(i, 1.0);
        let depth = (da * 0.5 + 0.5).clamp(0.0, 1.0);
        p.line_segment([a, b], Stroke::new(2.0, ink.dim.gamma_multiply((0.25 + 0.45 * depth) * k)));
    }
    // Strands, shaded by depth: the near side is brighter and thicker.
    for strand in [0.0f32, 1.0] {
        for i in 0..steps {
            let (a, da) = at(i, strand);
            let (b, _) = at(i + 1, strand);
            let depth = (da * 0.5 + 0.5).clamp(0.0, 1.0);
            let col = ink.accent.gamma_multiply((0.35 + 0.65 * depth) * k);
            p.line_segment([a, b], Stroke::new(2.0 + 2.5 * depth, col));
        }
    }
    // Motes drifting down along one strand.
    for n in 0..4 {
        let u = ((t * 0.12 + n as f32 / 4.0).fract() * steps as f32) as usize;
        let (m, d) = at(u.min(steps), if n % 2 == 0 { 0.0 } else { 1.0 });
        let depth = (d * 0.5 + 0.5).clamp(0.0, 1.0);
        p.circle_filled(m, 2.5 + 2.0 * depth, ink.text.gamma_multiply((0.4 + 0.6 * depth) * k));
    }
}
