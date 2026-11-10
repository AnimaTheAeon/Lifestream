//! Setting rows with built-in help text and tooltips.

use eframe::egui::{self, Align, Color32, Layout, RichText, Ui};

use super::theme::{self as ff, BAD, GOOD, LABEL, TEXT, TEXT_DIM, WARN};

/// Page heading plus a one-line description.
pub fn page_title(ui: &mut Ui, title: &str, subtitle: &str) {
    ff::title(ui, title, 26.0, TEXT);
    ui.label(RichText::new(subtitle).color(TEXT_DIM));
    ff::divider(ui);
}

/// Gold section heading.
pub fn section(ui: &mut Ui, text: &str) {
    ui.add_space(8.0);
    ff::title(ui, text, 17.0, WARN);
    ui.add_space(2.0);
}

/// One setting: title and help on the left, control on the right. The help
/// text also shows as a tooltip anywhere on the row.
pub fn row<R>(ui: &mut Ui, title: &str, help: &str, control: impl FnOnce(&mut Ui) -> R) -> R {
    row_tagged(ui, title, None, help, control)
}

/// Like [`row`], with a small tag after the title (e.g. "ON BY DEFAULT").
pub fn row_tagged<R>(
    ui: &mut Ui,
    title: &str,
    tag: Option<(&str, Color32)>,
    help: &str,
    control: impl FnOnce(&mut Ui) -> R,
) -> R {
    let mut out = None;
    let resp = ui
        .horizontal(|ui| {
            let right_w = 260.0_f32.min(ui.available_width() * 0.45);
            let left_w = (ui.available_width() - right_w - 12.0).max(120.0);
            ui.allocate_ui_with_layout(egui::vec2(left_w, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(left_w);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).color(TEXT).strong().size(15.0));
                    if let Some((t, c)) = tag {
                        ff::tag(ui, t, c);
                    }
                });
                if !help.is_empty() {
                    ui.add(egui::Label::new(RichText::new(help).color(TEXT_DIM).size(12.5)).wrap());
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                out = Some(control(ui));
            });
        })
        .response;
    if !help.is_empty() {
        resp.on_hover_text(help);
    }
    let rect = ui.available_rect_before_wrap();
    ui.painter().line_segment(
        [egui::pos2(rect.left(), rect.top() + 2.0), egui::pos2(rect.right(), rect.top() + 2.0)],
        egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(150, 160, 220, 40)),
    );
    ui.add_space(5.0);
    out.unwrap()
}

/// ON/OFF button. Returns true when clicked (value already flipped).
pub fn toggle(ui: &mut Ui, value: &mut bool) -> bool {
    toggle_labeled(ui, value, "ON", "OFF")
}

pub fn toggle_labeled(ui: &mut Ui, value: &mut bool, on: &str, off: &str) -> bool {
    let (label, color) = if *value { (on, GOOD) } else { (off, BAD) };
    let clicked = ui
        .add(egui::Button::new(RichText::new(label).color(color).strong()).min_size(egui::vec2(64.0, 26.0)))
        .clicked();
    if clicked {
        *value = !*value;
    }
    clicked
}

/// A labeled field stacked above its input, for grids of numbers.
pub fn field<R>(ui: &mut Ui, label: &str, help: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut out = None;
    let r = ui
        .vertical(|ui| {
            ff::title(ui, label, 13.0, LABEL);
            out = Some(add(ui));
        })
        .response;
    if !help.is_empty() {
        r.on_hover_text(help);
    }
    out.unwrap()
}

/// Number box in KB/s where 0 means unlimited.
pub fn rate(ui: &mut Ui, v: &mut i32) -> egui::Response {
    let unlimited = *v <= 0;
    let r = ui.add(
        egui::DragValue::new(v)
            .range(0..=1_000_000)
            .speed(10.0)
            .custom_formatter(|n, _| if n <= 0.0 { "Unlimited".into() } else { format!("{n:.0} KB/s") }),
    );
    if unlimited {
        r.on_hover_text("0 = no limit")
    } else {
        r
    }
}

pub fn folder_field(ui: &mut Ui, value: &mut String, width: f32) -> bool {
    let mut changed = false;
    if ui.button("Browse").clicked() {
        if let Some(p) = rfd::FileDialog::new().set_directory(value.as_str()).pick_folder() {
            *value = p.display().to_string();
            changed = true;
        }
    }
    changed |= ui.add(egui::TextEdit::singleline(value).desired_width(width)).changed();
    changed
}

/// Filter chips at the top of the torrent list.
pub fn pill(ui: &mut Ui, text: &str, selected: bool) -> egui::Response {
    let color = if selected { Color32::WHITE } else { Color32::from_rgb(214, 218, 236) };
    let fill = if selected { Color32::from_rgb(60, 100, 200) } else { Color32::from_rgb(12, 20, 72) };
    ui.add(
        egui::Button::new(RichText::new(text).color(color).strong())
            .fill(fill)
            .corner_radius(12.0)
            .min_size(egui::vec2(0.0, 26.0)),
    )
}
