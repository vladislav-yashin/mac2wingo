//! macOS-flavoured theme: semantic colours, spacing, typography and window
//! chrome metrics.
//!
//! The goal is to make the egui front-end feel like a native AppKit window:
//! system light/dark appearance, the San Francisco type face, the user's
//! accent colour and unified-toolbar metrics for the traffic lights.

use egui::style::WidgetVisuals;
use egui::{
    vec2, Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, FontTweak,
    Margin, Shadow, Stroke, Style, TextStyle, ThemePreference,
};

/// The named font family used for semibold headings.
pub const SEMIBOLD: &str = "semibold";

/// Backing font used for proportional text.
const SANS: &str = "macos-sans";
/// Backing font used for semibold text.
const SANS_SEMIBOLD: &str = "macos-sans-semibold";
/// Backing font used for monospaced text.
const MONO: &str = "macos-mono";

/// The semibold [`FontFamily`] installed by [`apply`].
#[must_use]
pub fn semibold_family() -> FontFamily {
    FontFamily::Name(SEMIBOLD.into())
}

/// The accent colour of the current style (system accent in production).
#[must_use]
pub fn accent(ui: &egui::Ui) -> Color32 {
    ui.visuals().selection.bg_fill
}

/// Install the theme on a context: system appearance, fonts and styles.
pub fn apply(ctx: &Context) {
    ctx.set_theme(ThemePreference::System);
    install_fonts(ctx);
    ctx.all_styles_mut(apply_style);
}

// ---------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------

fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();

    let mut proportional = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    #[cfg(target_os = "macos")]
    {
        if let Some((regular, mono)) = load_system_fonts() {
            fonts.font_data.insert(
                SANS.to_owned(),
                std::sync::Arc::new(
                    FontData::from_owned(regular.clone()).tweak(weight_tweak(400.0)),
                ),
            );
            fonts.font_data.insert(
                SANS_SEMIBOLD.to_owned(),
                std::sync::Arc::new(FontData::from_owned(regular).tweak(weight_tweak(590.0))),
            );

            proportional.insert(0, SANS.to_owned());
            fonts
                .families
                .insert(FontFamily::Proportional, proportional.clone());

            if let Some(mono) = mono {
                fonts.font_data.insert(
                    MONO.to_owned(),
                    std::sync::Arc::new(FontData::from_owned(mono)),
                );
                let mut monospace = fonts
                    .families
                    .get(&FontFamily::Monospace)
                    .cloned()
                    .unwrap_or_default();
                monospace.insert(0, MONO.to_owned());
                fonts.families.insert(FontFamily::Monospace, monospace);
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    let _ = &mut proportional;

    // Always provide the semibold family so text styles can reference it.
    let mut semibold = Vec::with_capacity(proportional.len() + 1);
    if fonts.font_data.contains_key(SANS_SEMIBOLD) {
        semibold.push(SANS_SEMIBOLD.to_owned());
    }
    semibold.extend(proportional);
    fonts.families.insert(semibold_family(), semibold);

    ctx.set_fonts(fonts);
}

/// A [`FontTweak`] that selects a weight on a variable font's `wght` axis.
#[cfg(target_os = "macos")]
fn weight_tweak(weight: f32) -> FontTweak {
    use egui::epaint::text::VariationCoords;
    FontTweak {
        coords: VariationCoords::new([("wght", weight)]),
        ..Default::default()
    }
}

/// Read the system sans/mono fonts, if present.
#[cfg(target_os = "macos")]
fn load_system_fonts() -> Option<(Vec<u8>, Option<Vec<u8>>)> {
    const SANS_PATHS: [&str; 3] = [
        "/System/Library/Fonts/SFNS.ttf",
        "/System/Library/Fonts/HelveticaNeue.ttc",
        "/System/Library/Fonts/Geneva.ttf",
    ];
    const MONO_PATHS: [&str; 2] = [
        "/System/Library/Fonts/SFNSMono.ttf",
        "/System/Library/Fonts/Menlo.ttc",
    ];

    let regular = SANS_PATHS
        .iter()
        .find_map(|path| std::fs::read(path).ok())?;
    let mono = MONO_PATHS.iter().find_map(|path| std::fs::read(path).ok());
    Some((regular, mono))
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

fn apply_style(style: &mut Style) {
    let dark = style.visuals.dark_mode;
    let palette = Palette::new(dark, accent_color(accent_index(), dark));

    style.text_styles = [
        (TextStyle::Heading, FontId::new(15.0, semibold_family())),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
    ]
    .into();

    style.spacing.item_spacing = vec2(8.0, 8.0);
    style.spacing.button_padding = vec2(14.0, 5.0);
    style.spacing.window_margin = Margin::same(16);
    style.spacing.menu_margin = Margin::same(6);
    style.spacing.indent = 16.0;
    style.spacing.interact_size = vec2(24.0, 26.0);
    style.spacing.slider_width = 140.0;
    style.spacing.combo_width = 220.0;
    style.spacing.text_edit_width = 220.0;
    style.spacing.icon_width = 16.0;
    style.spacing.icon_width_inner = 8.0;
    style.spacing.icon_spacing = 8.0;
    style.spacing.combo_height = 320.0;
    style.spacing.menu_width = 240.0;
    style.spacing.extra_text_line_spacing = 2.0;

    apply_visuals(&mut style.visuals, &palette);
}

fn apply_visuals(visuals: &mut egui::Visuals, p: &Palette) {
    visuals.override_text_color = None;
    visuals.weak_text_color = Some(p.secondary);
    visuals.panel_fill = p.window;
    visuals.window_fill = p.window;
    visuals.window_stroke = Stroke::NONE;
    visuals.window_shadow = Shadow {
        offset: [0, 12],
        blur: 32,
        spread: 0,
        color: p.shadow,
    };
    visuals.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: p.shadow,
    };
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);

    visuals.faint_bg_color = p.faint;
    visuals.extreme_bg_color = p.field;
    visuals.text_edit_bg_color = Some(p.field);
    visuals.code_bg_color = p.faint;
    visuals.hyperlink_color = p.accent;
    visuals.warn_fg_color = p.warning;
    visuals.error_fg_color = p.danger;

    visuals.selection.bg_fill = p.accent;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);

    visuals.button_frame = true;
    visuals.striped = false;
    visuals.indent_has_left_vline = false;
    visuals.slider_trailing_fill = true;

    visuals.widgets.noninteractive = WidgetVisuals {
        bg_fill: Color32::TRANSPARENT,
        weak_bg_fill: Color32::TRANSPARENT,
        bg_stroke: Stroke::new(1.0, p.separator),
        fg_stroke: Stroke::new(1.0, p.label),
        corner_radius: CornerRadius::same(6),
        expansion: 0.0,
    };
    visuals.widgets.inactive = WidgetVisuals {
        bg_fill: p.control,
        weak_bg_fill: p.control,
        bg_stroke: Stroke::new(1.0, p.control_stroke),
        fg_stroke: Stroke::new(1.0, p.label),
        corner_radius: CornerRadius::same(6),
        expansion: 0.0,
    };
    visuals.widgets.hovered = WidgetVisuals {
        bg_fill: p.control_hover,
        weak_bg_fill: p.control_hover,
        bg_stroke: Stroke::new(1.0, p.control_stroke),
        fg_stroke: Stroke::new(1.0, p.label),
        corner_radius: CornerRadius::same(6),
        expansion: 0.0,
    };
    visuals.widgets.active = WidgetVisuals {
        bg_fill: p.control_active,
        weak_bg_fill: p.control_active,
        bg_stroke: Stroke::new(1.0, p.control_stroke),
        fg_stroke: Stroke::new(1.0, p.label),
        corner_radius: CornerRadius::same(6),
        expansion: 0.0,
    };
    visuals.widgets.open = WidgetVisuals {
        bg_fill: p.control,
        weak_bg_fill: p.control,
        bg_stroke: Stroke::new(1.0, p.accent),
        fg_stroke: Stroke::new(1.0, p.label),
        corner_radius: CornerRadius::same(6),
        expansion: 0.0,
    };
}

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------

