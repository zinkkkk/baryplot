//! CPU sampling is independent of the event loop; domain pixels use Rayon.
use crate::{
    expression,
    model::{Bounds, Compiled, Curve, Extent, InputKind, InputPath, Mode, Settings},
};
use num_complex::Complex64 as C;
use rayon::prelude::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

#[derive(Debug, Clone)]
pub struct Sample {
    pub path: usize,
    pub input: C,
    pub output: Option<C>,
    pub extrapolated: bool,
}
#[derive(Debug, Clone)]
pub struct RenderData {
    pub samples: Arc<[Sample]>,
    pub image: Option<iced::widget::image::Handle>,
    pub domain: Bounds,
    pub elapsed_ms: f64,
    pub missing: usize,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PathPlan {
    pub path: usize,
    pub kind: InputKind,
    pub curve: Curve,
    pub count: usize,
    pub interval: [f64; 2],
    pub extra: f64,
    pub points: Vec<[f64; 2]>,
    pub curve_weights: Vec<f64>,
}
impl PathPlan {
    pub fn input(&self, t: f64, mode: Mode) -> C {
        let z = match self.kind {
            InputKind::Interval => C::new(
                self.interval[0] + t * (self.interval[1] - self.interval[0]),
                0.,
            ),
            InputKind::Point => self
                .points
                .first()
                .map_or(C::default(), |p| C::new(p[0], p[1])),
            InputKind::Curve => curve_point(&self.points, &self.curve_weights, self.curve, t),
        };
        if mode == Mode::Real {
            C::new(z.re, 0.)
        } else {
            z
        }
    }
}
#[derive(Debug, Clone)]
pub struct Plan {
    pub paths: Vec<PathPlan>,
    pub path_errors: std::collections::BTreeMap<usize, String>,
    pub domain: Bounds,
    pub grid_step: Option<f64>,
    pub grid_y_step: Option<f64>,
}
pub fn real(source: &str, model: &Compiled) -> Result<f64, String> {
    let z = expression::evaluate(source, &model.variables)?;
    if z.im.abs() > 1e-12 {
        Err("Expected a real value".into())
    } else {
        Ok(z.re)
    }
}
pub fn plan(s: &Settings, model: &Compiled) -> Result<Plan, String> {
    if !s.guides.mapping.valid(s.bounds) {
        return Err(
            "Axis bounds must increase in the selected scale; log10 bounds must be positive".into(),
        );
    }
    let margin = if s.mode == Mode::Complex || s.real_coloring {
        [
            real(&s.domain_margin[0], model)?,
            real(&s.domain_margin[1], model)?,
        ]
    } else {
        [0., 0.]
    };
    if margin.iter().any(|m| *m < 0. || *m > 1e6) {
        return Err("Domain margins must be between 0 and 1000000".into());
    }
    let mapping = s.guides.mapping;
    let projected = mapping.projected(s.bounds);
    let domain = match s.domain_extent {
        Extent::Percent => mapping.unprojected(projected.expanded(
            margin[0] * projected.width() / 100.,
            margin[1] * projected.height() / 100.,
        )),
        Extent::Absolute => s.bounds.expanded(margin[0], margin[1]),
    };
    if !mapping.valid(domain) {
        return Err("Expanded colour domain must be finite and valid for the selected scales (log10 requires positive bounds)".into());
    }
    let grid_step = if s.grid_step.trim() == "auto" {
        None
    } else {
        let v = real(&s.grid_step, model)?;
        if v <= 0. {
            return Err("Grid spacing must be positive, or 'auto'".into());
        }
        Some(v)
    };
    let grid_y_step = match s.grid_y_step.trim() {
        "same" => grid_step,
        "auto" => None,
        source => {
            let value = real(source, model)?;
            if value <= 0. {
                return Err("Y grid spacing must be positive, 'auto', or 'same'".into());
            }
            Some(value)
        }
    };
    let projected_domain = mapping.projected(domain);
    crate::appearance::image_size(
        s.resolution,
        projected_domain.height() / projected_domain.width(),
    )?;
    if !s.guides.valid() {
        return Err("Invalid grid or guide appearance".into());
    }
    if !s.colour_style.valid() {
        return Err("Invalid domain-colour settings".into());
    }
    for style in [s.input_line, s.output_line] {
        if !style.width.is_finite()
            || !(0.2..=12.).contains(&style.width)
            || style
                .color
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Err("Invalid line appearance".into());
        }
    }
    if [s.input_opacity, s.line_opacity, s.color_opacity]
        .iter()
        .any(|v| !(0. ..=1.).contains(v))
    {
        return Err("Opacity must be between 0 and 1".into());
    }
    if s.additional_paths.len() > 31 {
        return Err("At most 32 input paths are supported".into());
    }
    let mut paths = Vec::new();
    let mut path_errors = std::collections::BTreeMap::new();
    for (index, path) in s.paths().enumerate().filter(|(_, p)| p.enabled) {
        match path_plan(index, path, s.mode, model) {
            Ok(plan) => paths.push(plan),
            Err(e) => {
                path_errors.insert(index, e);
            }
        }
    }
    Ok(Plan {
        paths,
        path_errors,
        domain,
        grid_step,
        grid_y_step,
    })
}
fn path_plan(
    index: usize,
    s: &InputPath,
    mode: Mode,
    model: &Compiled,
) -> Result<PathPlan, String> {
    let kind = s.kind(mode);
    let count = if kind == InputKind::Point {
        1.
    } else {
        real(&s.samples, model)?
    };
    if kind != InputKind::Point && (count.fract() != 0. || !(16. ..=100_000.).contains(&count)) {
        return Err("Samples must be an integer from 16 to 100000".into());
    }
    let interval = if kind == InputKind::Interval {
        [real(&s.interval[0], model)?, real(&s.interval[1], model)?]
    } else {
        [0., 1.]
    };
    if kind == InputKind::Interval && interval[0] >= interval[1] {
        return Err("Input interval must have start < end".into());
    }
    let extra = if kind == InputKind::Point {
        0.
    } else {
        real(&s.extrapolation, model)? / 100.
    };
    if !(0. ..=10.).contains(&extra) {
        return Err("Extrapolation must be between 0 and 1000%".into());
    }
    let mut points = Vec::new();
    let mut weights = Vec::new();
    if kind != InputKind::Interval {
        let drafts = s.drafts();
        if drafts.len() > 64 {
            return Err("An input path supports at most 64 control points".into());
        }
        for (i, p) in drafts
            .iter()
            .take(if kind == InputKind::Point { 1 } else { 64 })
            .enumerate()
        {
            let point = [
                real(&p[0], model)?,
                if mode == Mode::Complex {
                    real(&p[1], model)?
                } else {
                    0.
                },
            ];
            if point.iter().any(|v| !v.is_finite() || v.abs() > 1e14) {
                return Err("Control point coordinates must be finite and below 1e14".into());
            }
            points.push(point);
            let w = if kind == InputKind::Curve && s.curve == Curve::Nurbs {
                real(
                    s.curve_weights
                        .get(i)
                        .ok_or("Each NURBS point needs a weight")?,
                    model,
                )?
            } else {
                1.
            };
            if w <= 0. || w > 1e12 {
                return Err("NURBS weights must be positive and at most 1e12".into());
            }
            weights.push(w);
        }
    }
    let count = if (kind == InputKind::Point && points.is_empty())
        || (kind == InputKind::Curve && points.len() < 2)
    {
        0
    } else {
        count as usize
    };
    Ok(PathPlan {
        path: index,
        kind,
        curve: s.curve,
        count,
        interval,
        extra,
        points,
        curve_weights: weights,
    })
}
/// Keep expensive sampling off the event loop; include the cost of every enabled path.
pub fn fits_inline(model: &Compiled, _s: &Settings, p: &Plan) -> bool {
    p.paths
        .iter()
        .map(|p| {
            let cost = if p.kind == InputKind::Curve {
                match p.curve {
                    Curve::Bezier => p.points.len().pow(2),
                    Curve::Nurbs => 16,
                    Curve::Spline => 4,
                    Curve::Line => 2,
                }
            } else {
                0
            };
            p.count.saturating_mul(model.nodes.len() + cost)
        })
        .sum::<usize>()
        <= 65_536
}

