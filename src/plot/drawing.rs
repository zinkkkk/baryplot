//! Drawing primitives for paths, roots, grids and labels.
use super::*;

pub(super) fn draw_paths(
    frame: &mut Frame,
    s: &crate::model::Settings,
    plan: Option<&Plan>,
    data: Option<&RenderData>,
    b: Projection,
    a: Rectangle,
) {
    let Some(plan) = plan else {
        return;
    };
    for p in &plan.paths {
        if s.input_path && p.count > 0 && p.kind != InputKind::Point {
            for (left, right, opacity, extra) in [
                (-p.extra, 0., 0.28, true),
                (1., 1. + p.extra, 0.28, true),
                (0., 1., 1., false),
            ] {
                if extra && (!s.input_line.extrapolation || p.extra == 0.) {
                    continue;
                }
                let path = Path::new(|builder| {
                    let mut connected = false;
                    for i in 0..=256 {
                        let point = screen(
                            p.input(left + (right - left) * i as f64 / 256., s.mode),
                            b,
                            a,
                        );
                        if !finite(point) {
                            connected = false;
                            continue;
                        }
                        if connected {
                            builder.line_to(point);
                        } else {
                            builder.move_to(point);
                        }
                        connected = true;
                    }
                });
                frame.stroke(&path, line_stroke(s.input_line, s.input_opacity * opacity));
            }
            if p.kind == InputKind::Curve && s.input_line.control_polygon {
                let polygon = Path::new(|builder| {
                    let mut connected = false;
                    for point in &p.points {
                        let point = screen(
                            C::new(point[0], if s.mode == Mode::Real { 0. } else { point[1] }),
                            b,
                            a,
                        );
                        if !finite(point) {
                            connected = false;
                            continue;
                        }
                        if connected {
                            builder.line_to(point);
                        } else {
                            builder.move_to(point);
                        }
                        connected = true;
                    }
                });
                frame.stroke(
                    &polygon,
                    Stroke::default()
                        .with_color(alpha(color(s.input_line.color), s.input_opacity * 0.25)),
                );
            }
        }
        if s.output
            && let Some(data) = data
        {
            // Sampling appends paths in index order. Limit each draw to its own
            // contiguous range instead of scanning every other path's samples.
            let start = data.samples.partition_point(|v| v.path < p.path);
            let end = data.samples.partition_point(|v| v.path <= p.path);
            let samples = &data.samples[start..end];
            if p.kind == InputKind::Point {
                for sample in samples {
                    if let Some(output) = sample.output {
                        let z = if s.mode == Mode::Real {
                            C::new(sample.input.re, output.re)
                        } else {
                            output
                        };
                        let point = screen(z, b, a);
                        if finite(point) && a.contains(point) {
                            frame.fill(
                                &Path::circle(point, 5.),
                                alpha(color(s.output_line.color), s.line_opacity),
                            );
                        }
                    }
                }
                continue;
            }
            for extra in [true, false] {
                if extra && !s.output_line.extrapolation {
                    continue;
                }
                let path = Path::new(|builder| {
                    let mut previous: Option<Point> = None;
                    for sample in samples {
                        if sample.extrapolated != extra {
                            previous = None;
                            continue;
                        }
                        let Some(output) = sample.output else {
                            previous = None;
                            continue;
                        };
                        let point = screen(
                            if s.mode == Mode::Real {
                                C::new(sample.input.re, output.re)
                            } else {
                                output
                            },
                            b,
                            a,
                        );
                        if !finite(point)
                            || point.x.abs() > a.width * 8.
                            || point.y.abs() > a.height * 8.
                        {
                            previous = None;
                            continue;
                        }
                        if previous
                            .is_some_and(|last| last.distance(point) < a.width.max(a.height) * 0.65)
                        {
                            builder.line_to(point);
                        } else {
                            builder.move_to(point);
                        }
                        previous = Some(point);
                    }
                });
                let mut stroke = line_stroke(
                    s.output_line,
                    s.line_opacity * if extra { 0.38 } else { 1. },
                );
                if extra {
                    stroke.width *= 0.65;
                }
                frame.stroke(&path, stroke);
            }
        }
    }
}