struct Palette {
    window: Color32,
    control: Color32,
    control_hover: Color32,
    control_active: Color32,
    control_stroke: Color32,
    field: Color32,
    faint: Color32,
    separator: Color32,
    label: Color32,
    secondary: Color32,
    accent: Color32,
    warning: Color32,
    danger: Color32,
    shadow: Color32,
}

impl Palette {
    fn new(dark: bool, accent: Color32) -> Self {
        if dark {
            Self {
                window: rgb(30, 30, 32),
                control: rgb(58, 58, 60),
                control_hover: rgb(68, 68, 70),
                control_active: rgb(78, 78, 80),
                control_stroke: rgb(84, 84, 86),
                field: rgb(28, 28, 30),
                faint: rgb(44, 44, 46),
                separator: Color32::from_white_alpha(24),
                label: rgb(242, 242, 247),
                secondary: rgb(161, 161, 166),
                accent,
                warning: rgb(255, 159, 10),
                danger: rgb(255, 69, 58),
                shadow: Color32::from_black_alpha(140),
            }
        } else {
            Self {
                window: rgb(236, 236, 236),
                control: rgb(255, 255, 255),
                control_hover: rgb(247, 247, 247),
                control_active: rgb(229, 229, 229),
                control_stroke: rgb(210, 210, 210),
                field: rgb(255, 255, 255),
                faint: rgb(242, 242, 247),
                separator: Color32::from_black_alpha(24),
                label: rgb(29, 29, 31),
                secondary: rgb(110, 110, 115),
                accent,
                warning: rgb(224, 134, 0),
                danger: rgb(215, 0, 21),
                shadow: Color32::from_black_alpha(40),
            }
        }
    }
}

