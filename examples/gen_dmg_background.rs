//! Draw the background image used by the DMG installer window.
//!
//! Run with `cargo run --example gen_dmg_background`; the script
//! `scripts/generate-dmg-background.sh` then produces the 1x and 2x PNGs that
//! `dmgbuild` combines into a HiDPI background.

use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, Path, PathBuilder, Pixmap, Point, Shader,
    SpreadMode, Transform,
};

/// The DMG window is 660x420 points; we render at 2x for Retina displays.
const SCALE: f32 = 2.0;
const WIDTH: u32 = 660 * 2;
const HEIGHT: u32 = 420 * 2;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut pixmap = Pixmap::new(WIDTH, HEIGHT).expect("create pixmap");

    let background = rect(0.0, 0.0, 660.0, 420.0).ok_or("background path")?;
    let gradient = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(0.0, HEIGHT as f32),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(250, 252, 255, 255)),
            GradientStop::new(1.0, Color::from_rgba8(224, 235, 250, 255)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .ok_or("failed to build gradient")?;
    let paint = Paint {
        shader: gradient,
        ..Default::default()
    };
    pixmap.fill_path(
        &background,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    // Arrow pointing from the app icon towards the Applications shortcut.
    let arrow = Color::from_rgba8(74, 144, 226, 255);
    let shaft = rounded_rect(250.0, 201.0, 130.0, 18.0, 9.0).ok_or("shaft path")?;
    fill(&mut pixmap, &shaft, arrow);

    let mut head = PathBuilder::new();
    head.move_to(375.0 * SCALE, 165.0 * SCALE);
    head.line_to(420.0 * SCALE, 210.0 * SCALE);
    head.line_to(375.0 * SCALE, 255.0 * SCALE);
    head.close();
    let head = head.finish().ok_or("head path")?;
    fill(&mut pixmap, &head, arrow);

    std::fs::create_dir_all("assets")?;
    pixmap.save_png("assets/dmg_background@2x.png")?;
    println!("wrote assets/dmg_background@2x.png");
    Ok(())
}

fn fill(pixmap: &mut Pixmap, path: &Path, color: Color) {
    let paint = Paint {
        shader: Shader::SolidColor(color),
        ..Default::default()
    };
    pixmap.fill_path(path, &paint, FillRule::Winding, Transform::identity(), None);
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Option<Path> {
    let mut builder = PathBuilder::new();
    builder.move_to(x * SCALE, y * SCALE);
    builder.line_to((x + w) * SCALE, y * SCALE);
    builder.line_to((x + w) * SCALE, (y + h) * SCALE);
    builder.line_to(x * SCALE, (y + h) * SCALE);
    builder.close();
    builder.finish()
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let (x, y, w, h, r) = (x * SCALE, y * SCALE, w * SCALE, h * SCALE, r * SCALE);
    let mut builder = PathBuilder::new();
    builder.move_to(x + r, y);
    builder.line_to(x + w - r, y);
    builder.quad_to(x + w, y, x + w, y + r);
    builder.line_to(x + w, y + h - r);
    builder.quad_to(x + w, y + h, x + w - r, y + h);
    builder.line_to(x + r, y + h);
    builder.quad_to(x, y + h, x, y + h - r);
    builder.line_to(x, y + r);
    builder.quad_to(x, y, x + r, y);
    builder.close();
    builder.finish()
}
