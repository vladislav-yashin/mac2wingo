//! Draw the application icon used by the `.app` bundle.
//!
//! Run with `cargo run --example gen_icon`; the script
//! `scripts/generate-icon.sh` then scales the master PNG into an `.icns`.

use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, Path, PathBuilder, Pixmap, Point, Shader,
    SpreadMode, Transform,
};

const SIZE: u32 = 1024;
const ICON_MARGIN: f32 = 100.0;
const ICON_RADIUS: f32 = 185.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut pixmap = Pixmap::new(SIZE, SIZE).expect("create pixmap");

    // Rounded-square background with a blue gradient.
    let background = rounded_rect(
        ICON_MARGIN,
        ICON_MARGIN,
        SIZE as f32 - ICON_MARGIN * 2.0,
        SIZE as f32 - ICON_MARGIN * 2.0,
        ICON_RADIUS,
    )
    .ok_or("failed to build background path")?;

    let gradient = LinearGradient::new(
        Point::from_xy(0.0, ICON_MARGIN),
        Point::from_xy(0.0, SIZE as f32 - ICON_MARGIN),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(64, 166, 255, 255)),
            GradientStop::new(1.0, Color::from_rgba8(8, 88, 240, 255)),
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

    // USB stick: white body with a light connector.
    let body = rounded_rect(357.0, 352.0, 310.0, 420.0, 56.0).ok_or("body path")?;
    fill(&mut pixmap, &body, Color::from_rgba8(255, 255, 255, 255));

    let connector = rounded_rect(439.0, 236.0, 146.0, 132.0, 22.0).ok_or("connector path")?;
    fill(
        &mut pixmap,
        &connector,
        Color::from_rgba8(210, 214, 220, 255),
    );
    let slot = rounded_rect(463.0, 268.0, 98.0, 30.0, 10.0).ok_or("slot path")?;
    fill(&mut pixmap, &slot, Color::from_rgba8(160, 166, 174, 255));

    // Windows four-pane logo on the body.
    let panes = [
        (436.0, 500.0),
        (521.0, 500.0),
        (436.0, 585.0),
        (521.0, 585.0),
    ];
    for (x, y) in panes {
        let pane = rounded_rect(x, y, 67.0, 67.0, 8.0).ok_or("pane path")?;
        fill(&mut pixmap, &pane, Color::from_rgba8(10, 90, 220, 255));
    }

    // Activity LED near the base of the stick.
    let led = rounded_rect(498.0, 700.0, 28.0, 12.0, 6.0).ok_or("led path")?;
    fill(&mut pixmap, &led, Color::from_rgba8(140, 148, 158, 255));

    std::fs::create_dir_all("assets")?;
    pixmap.save_png("assets/icon_1024.png")?;
    println!("wrote assets/icon_1024.png");
    Ok(())
}

fn fill(pixmap: &mut Pixmap, path: &Path, color: Color) {
    let paint = Paint {
        shader: Shader::SolidColor(color),
        ..Default::default()
    };
    pixmap.fill_path(path, &paint, FillRule::Winding, Transform::identity(), None);
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
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
