//! Small reusable UI building blocks.

use egui::{Color32, CornerRadius, Frame, Margin, RichText, Stroke, Ui};

use crate::ui::theme;

/// A muted label followed by a value on the same row.
///
/// The label column adapts to the available width and the value is truncated
/// with a tooltip when it does not fit, so long paths never blow up the layout.
pub fn key_value(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        let label_width = (ui.available_width() * 0.36).clamp(110.0, 170.0);
        ui.add_sized(
            [label_width, 18.0],
            egui::Label::new(RichText::new(label).weak()).truncate(),
        );

        let font = egui::TextStyle::Body.resolve(ui.style());
        let color = ui.visuals().text_color();
        let full = ui.painter().layout_no_wrap(value.to_owned(), font, color);
        let clipped = full.size().x > ui.available_width();

        let response = ui.add(egui::Label::new(value).truncate());
        if clipped {
            let _ = response.on_hover_text(value);
        }
    });
}

/// A prominent section heading.
pub fn section_heading(ui: &mut Ui, text: &str) {
    ui.add_space(2.0);
    ui.heading(text);
    ui.add_space(6.0);
}

/// A short inline separator with spacing.
pub fn spaced_separator(ui: &mut Ui) {
    crate::ui::macos::separator(ui);
}

/// A colored informational notice.
pub fn notice(ui: &mut Ui, text: &str, color: Color32) {
    let [r, g, b, _] = color.to_array();
    Frame::new()
        .fill(Color32::from_rgba_unmultiplied(r, g, b, 28))
        .stroke(Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(r, g, b, 96),
        ))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(text).color(color));
            });
        });
}

/// A small, muted hint placed under a control.
pub fn hint(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).weak().small()).wrap());
}

/// A bullet list item.
pub fn bullet(ui: &mut Ui, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new("•").weak());
        ui.label(text);
    });
}

/// The application accent colour for the current style.
#[must_use]
pub fn accent(ui: &Ui) -> Color32 {
    theme::accent(ui)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widgets_render_without_panicking() {
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            key_value(ui, "Name", "SanDisk");
            key_value(
                ui,
                "Selected image",
                "/Volumes/Work/very/long/path/to/windows/Win11_23H2_English_x64.iso",
            );
            section_heading(ui, "Section");
            spaced_separator(ui);
            notice(ui, "Notice", Color32::RED);
            hint(ui, "Hint");
            bullet(ui, "Item");
        });
        harness.run();
    }
}
