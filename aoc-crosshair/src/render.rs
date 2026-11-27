//! Rasterização pura da mira (tiny-skia) — unit-tested com asserts de pixel.

use aoc_gmenu::config::{parse_rgba_hex, CrosshairConfig, CrosshairShape};
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Tamanho do pixmap = size*2+outline margem (centrado).
pub fn pixmap_size(cfg: &CrosshairConfig) -> u32 {
    cfg.size
        .saturating_mul(2)
        .saturating_add(cfg.outline_thickness.saturating_mul(4))
        .saturating_add(8)
        .max(8)
}

fn color_from_hex(hex: &str, opacity: f32) -> Color {
    let (r, g, b, a) = parse_rgba_hex(hex).unwrap_or((255, 0, 0, 255));
    let a = ((f32::from(a) / 255.0) * opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::from_rgba8(r, g, b, a)
}

fn stroke_paint(color: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(color);
    p.anti_alias = true;
    p
}

fn draw_line(
    pixmap: &mut Pixmap,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    color: Color,
) {
    let mut pb = PathBuilder::new();
    pb.move_to(x0, y0);
    pb.line_to(x1, y1);
    if let Some(path) = pb.finish() {
        let stroke = Stroke {
            width: thickness.max(1.0),
            ..Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &stroke_paint(color),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

fn draw_circle_stroke(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    radius: f32,
    thickness: f32,
    color: Color,
) {
    let mut pb = PathBuilder::new();
    // tiny-skia: approximate circle with push_circle if available
    pb.push_circle(cx, cy, radius);
    if let Some(path) = pb.finish() {
        let stroke = Stroke {
            width: thickness.max(1.0),
            ..Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &stroke_paint(color),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

fn fill_circle(pixmap: &mut Pixmap, cx: f32, cy: f32, radius: f32, color: Color) {
    let mut pb = PathBuilder::new();
    pb.push_circle(cx, cy, radius);
    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &stroke_paint(color),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// Desenha a mira centrada num pixmap transparente.
pub fn render_crosshair(cfg: &CrosshairConfig) -> Pixmap {
    let dim = pixmap_size(cfg);
    let mut pixmap = Pixmap::new(dim, dim).expect("pixmap");
    // transparente
    pixmap.fill(Color::from_rgba8(0, 0, 0, 0));

    let cx = dim as f32 / 2.0;
    let cy = dim as f32 / 2.0;
    let half = cfg.size as f32;
    let gap = cfg.gap as f32;
    let thick = cfg.thickness as f32;
    let color = color_from_hex(&cfg.color, cfg.opacity);
    let outline = color_from_hex(&cfg.outline_color, cfg.opacity);
    let othick = cfg.outline_thickness as f32;

    let draw_cross = |pixmap: &mut Pixmap, t: f32, c: Color| {
        // horizontal arms
        if gap < half {
            draw_line(pixmap, cx - half, cy, cx - gap, cy, t, c);
            draw_line(pixmap, cx + gap, cy, cx + half, cy, t, c);
            draw_line(pixmap, cx, cy - half, cx, cy - gap, t, c);
            draw_line(pixmap, cx, cy + gap, cx, cy + half, t, c);
        }
    };

    let draw_x = |pixmap: &mut Pixmap, t: f32, c: Color| {
        let g = gap / std::f32::consts::SQRT_2;
        let h = half / std::f32::consts::SQRT_2;
        if g < h {
            draw_line(pixmap, cx - h, cy - h, cx - g, cy - g, t, c);
            draw_line(pixmap, cx + g, cy + g, cx + h, cy + h, t, c);
            draw_line(pixmap, cx + h, cy - h, cx + g, cy - g, t, c);
            draw_line(pixmap, cx - g, cy + g, cx - h, cy + h, t, c);
        }
    };

    match cfg.shape {
        CrosshairShape::Cross => {
            if cfg.outline {
                draw_cross(&mut pixmap, thick + othick * 2.0, outline);
            }
            draw_cross(&mut pixmap, thick, color);
        }
        CrosshairShape::X => {
            if cfg.outline {
                draw_x(&mut pixmap, thick + othick * 2.0, outline);
            }
            draw_x(&mut pixmap, thick, color);
        }
        CrosshairShape::Dot => {
            let r = (cfg.size as f32 / 4.0).max(1.0);
            if cfg.outline {
                fill_circle(&mut pixmap, cx, cy, r + othick, outline);
            }
            fill_circle(&mut pixmap, cx, cy, r, color);
        }
        CrosshairShape::Circle => {
            let r = half * 0.6;
            if cfg.outline {
                draw_circle_stroke(&mut pixmap, cx, cy, r, thick + othick * 2.0, outline);
            }
            draw_circle_stroke(&mut pixmap, cx, cy, r, thick, color);
        }
        CrosshairShape::CrossDot => {
            if cfg.outline {
                draw_cross(&mut pixmap, thick + othick * 2.0, outline);
            }
            draw_cross(&mut pixmap, thick, color);
            let r = (thick * 1.5).max(1.5);
            if cfg.outline {
                fill_circle(&mut pixmap, cx, cy, r + othick, outline);
            }
            fill_circle(&mut pixmap, cx, cy, r, color);
        }
    }

    pixmap
}

/// Pixel não-transparente no centro (para asserts).
#[cfg_attr(not(test), allow(dead_code))]
pub fn center_alpha(pixmap: &Pixmap) -> u8 {
    let w = pixmap.width();
    let h = pixmap.height();
    let px = pixmap.pixel(w / 2, h / 2).unwrap();
    px.alpha()
}

/// Conta pixels com alpha > limiar.
#[cfg_attr(not(test), allow(dead_code))]
pub fn count_opaque(pixmap: &Pixmap, threshold: u8) -> usize {
    pixmap
        .pixels()
        .iter()
        .filter(|p| p.alpha() > threshold)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(shape: CrosshairShape) -> CrosshairConfig {
        CrosshairConfig {
            enabled: true,
            shape,
            size: 20,
            thickness: 2,
            color: "#FF0000FF".into(),
            opacity: 1.0,
            gap: 3,
            outline: false,
            outline_color: "#000000FF".into(),
            outline_thickness: 1,
            output: "DP-1".into(),
        }
    }

    #[test]
    fn cross_draws_opaque_pixels() {
        let pm = render_crosshair(&cfg(CrosshairShape::Cross));
        assert!(count_opaque(&pm, 10) > 20);
        // centro com gap deve ser transparente
        assert_eq!(center_alpha(&pm), 0);
        // pixel no braço horizontal
        let mid_y = pm.height() / 2;
        let arm_x = pm.width() / 2 + 10;
        let a = pm.pixel(arm_x, mid_y).unwrap().alpha();
        assert!(a > 10, "braço horizontal deveria ser opaco, alpha={a}");
    }

    #[test]
    fn dot_center_opaque() {
        let pm = render_crosshair(&cfg(CrosshairShape::Dot));
        assert!(center_alpha(&pm) > 200);
    }

    #[test]
    fn circle_has_ring_not_full_center() {
        let pm = render_crosshair(&cfg(CrosshairShape::Circle));
        assert!(count_opaque(&pm, 10) > 10);
        // centro oco (stroke only)
        assert_eq!(center_alpha(&pm), 0);
    }

    #[test]
    fn x_shape_opaque() {
        let pm = render_crosshair(&cfg(CrosshairShape::X));
        assert!(count_opaque(&pm, 10) > 10);
    }

    #[test]
    fn cross_dot_center_filled() {
        let pm = render_crosshair(&cfg(CrosshairShape::CrossDot));
        assert!(center_alpha(&pm) > 100);
        assert!(count_opaque(&pm, 10) > 30);
    }

    #[test]
    fn outline_increases_coverage() {
        let mut c = cfg(CrosshairShape::Cross);
        let bare = count_opaque(&render_crosshair(&c), 5);
        c.outline = true;
        let with = count_opaque(&render_crosshair(&c), 5);
        assert!(with >= bare);
    }
}