/// Small curves are cheaper to evaluate inline than dispatch through a worker pool.
pub fn sample_line(model: &Compiled, s: &Settings, p: &Plan) -> RenderData {
    line(model, s, p, || false).expect("inline sampling cannot be cancelled")
}
fn line(
    model: &Compiled,
    s: &Settings,
    p: &Plan,
    cancelled: impl Fn() -> bool + Sync,
) -> Option<RenderData> {
    let start = Instant::now();
    let mut all_samples = Vec::with_capacity(p.paths.iter().map(|p| p.count).sum());
    for path in &p.paths {
        let count = path.count;
        let step = (1. + 2. * path.extra) / count.saturating_sub(1).max(1) as f64;
        let mut samples = vec![
            Sample {
                path: path.path,
                input: C::default(),
                output: None,
                extrapolated: false
            };
            count
        ];
        let fill = |chunk: &mut [Sample], base: usize| {
            if cancelled() {
                return;
            }
            let mut inputs = [C::default(); 128];
            let mut outputs = [None; 128];
            for (i, sample) in chunk.iter_mut().enumerate() {
                let t = -path.extra + step * (base + i) as f64;
                sample.input = path.input(t, s.mode);
                sample.extrapolated = !(0. ..=1.).contains(&t);
                inputs[i] = sample.input;
            }
            if cancelled() {
                return;
            }
            model.evaluate_many(&inputs[..chunk.len()], &mut outputs[..chunk.len()]);
            for (sample, output) in chunk.iter_mut().zip(outputs) {
                sample.output = output;
            }
        };
        if count * model.nodes.len() > 262_144 {
            samples
                .par_chunks_mut(128)
                .enumerate()
                .for_each(|(i, c)| fill(c, i * 128));
        } else {
            for (i, c) in samples.chunks_mut(128).enumerate() {
                fill(c, i * 128);
            }
        }
        if cancelled() {
            return None;
        }
        all_samples.extend(samples);
    }
    let samples = all_samples;
    if cancelled() {
        return None;
    }
    let missing = samples.iter().filter(|v| v.output.is_none()).count();
    Some(RenderData {
        samples: samples.into(),
        image: None,
        domain: p.domain,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.,
        missing,
    })
}

