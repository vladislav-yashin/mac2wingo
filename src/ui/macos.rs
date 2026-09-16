//! macOS-flavoured controls built on top of egui primitives.
//!
//! egui already routes checkbox/radio/button drawing through
//! [`egui::Visuals`]; the helpers here add the parts that need explicit
//! painting (accent-filled checkboxes, segmented controls, alerts and cards).

use egui::epaint::RectShape;
use egui::{
    pos2, vec2, Button, Color32, CornerRadius, Frame, Margin, ProgressBar, Rect, Response,
    RichText, Sense, Shape, Stroke, StrokeKind, TextStyle, Ui, Vec2, WidgetInfo, WidgetType,
};

use crate::ui::theme;

/// Visual weight of a push button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// Accent-filled default button.
    Primary,
    /// Bordered button on the window background.
    Secondary,
    /// Red, for destructive actions.
    Destructive,
}

/// A macOS push button.
pub fn push_button(ui: &mut Ui, text: &str, kind: ButtonKind, enabled: bool) -> Response {
    let min_size = vec2(0.0, 28.0);
    let fill = match kind {
        ButtonKind::Primary => theme::accent(ui),
        ButtonKind::Destructive => ui.visuals().error_fg_color,
        ButtonKind::Secondary => {
            return ui.add_enabled(enabled, Button::new(text).min_size(min_size))
        }
    };

    let hovered = theme::lighten(fill, 0.10);
    let active = theme::darken(fill, 0.10);

    ui.scope(|ui| {
        let widgets = &mut ui.style_mut().visuals.widgets;
        widgets.inactive.weak_bg_fill = fill;
        widgets.inactive.bg_fill = fill;
        widgets.hovered.weak_bg_fill = hovered;
        widgets.hovered.bg_fill = hovered;
        widgets.active.weak_bg_fill = active;
        widgets.active.bg_fill = active;

        let button = Button::new(RichText::new(text).color(Color32::WHITE)).min_size(min_size);
        ui.add_enabled(enabled, button)
    })
    .inner
}

/// An accent-filled checkbox with a white checkmark.
pub fn checkbox(ui: &mut Ui, checked: &mut bool, text: &str) -> Response {
    let accent = theme::accent(ui);
    let icon = 15.0;
    let gap = 8.0;
    let text_color = ui.visuals().text_color();
    let font = TextStyle::Body.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, text_color);

    let height = ui.spacing().interact_size.y.max(icon);
    let width = icon + gap + galley.size().x;
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, height), Sense::click());

    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    response.widget_info(|| {
        WidgetInfo::selected(WidgetType::Checkbox, ui.is_enabled(), *checked, text)
    });

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let box_rect = Rect::from_min_size(
            pos2(rect.left(), rect.center().y - icon / 2.0),
            Vec2::splat(icon),
        );
        let (fill, stroke) = if *checked {
            (accent, Stroke::NONE)
        } else {
            (visuals.bg_fill, Stroke::new(1.0, visuals.bg_stroke.color))
        };
        ui.painter().add(RectShape::new(
            box_rect,
            CornerRadius::same(4),
            fill,
            stroke,
            StrokeKind::Inside,
        ));

        if *checked {
            let c = box_rect.center();
            let s = icon / 2.0;
            ui.painter().add(Shape::line(
                vec![
                    pos2(c.x - s * 0.48, c.y + s * 0.02),
                    pos2(c.x - s * 0.12, c.y + s * 0.38),
                    pos2(c.x + s * 0.50, c.y - s * 0.40),
                ],
                Stroke::new(1.8, Color32::WHITE),
            ));
        }

        ui.painter().galley(
            pos2(
                box_rect.right() + gap,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            text_color,
        );
    }

    response
}

/// An accent-filled radio button with a white dot.
pub fn radio_value<T: PartialEq + Clone>(
    ui: &mut Ui,
    current: &mut T,
    value: T,
    text: &str,
) -> Response {
    let accent = theme::accent(ui);
    let icon = 15.0;
    let gap = 8.0;
    let text_color = ui.visuals().text_color();
    let font = TextStyle::Body.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, text_color);

    let selected = *current == value;
    let height = ui.spacing().interact_size.y.max(icon);
    let width = icon + gap + galley.size().x;
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, height), Sense::click());

    if response.clicked() {
        *current = value;
        response.mark_changed();
    }
    response.widget_info(|| {
        WidgetInfo::selected(WidgetType::RadioButton, ui.is_enabled(), selected, text)
    });

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let center = pos2(rect.left() + icon / 2.0, rect.center().y);
        let radius = icon / 2.0;
        if selected {
            ui.painter()
                .add(Shape::circle_filled(center, radius, accent));
            ui.painter()
                .add(Shape::circle_filled(center, radius * 0.42, Color32::WHITE));
        } else {
            ui.painter()
                .add(Shape::circle_filled(center, radius, visuals.bg_fill));
            ui.painter().circle_stroke(
                center,
                radius - 0.5,
                Stroke::new(1.0, visuals.bg_stroke.color),
            );
        }

        ui.painter().galley(
            pos2(
                rect.left() + icon + gap,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            text_color,
        );
    }

    response
}