/// The macOS system accent for a given `AppleAccentColor` index.
fn accent_color(index: Option<i32>, dark_colors: bool) -> Color32 {
    let color = |light: (u8, u8, u8), dark: (u8, u8, u8)| {
        if dark_colors {
            rgb(dark.0, dark.1, dark.2)
        } else {
            rgb(light.0, light.1, light.2)
        }
    };
    match index {
        Some(-1) => color((142, 142, 147), (152, 152, 157)),
        Some(0) => color((255, 59, 48), (255, 69, 58)),
        Some(1) => color((255, 149, 0), (255, 159, 10)),
        Some(2) => color((255, 204, 0), (255, 214, 10)),
        Some(3) => color((52, 199, 89), (48, 209, 88)),
        Some(5) => color((175, 82, 222), (191, 90, 242)),
        Some(6) => color((255, 45, 85), (255, 55, 95)),
        // 4 and anything unset/unknown default to system blue.
        _ => color((0, 122, 255), (10, 132, 255)),
    }
}

fn parse_accent_index(text: &str) -> Option<i32> {
    text.trim().parse().ok()
}

fn accent_index() -> Option<i32> {
    static CACHE: std::sync::OnceLock<Option<i32>> = std::sync::OnceLock::new();
    *CACHE.get_or_init(read_accent_index)
}

#[cfg(target_os = "macos")]
fn read_accent_index() -> Option<i32> {
    let output = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleAccentColor"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_accent_index(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(target_os = "macos"))]
fn read_accent_index() -> Option<i32> {
    None
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

// ---------------------------------------------------------------------------
// Window chrome
// ---------------------------------------------------------------------------

/// Space reserved for the native macOS window controls ("traffic lights") when
/// the content view is drawn behind the titlebar.
#[derive(Clone, Copy, Debug)]
pub struct ChromeInset {
    pub left: f32,
    pub top: f32,
}

impl ChromeInset {
    /// Fallback metrics used when the native window handle is unavailable.
    pub const DEFAULT: Self = Self {
        left: 78.0,
        top: 28.0,
    };
}

/// Measure the traffic-light area so the unified toolbar can avoid it.
#[must_use]
pub fn chrome_inset(frame: &eframe::Frame, zoom: f32) -> ChromeInset {
    #[cfg(target_os = "macos")]
    {
        use raw_window_handle::HasWindowHandle as _;
        if let Some(window) = frame.winit_window() {
            if let Ok(handle) = window.window_handle() {
                if let Some(metrics) =
                    eframe::WindowChromeMetrics::from_window_handle(&handle.as_raw())
                {
                    let size = metrics.traffic_lights_size / zoom.max(1.0);
                    return ChromeInset {
                        left: size.x,
                        top: size.y,
                    };
                }
            }
        }
    }
    let _ = (frame, zoom);
    ChromeInset::DEFAULT
}

/// Lighten a colour by blending it towards white.
#[must_use]
pub fn lighten(color: Color32, amount: f32) -> Color32 {
    let [r, g, b, a] = color.to_array();
    let mix = |c: u8| (f32::from(c) + (255.0 - f32::from(c)) * amount).round() as u8;
    Color32::from_rgba_unmultiplied(mix(r), mix(g), mix(b), a)
}

/// Darken a colour by blending it towards black.
#[must_use]
pub fn darken(color: Color32, amount: f32) -> Color32 {
    let [r, g, b, a] = color.to_array();
    let mix = |c: u8| (f32::from(c) * (1.0 - amount)).round() as u8;
    Color32::from_rgba_unmultiplied(mix(r), mix(g), mix(b), a)
}

/// Height of the compact unified-toolbar row.
pub const TOOLBAR_ROW_HEIGHT: f32 = 22.0;

/// Vertical gap around the toolbar row inside the header panel. The header is
/// 1.5× the native titlebar so the traffic lights can be centred in it.
pub const TOOLBAR_GAP: f32 = 10.0;

/// Height of the unified header panel, which also hosts the traffic lights.
#[must_use]
pub fn header_panel_height(inset: &ChromeInset) -> f32 {
    inset.top.max(TOOLBAR_ROW_HEIGHT + 2.0 * TOOLBAR_GAP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_parsing() {
        assert_eq!(parse_accent_index(" 4\n"), Some(4));
        assert_eq!(parse_accent_index("-1"), Some(-1));
        assert_eq!(parse_accent_index(""), None);
        assert_eq!(parse_accent_index("blue"), None);
    }

    #[test]
    fn accent_mapping_uses_blue_as_default() {
        assert_eq!(accent_color(None, false), rgb(0, 122, 255));
        assert_eq!(accent_color(Some(4), false), rgb(0, 122, 255));
        assert_eq!(accent_color(Some(3), false), rgb(52, 199, 89));
        assert_ne!(accent_color(Some(3), true), accent_color(Some(3), false));
    }

    #[test]
    fn lighten_and_darken_move_towards_extremes() {
        let base = rgb(100, 100, 100);
        assert!(lighten(base, 0.5).r() > base.r());
        assert!(darken(base, 0.5).r() < base.r());
    }
}