pub fn generate(
    model: &Compiled,
    s: &Settings,
    p: &Plan,
    generation: u64,
    latest: &Arc<AtomicU64>,
    line_ready: impl FnOnce(RenderData),
) -> Option<RenderData> {
    let start = Instant::now();
    let cancelled = || latest.load(Ordering::Relaxed) != generation;
    if cancelled() {
        return None;
    }
    let data = line(model, s, p, cancelled)?;
    line_ready(data.clone());
    let samples = data.samples;
    let image = if s.coloring_enabled() {
        let mapping = s.guides.mapping;
        let projected = mapping.projected(p.domain);
        let (width, height) =
            crate::appearance::image_size(s.resolution, projected.height() / projected.width())
                .ok()?;
        let shader = crate::appearance::Shader::new(s.colour_style);
        let x_coordinates = (mapping.x != crate::axes::Scale::Linear).then(|| {
            (0..width)
                .map(|x| {
                    mapping.x.inverse(
                        projected.x_min + (x as f64 + 0.5) / width as f64 * projected.width(),
                        mapping.threshold,
                    )
                })
                .collect::<Vec<_>>()
        });
        let mut rgba = vec![0_u8; width * height * 4];
        rgba.par_chunks_mut(width * 4).enumerate().for_each_init(
            || {
                (
                    vec![None; width],
                    vec![C::default(); if x_coordinates.is_some() { width } else { 0 }],
                )
            },
            |(values, inputs), (y, row)| {
                if cancelled() {
                    return;
                }
                let im = mapping.y.inverse(
                    projected.y_max - (y as f64 + 0.5) / height as f64 * projected.height(),
                    mapping.threshold,
                );
                if let Some(xs) = &x_coordinates {
                    for (z, &re) in inputs.iter_mut().zip(xs) {
                        *z = C::new(re, im);
                    }
                    model.evaluate_many(inputs, values);
                } else {
                    model.evaluate_row(p.domain.x_min, p.domain.width() / width as f64, im, values);
                }
                for (pixel, value) in row.as_chunks_mut::<4>().0.iter_mut().zip(values.iter()) {
                    pixel.copy_from_slice(&shader.color(*value));
                }
            },
        );
        if cancelled() {
            return None;
        }
        Some(iced::widget::image::Handle::from_rgba(
            width as u32,
            height as u32,
            rgba,
        ))
    } else {
        None
    };
    let missing = samples.iter().filter(|v| v.output.is_none()).count();
    Some(RenderData {
        samples,
        image,
        domain: p.domain,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.,
        missing,
    })
}
pub fn curve_point(points: &[[f64; 2]], weights: &[f64], kind: Curve, t: f64) -> C {
    let mut buffer = [C::new(0., 0.); 64];
    let len = points.len().min(buffer.len());
    for (z, p) in buffer.iter_mut().zip(points) {
        *z = C::new(p[0], p[1]);
    }
    let p = &buffer[..len];
    if p.len() < 2 {
        return p.first().copied().unwrap_or_default();
    }
    match kind {
        Curve::Bezier => {
            let mut work = buffer;
            let q = &mut work[..len];
            for k in (1..q.len()).rev() {
                for i in 0..k {
                    q[i] = q[i] * (1. - t) + q[i + 1] * t;
                }
            }
            q[0]
        }
        Curve::Line => {
            let u = t * (p.len() - 1) as f64;
            let i = (u.floor().max(0.) as usize).min(p.len() - 2);
            p[i] + (p[i + 1] - p[i]) * (u - i as f64)
        }
        Curve::Spline => {
            let u = t * (p.len() - 1) as f64;
            let i = (u.floor().max(0.) as usize).min(p.len() - 2);
            let v = u - i as f64;
            let a = if i == 0 { p[0] * 2. - p[1] } else { p[i - 1] };
            let b = p[i];
            let c = p[i + 1];
            let d = if i + 2 >= p.len() {
                c * 2. - b
            } else {
                p[i + 2]
            };
            (b * 2.
                + (c - a) * v
                + (a * 2. - b * 5. + c * 4. - d) * v * v
                + (-a + b * 3. - c * 3. + d) * v * v * v)
                * 0.5
        }
        Curve::Nurbs => {
            if t < 0. {
                let a = nurbs(p, weights, 0.);
                return a + (nurbs(p, weights, 1e-5) - a) * (t / 1e-5);
            }
            if t > 1. {
                let b = nurbs(p, weights, 1.);
                return b + (b - nurbs(p, weights, 1. - 1e-5)) * ((t - 1.) / 1e-5);
            }
            nurbs(p, weights, t)
        }
    }
}
fn nurbs(p: &[C], weights: &[f64], t: f64) -> C {
    let degree = 3.min(p.len() - 1);
    let n = p.len();
    let knot = |i: usize| {
        if i <= degree {
            0.
        } else if i >= n {
            1.
        } else {
            (i - degree) as f64 / (n - degree) as f64
        }
    };
    let span = if t >= 1. {
        n - 1
    } else {
        (degree..n)
            .find(|&i| t >= knot(i) && t < knot(i + 1))
            .unwrap_or(degree)
    };
    let mut d = [(C::new(0., 0.), 0.); 4];
    for (j, v) in d.iter_mut().take(degree + 1).enumerate() {
        let i = span - degree + j;
        let w = weights.get(i).copied().unwrap_or(1.);
        *v = (p[i] * w, w);
    }
    for r in 1..=degree {
        for j in (r..=degree).rev() {
            let i = span - degree + j;
            let den = knot(i + degree - r + 1) - knot(i);
            let a = if den == 0. { 0. } else { (t - knot(i)) / den };
            d[j] = (
                d[j - 1].0 * (1. - a) + d[j].0 * a,
                d[j - 1].1 * (1. - a) + d[j].1 * a,
            );
        }
    }
    d[degree].0 / d[degree].1
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod real_coloring_tests;

#[cfg(test)]
mod transformed_view_tests;
