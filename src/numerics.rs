//! Background-only dense numerical work. Reference: Chebfun's current aaa.m (AAA, PRZ).
pub mod aaa;
pub mod filter;
pub mod worker;
use crate::model::{Compiled, Node};
use faer::{Mat, linalg::solvers::SolveLstsq};
use num_complex::Complex64 as C;
use serde::{Deserialize, Serialize};
use std::sync::{
    OnceLock,
    atomic::{AtomicU64, Ordering},
};

fn worker_count(cpus: usize) -> usize {
    cpus.saturating_sub(1).max(1)
}
/// Keep dense algebra and function sampling off the renderer's Rayon pool.
pub fn on_pool<R: Send>(work: impl FnOnce() -> R + Send) -> R {
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    let pool = POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count(
                std::thread::available_parallelism().map_or(1, usize::from),
            ))
            .thread_name(|i| format!("baryplot-numerics-{i}"))
            .build()
            .ok()
    });
    match pool {
        Some(pool) => pool.install(work),
        None => work(),
    }
}
pub fn cancelled(epoch: u64, latest: &AtomicU64) -> bool {
    latest.load(Ordering::Relaxed) != epoch
}
pub fn finite(z: C) -> bool {
    z.re.is_finite() && z.im.is_finite()
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalysisOptions {
    pub poles: bool,
    pub zeros: bool,
    pub residues: bool,
}
impl AnalysisOptions {
    pub fn enabled(self) -> bool {
        self.poles || self.zeros || self.residues
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Spectrum {
    pub poles: Vec<C>,
    pub zeros: Vec<C>,
    pub residues: Vec<Option<C>>,
    pub warnings: Vec<String>,
    pub elapsed_ms: f64,
    pub cleanup_metrics: Option<filter::Metrics>,
}
fn root_pencil(nodes: &[Node], zero: bool) -> Result<Vec<C>, String> {
    let m = nodes.len();
    if m < 2 {
        return Ok(Vec::new());
    }
    let center = nodes.iter().map(|n| n.z / m as f64).sum::<C>();
    let scale = nodes
        .iter()
        .map(|n| (n.z - center).norm())
        .fold(0_f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let wscale = nodes.iter().map(|n| n.weight.norm()).fold(0_f64, f64::max);
    let fscale = nodes
        .iter()
        .map(|n| n.value.norm())
        .fold(0_f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let a = Mat::from_fn(m + 1, m + 1, |i, j| {
        if i == 0 && j > 0 {
            let n = &nodes[j - 1];
            n.weight / wscale
                * if zero {
                    n.value / fscale
                } else {
                    C::new(1., 0.)
                }
        } else if j == 0 && i > 0 {
            C::new(1., 0.)
        } else if i == j && i > 0 {
            (nodes[i - 1].z - center) / scale
        } else {
            C::new(0., 0.)
        }
    });
    let b = Mat::from_fn(m + 1, m + 1, |i, j| {
        C::new(if i == j && i > 0 { 1. } else { 0. }, 0.)
    });
    let evd = a
        .generalized_eigen(&b)
        .map_err(|e| format!("Generalized eigenvalue calculation failed: {e:?}"))?;
    let mut roots = Vec::new();
    for i in 0..=m {
        let alpha = evd.S_a()[i];
        let beta = evd.S_b()[i];
        if beta != C::new(0., 0.) {
            let z = center + scale * (alpha / beta);
            if finite(z) {
                roots.push(z);
            }
        }
    }
    roots.sort_by(|a, b| a.re.total_cmp(&b.re).then_with(|| a.im.total_cmp(&b.im)));
    Ok(roots)
}
fn formula_residue(nodes: &[Node], p: C) -> Option<C> {
    let d = nodes
        .iter()
        .map(|n| (p - n.z).norm())
        .fold(f64::INFINITY, f64::min);
    if d == 0. {
        return None;
    }
    let wscale = nodes.iter().map(|n| n.weight.norm()).fold(0_f64, f64::max);
    let fscale = nodes
        .iter()
        .map(|n| n.value.norm())
        .fold(0_f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let (mut num, mut derivative) = (C::new(0., 0.), C::new(0., 0.));
    for n in nodes {
        let q = C::new(d, 0.) / (p - n.z);
        let w = n.weight / wscale;
        num += w * q * (n.value / fscale);
        derivative -= w * q * q;
    }
    let residue = (num / derivative) * d * fscale;
    finite(residue).then_some(residue)
}
fn least_squares_residues(
    model: &Compiled,
    poles: &[C],
    zeros: usize,
    reference: Option<&[[C; 2]]>,
) -> Option<Vec<C>> {
    if poles.is_empty() {
        return Some(Vec::new());
    }
    let center = model
        .nodes
        .iter()
        .map(|n| n.z / model.nodes.len() as f64)
        .sum::<C>();
    let scale = model
        .nodes
        .iter()
        .map(|n| (n.z - center).norm())
        .fold(0_f64, f64::max)
        .max(1.);
    let degree = zeros.saturating_sub(poles.len());
    let cols = degree + 1 + poles.len();
    let count = (4 * cols).max(64);
    let samples: Vec<_> = if let Some(reference) = reference {
        reference
            .iter()
            .map(|pair| ((pair[0] - center) / scale, pair[1]))
            .collect()
    } else {
        (0..count)
            .filter_map(|i| {
                let t = C::from_polar(
                    1.73,
                    std::f64::consts::TAU * (i as f64 + 0.37) / count as f64,
                );
                model.evaluate(center + scale * t).map(|f| (t, f))
            })
            .collect()
    };
    if samples.len() < cols {
        return None;
    }
    let fscale = samples
        .iter()
        .map(|(_, f)| f.norm())
        .fold(0_f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let a = Mat::from_fn(samples.len(), cols, |i, j| {
        if j <= degree {
            samples[i].0.powi(j as i32)
        } else {
            C::new(1., 0.) / (samples[i].0 - (poles[j - degree - 1] - center) / scale)
        }
    });
    let b = Mat::from_fn(samples.len(), 1, |i, _| samples[i].1 / fscale);
    let x = a.col_piv_qr().solve_lstsq(&b);
    let residues: Vec<_> = (0..poles.len())
        .map(|i| x[(i + degree + 1, 0)] * fscale * scale)
        .collect();
    residues.iter().copied().all(finite).then_some(residues)
}
#[cfg(test)]
pub fn analyze(
    model: &Compiled,
    options: AnalysisOptions,
    epoch: u64,
    latest: &AtomicU64,
) -> Result<Option<Spectrum>, String> {
    analyze_with_reference(model, options, epoch, latest, None)
}
pub fn analyze_with_reference(
    model: &Compiled,
    options: AnalysisOptions,
    epoch: u64,
    latest: &AtomicU64,
    reference: Option<&[[C; 2]]>,
) -> Result<Option<Spectrum>, String> {
    on_pool(|| analyze_impl(model, options, epoch, latest, reference))
}
fn analyze_impl(
    model: &Compiled,
    options: AnalysisOptions,
    epoch: u64,
    latest: &AtomicU64,
    reference: Option<&[[C; 2]]>,
) -> Result<Option<Spectrum>, String> {
    let start = std::time::Instant::now();
    let nodes: Vec<_> = model
        .nodes
        .iter()
        .filter(|n| n.weight != C::new(0., 0.))
        .cloned()
        .collect();
    let mut out = Spectrum::default();
    if nodes.iter().all(|n| n.value == nodes[0].value) {
        if nodes[0].value == C::new(0., 0.) {
            out.warnings
                .push("The zero function has no isolated zeros.".into());
        }
        return Ok(Some(out));
    }
    if cancelled(epoch, latest) {
        return Ok(None);
    }
    if options.poles || options.residues {
        out.poles = root_pencil(&nodes, false)?;
    }
    if cancelled(epoch, latest) {
        return Ok(None);
    }
    if options.zeros || options.residues {
        out.zeros = root_pencil(&nodes, true)?;
    }
    if cancelled(epoch, latest) {
        return Ok(None);
    }
    if options.residues {
        out.residues = out
            .poles
            .iter()
            .enumerate()
            .map(|(i, &p)| {
                let clustered = out
                    .poles
                    .iter()
                    .enumerate()
                    .any(|(j, q)| i != j && (p - q).norm() < 1e-6 * (1. + p.norm()));
                if clustered {
                    None
                } else {
                    formula_residue(&nodes, p)
                }
            })
            .collect();
        if out.residues.iter().all(Option::is_some) {
            if let Some(refined) =
                least_squares_residues(model, &out.poles, out.zeros.len(), reference)
            {
                out.residues = refined.into_iter().map(Some).collect();
            }
        } else {
            out.warnings.push(
                "Simple-pole residues are unavailable for repeated or tightly clustered poles."
                    .into(),
            );
        }
    }
    if cancelled(epoch, latest) {
        return Ok(None);
    }
    if options.residues {
        let fallback: Vec<_>;
        let samples = if let Some(samples) = reference {
            samples
        } else {
            fallback = nodes.iter().map(|n| [n.z, n.value]).collect();
            &fallback
        };
        out.cleanup_metrics = Some(filter::metrics(&out.poles, &out.zeros, samples));
    }
    out.elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
    Ok(Some(out))
}

#[cfg(test)]
mod tests;