/// A horizontal segmented control. Returns `true` when the selection changed.
pub fn segmented<T: PartialEq + Clone>(
    ui: &mut Ui,
    current: &mut T,
    options: &[(T, &str)],
) -> bool {
    let accent = theme::accent(ui);
    let height = 24.0;
    let padding = 14.0;
    let font = TextStyle::Button.resolve(ui.style());
    let text_color = ui.visuals().text_color();

    let galleys: Vec<_> = options
        .iter()
        .map(|(_, label)| {
            ui.painter()
                .layout_no_wrap((*label).to_owned(), font.clone(), text_color)
        })
        .collect();
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + padding * 2.0).collect();
    let total: f32 = widths.iter().sum();

    let (rect, _) = ui.allocate_exact_size(vec2(total, height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return false;
    }

    let base = ui.visuals().widgets.inactive;
    ui.painter().add(RectShape::new(
        rect,
        CornerRadius::same(6),
        base.bg_fill,
        Stroke::new(1.0, base.bg_stroke.color),
        StrokeKind::Inside,
    ));

    let mut changed = false;
    let mut x = rect.left();
    for (index, ((option, _), galley)) in options.iter().zip(galleys).enumerate() {
        let width = widths[index];
        let segment = Rect::from_min_size(pos2(x, rect.top()), vec2(width, height));
        let selected = *current == *option;
        let response = ui.interact(
            segment,
            ui.id().with(("macos-segmented", index)),
            Sense::click(),
        );

        if selected {
            let inner = segment.shrink(2.0);
            ui.painter().add(RectShape::new(
                inner,
                CornerRadius::same(5),
                accent,
                Stroke::NONE,
                StrokeKind::Inside,
            ));
        } else if response.hovered() {
            let inner = segment.shrink(2.0);
            ui.painter().add(RectShape::new(
                inner,
                CornerRadius::same(5),
                theme::lighten(base.bg_fill, 0.04),
                Stroke::NONE,
                StrokeKind::Inside,
            ));
        }

        if response.clicked() {
            *current = option.clone();
            changed = true;
        }

        let color = if selected { Color32::WHITE } else { text_color };
        ui.painter().galley_with_override_text_color(
            pos2(
                segment.center().x - galley.size().x / 2.0,
                segment.center().y - galley.size().y / 2.0,
            ),
            galley,
            color,
        );

        x += width;
    }

    if changed {
        ui.ctx().request_repaint();
    }
    changed
}

/// A rounded card that groups related content.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let base = ui.visuals().widgets.inactive;
    Frame::new()
        .fill(base.bg_fill)
        .stroke(Stroke::new(1.0, base.bg_stroke.color))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            let width = ui.available_width();
            ui.set_min_width(width);
            ui.set_max_width(width);
            add(ui)
        })
        .inner
}

/// Severity of an [`alert`] banner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertKind {
    Info,
    Warning,
    Error,
}

/// A tinted banner used for notices and warnings.
pub fn alert(ui: &mut Ui, kind: AlertKind, text: &str) {
    let color = match kind {
        AlertKind::Info => theme::accent(ui),
        AlertKind::Warning => ui.visuals().warn_fg_color,
        AlertKind::Error => ui.visuals().error_fg_color,
    };
    let icon = match kind {
        AlertKind::Info => "ⓘ",
        AlertKind::Warning => "⚠",
        AlertKind::Error => "⊘",
    };

    Frame::new()
        .fill(tint(color, 28))
        .stroke(Stroke::new(1.0, tint(color, 96)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new(icon).color(color).strong());
                ui.label(RichText::new(text).color(color));
            });
        });
}

/// A thin, accent-filled progress bar.
pub fn progress(ui: &mut Ui, fraction: f32) -> Response {
    let accent = theme::accent(ui);
    ui.add(ProgressBar::new(fraction).fill(accent).desired_height(8.0))
}

/// A horizontal separator with breathing room.
pub fn separator(ui: &mut Ui) {
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);
}

fn tint(color: Color32, alpha: u8) -> Color32 {
    let [r, g, b, _] = color.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_render_without_panicking() {
        let mut checked = false;
        let mut choice = 0u8;
        let options = [(0u8, "GPT"), (1u8, "MBR")];
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            push_button(ui, "Start", ButtonKind::Primary, true);
            push_button(ui, "Back", ButtonKind::Secondary, true);
            checkbox(ui, &mut checked, "Create account");
            radio_value(ui, &mut choice, 1u8, "MBR");
            segmented(ui, &mut choice, &options);
            alert(ui, AlertKind::Warning, "Careful");
            progress(ui, 0.5);
            card(ui, |ui| {
                ui.label("Card");
            });
        });
        harness.run();
    }
}