fn line_stroke(style: crate::appearance::LineStyle, opacity: f32) -> Stroke<'static> {
    let mut stroke = Stroke::default()
        .with_color(alpha(color(style.color), opacity))
        .with_width(style.width);
    if style.dashed {
        stroke.line_dash = canvas::LineDash {
            segments: &[8., 5.],
            offset: 0,
        };
    }
    stroke
}
fn compact(z: C) -> String {
    if z.im.abs() < 1e-12 * (1. + z.re.abs()) {
        format!("{:.3e}", z.re)
    } else {
        format!("{:.2e}{:+.2e}i", z.re, z.im)
    }
}
pub(super) fn draw_spectrum(
    frame: &mut Frame,
    spectrum: &crate::numerics::Spectrum,
    filter: crate::numerics::filter::DisplayFilter,
    s: &crate::model::Settings,
    b: impl Into<Projection>,
    a: Rectangle,
) {
    let b = b.into();
    let pole_color = Color::from_rgb(1., 0.42, 0.43);
    if s.analysis.poles || s.analysis.residues {
        for (i, &z) in spectrum.poles.iter().enumerate() {
            if !filter.pole(spectrum, i) {
                continue;
            }
            let p = screen(z, b, a);
            if !p.x.is_finite() || !p.y.is_finite() || p.x < a.x || p.x > a.x + a.width {
                continue;
            }
            if !a.contains(p) {
                continue;
            }
            let position = Point::new(p.x, p.y.clamp(a.y + 9., a.y + a.height - 9.));
            if a.contains(p) {
                for (u, v) in [((-5., -5.), (5., 5.)), ((-5., 5.), (5., -5.))] {
                    frame.stroke(
                        &Path::line(
                            Point::new(p.x + u.0, p.y + u.1),
                            Point::new(p.x + v.0, p.y + v.1),
                        ),
                        Stroke::default().with_width(2.).with_color(pole_color),
                    );
                }
            }
            let caption = if s.analysis.residues {
                format!(
                    "p{} · res {}",
                    i + 1,
                    spectrum
                        .residues
                        .get(i)
                        .copied()
                        .flatten()
                        .map_or("unavailable".into(), compact)
                )
            } else {
                format!("p{}", i + 1)
            };
            label(
                frame,
                caption,
                Point::new(position.x + 8., position.y - 16.),
                pole_color,
                11.,
            );
        }
    }
    if s.analysis.zeros {
        for (i, &z) in spectrum.zeros.iter().enumerate() {
            if !filter.zero(spectrum, i) {
                continue;
            }
            let p = screen(z, b, a);
            if !p.x.is_finite() || !p.y.is_finite() || !a.contains(p) {
                continue;
            }
            frame.stroke(
                &Path::circle(p, 9.),
                Stroke::default()
                    .with_width(2.)
                    .with_color(Color::from_rgb(0.48, 0.68, 1.)),
            );
            label(
                frame,
                format!("z{}", i + 1),
                Point::new(p.x + 7., p.y + 4.),
                Color::from_rgb(0.58, 0.75, 1.),
                11.,
            );
        }
    }
}
pub(super) fn label(frame: &mut Frame, content: String, position: Point, color: Color, size: f32) {
    frame.fill_text(canvas::Text {
        content,
        position,
        color,
        size: size.into(),
        ..Default::default()
    });
}
pub(super) fn guide_stroke(s: StrokeStyle) -> Stroke<'static> {
    let mut stroke = Stroke::default()
        .with_width(s.width)
        .with_color(alpha(color(s.color), s.opacity));
    if s.dashed {
        stroke.line_dash = canvas::LineDash {
            segments: &[6., 4.],
            offset: 0,
        };
    }
    stroke
}
pub(super) fn draw_grid(
    frame: &mut Frame,
    b: Projection,
    a: Rectangle,
    plan: Option<&Plan>,
    g: axes::Guides,
) {
    let p = b.mapping.projected(b.bounds);
    for (horizontal, min, max, step, axis) in [
        (false, p.x_min, p.x_max, plan.and_then(|p| p.grid_step), g.x),
        (
            true,
            p.y_min,
            p.y_max,
            plan.and_then(|p| p.grid_y_step),
            g.y,
        ),
    ] {
        let step =
            step.unwrap_or_else(|| axes::nice_step((max - min) / f64::from(axis.ticks.max(2))));
        let line = |frame: &mut Frame, t: f64, style: StrokeStyle| {
            let fraction = (t - min) / (max - min);
            let (u, v) = if horizontal {
                let y = a.y + a.height * (1. - fraction as f32);
                (Point::new(a.x, y), Point::new(a.x + a.width, y))
            } else {
                let x = a.x + a.width * fraction as f32;
                (Point::new(x, a.y), Point::new(x, a.y + a.height))
            };
            frame.stroke(&Path::line(u, v), guide_stroke(style));
        };
        if g.minor && axis.subdivisions > 1 {
            for t in axes::ticks(min - step, max, Some(step), axis.ticks) {
                for i in 1..axis.subdivisions.min(20) {
                    let t = t + step * f64::from(i) / f64::from(axis.subdivisions);
                    if t >= min && t <= max {
                        line(frame, t, g.minor_style);
                    }
                }
            }
        }
        if g.major {
            for t in axes::ticks(min, max, Some(step), axis.ticks) {
                line(frame, t, g.major_style);
            }
        }
        let scale = if horizontal { b.mapping.y } else { b.mapping.x };
        if g.zero && scale != axes::Scale::Log10 && min <= 0. && max >= 0. {
            line(frame, 0., g.zero_style);
        }
    }
}
pub(super) fn cursor_caption(mode: Mode, position: C, value: Option<C>) -> String {
    match mode {
        Mode::Real => format!(
            "x: {}   y: {}   f(x): {}",
            short(position.re),
            short(position.im),
            value.map_or_else(|| "undefined".into(), |v| short(v.re))
        ),
        Mode::Complex => format!(
            "z: {} {:+.3}i    f(z): {}",
            short(position.re),
            position.im,
            value.map_or_else(
                || "undefined".into(),
                |v| format!("{} {:+.3}i", short(v.re), v.im)
            )
        ),
    }
}
fn short(x: f64) -> String {
    if x != 0. && (x.abs() < 0.001 || x.abs() >= 10000.) {
        format!("{x:.2e}")
    } else {
        format!("{x:.3}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}
