//! Top-level layout: unified toolbar, progress indicator and navigation footer.

use eframe::App;
use egui::{pos2, vec2, Align, Frame, Key, Layout, Margin, RichText, Sense, Stroke, Ui};

use crate::app::{Mac2WinApp, Step};
use crate::i18n::Language;
use crate::ui::{macos, theme};

impl App for Mac2WinApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let zoom = ui.ctx().zoom_factor();
        let inset = self.chrome_inset(frame, zoom);
        // Unified toolbar: the row shares the traffic-light band and is centred
        // both against the buttons and against the panel edges.
        let panel_height = theme::header_panel_height(&inset);
        let top = ((panel_height - theme::TOOLBAR_ROW_HEIGHT) / 2.0).round();
        let margin = Margin {
            left: inset.left.round() as i8,
            right: 18,
            top: top as i8,
            bottom: (panel_height - theme::TOOLBAR_ROW_HEIGHT - top).round() as i8,
        };

        egui::Panel::top("mac2wingo_header")
            .resizable(false)
            .exact_size(panel_height)
            .frame(
                Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(margin),
            )
            .show(ui, |ui| {
                header_drag_region(ui, panel_height);
                toolbar(self, ui);
            });

        egui::Panel::bottom("mac2wingo_footer")
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(Margin::symmetric(18, 10)),
            )
            .show(ui, |ui| footer(self, ui));

        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(Margin {
                        left: 18,
                        right: 18,
                        top: 12,
                        bottom: 4,
                    }),
            )
            .show(ui, |ui| {
                if self.step() != Step::Failed {
                    if let Some(error) = self.error() {
                        macos::alert(ui, macos::AlertKind::Error, error);
                        ui.add_space(8.0);
                    }
                }

                if self.step() != Step::Done && self.step() != Step::Failed {
                    step_indicator(ui, self.step().progress_index());
                    ui.add_space(4.0);
                }

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| crate::ui::steps::show(self, ui));
            });
    }
}

/// Lets the window be dragged by the empty header background: a transparent
/// titlebar no longer provides a native drag area. Registered before the
/// toolbar so the widgets on top of it keep priority.
fn header_drag_region(ui: &mut Ui, panel_height: f32) {
    let viewport = ui.ctx().viewport_rect();
    let rect = egui::Rect::from_min_max(
        pos2(viewport.left(), viewport.top()),
        pos2(viewport.right(), viewport.top() + panel_height),
    );
    let response = ui.interact(rect, ui.id().with("header_drag"), Sense::click_and_drag());
    if response.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

fn toolbar(app: &mut Mac2WinApp, ui: &mut Ui) {
    let t = *app.i18n();
    // Compact widgets so the row fits the native titlebar band.
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = theme::TOOLBAR_ROW_HEIGHT;
        ui.spacing_mut().button_padding.y = 2.0;
        ui.horizontal_centered(|ui| {
            ui.add(egui::Label::new(RichText::new(t.t("app.title")).heading()).selectable(false));
            ui.add_space(10.0);
            let subtitle = ui.add(
                egui::Label::new(RichText::new(t.t("app.subtitle")).weak().small())
                    .truncate()
                    .selectable(false),
            );
            let _ = subtitle.on_hover_text(t.t("app.subtitle"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                language_selector(app, ui);
            });
        });
    });
}

fn language_selector(app: &mut Mac2WinApp, ui: &mut Ui) {
    let current = app.i18n().lang();
    let mut selected = current;

    egui::ComboBox::from_id_salt("language_selector")
        .selected_text(current.native_name())
        .width(130.0)
        .show_ui(ui, |ui| {
            for language in Language::ALL {
                ui.selectable_value(&mut selected, language, language.native_name());
            }
        });

    if selected != current {
        app.set_language(selected);
    }
}

/// A centered row of dots showing progress through the wizard.
fn step_indicator(ui: &mut Ui, current: usize) {
    let steps = Step::WIZARD.len();
    if steps == 0 {
        return;
    }

    let spacing = 22.0;
    let total = (steps - 1) as f32 * spacing;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let accent = theme::accent(ui);
    let inactive = ui.visuals().widgets.inactive.bg_stroke.color;
    let y = rect.center().y;
    let start_x = rect.center().x - total / 2.0;

    if total > 0.0 {
        ui.painter().line_segment(
            [pos2(start_x, y), pos2(start_x + total, y)],
            Stroke::new(1.0, inactive),
        );
    }

    for index in 0..steps {
        let x = start_x + index as f32 * spacing;
        let color = if index <= current { accent } else { inactive };
        ui.painter().circle_filled(pos2(x, y), 4.0, color);
        if index == current {
            ui.painter()
                .circle_stroke(pos2(x, y), 6.5, Stroke::new(1.5, accent));
        }
    }
}

fn footer(app: &mut Mac2WinApp, ui: &mut Ui) {
    let t = *app.i18n();
    ui.horizontal(|ui| match app.step() {
        Step::Welcome | Step::Image | Step::Device | Step::Options => {
            if app.step().previous().is_some()
                && macos::push_button(ui, t.t("nav.back"), macos::ButtonKind::Secondary, true)
                    .clicked()
            {
                app.go_back();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                primary_footer_button(app, ui, t.t("nav.next"));
            });
        }
        Step::Confirm => {
            if macos::push_button(ui, t.t("nav.back"), macos::ButtonKind::Secondary, true).clicked()
            {
                app.go_back();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                primary_footer_button(app, ui, t.t("nav.start"));
            });
        }
        Step::Writing => {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if macos::push_button(ui, t.t("nav.cancel"), macos::ButtonKind::Secondary, true)
                    .clicked()
                {
                    app.cancel_write();
                }
            });
        }
        Step::Done => {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if macos::push_button(ui, t.t("nav.close"), macos::ButtonKind::Primary, true)
                    .clicked()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if macos::push_button(
                    ui,
                    t.t("step.done.again"),
                    macos::ButtonKind::Secondary,
                    true,
                )
                .clicked()
                {
                    app.reset();
                }
            });
        }
        Step::Failed => {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if macos::push_button(ui, t.t("nav.retry"), macos::ButtonKind::Primary, true)
                    .clicked()
                {
                    app.retry();
                }
            });
        }
    });
}

/// The accent "Next"/"Start" button, also triggered by Return.
fn primary_footer_button(app: &mut Mac2WinApp, ui: &mut Ui, label: &str) {
    let enabled = app.can_advance();
    let clicked = macos::push_button(ui, label, macos::ButtonKind::Primary, enabled).clicked();
    let enter = enabled && ui.input(|i| i.key_pressed(Key::Enter));

    if !(clicked || enter) {
        return;
    }

    if app.step() == Step::Confirm {
        let ctx = ui.ctx().clone();
        app.start_write(&ctx);
    } else {
        app.advance();
    }
}
