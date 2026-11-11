//! Continuum AAA: refine samples in support-point gaps during one bounded run.
//! SVD weights, sign modification, Lawson and cleanup use current aaa.m formulas.
use super::*;
use crate::{
    expression::Variables,
    model::{Mode, Project},
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

mod linear;
mod nonlinear;
mod refinement;
mod sampling;
#[cfg(test)]
use linear::right_svd;
use linear::{normalize, svd_parallelism, vector, weights};
use refinement::finish;
#[cfg(test)]
use refinement::lawson;
use sampling::{Evaluator, Sample, gap_positions, verification_positions};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Method {
    #[default]
    Classic,
    Nonlinear,
}
impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Classic => "AAA",
            Self::Nonlinear => "NL-AAA · SK + Whitfield",
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub function: String,
    pub method: Method,
    pub sk_steps: String,
    pub wf_steps: String,
    pub refinement_tolerance: String,
    pub lawson_damping: String,
    pub interval: [String; 2],
    pub tolerance: String,
    pub max_nodes: String,
    pub degree: String,
    pub fixed_degree: bool,
    pub sign: bool,
    pub cleanup: bool,
    pub lawson: String,
    pub samples_per_gap: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            function: String::new(),
            method: Method::Classic,
            sk_steps: "20".into(),
            wf_steps: "20".into(),
            refinement_tolerance: "1e-4".into(),
            lawson_damping: "1".into(),
            interval: ["-1".into(), "1".into()],
            tolerance: "1e-13".into(),
            max_nodes: "100".into(),
            degree: "5".into(),
            fixed_degree: false,
            sign: false,
            cleanup: true,
            lawson: "0".into(),
            samples_per_gap: 3,
        }
    }
}
#[derive(Debug, Clone)]
pub struct Request {
    pub function: String,
    pub method: Method,
    pub sk_steps: usize,
    pub wf_steps: usize,
    pub refinement_tolerance: f64,
    pub lawson_damping: f64,
    pub variables: Variables,
    pub interval: [f64; 2],
    pub tolerance: f64,
    pub max_nodes: usize,
    pub fixed_degree: bool,
    pub sign: bool,
    pub cleanup: bool,
    pub lawson: usize,
    pub samples_per_gap: usize,
    pub mode: Mode,
}
impl Request {
    pub fn new(project: &Project) -> Result<Self, String> {
        let s = &project.fit;
        let variables = project.resolve_definitions().variables;
        let global = project
            .globals
            .iter()
            .find(|g| g.name == s.function && g.kind.is_function())
            .ok_or("Choose a function from Definitions")?;
        if crate::expression::parameters(&global.parameters)?.len() != 1 {
            return Err("AAA requires a function with one argument".into());
        }
        let real = |source: &str| -> Result<f64, String> {
            let v = crate::expression::evaluate(source, &variables)?;
            if v.im.abs() > 1e-12 {
                Err("Fit settings must be real".into())
            } else {
                Ok(v.re)
            }
        };
        let integer = |source: &str, lo: usize, hi: usize| -> Result<usize, String> {
            let n = real(source)?;
            if n.fract() != 0. || n < lo as f64 || n > hi as f64 {
                Err(format!("Expected an integer from {lo} to {hi}"))
            } else {
                Ok(n as usize)
            }
        };
        let interval = [
            real(&s.interval[0]).map_err(|e| format!("Interval start: {e}"))?,
            real(&s.interval[1]).map_err(|e| format!("Interval end: {e}"))?,
        ];
        if interval[0] >= interval[1]
            || interval.iter().any(|x| x.abs() >= 1e14)
            || (interval[1] - interval[0]) <= 1e-10
        {
            return Err(
                "Fit interval must increase with span above 1e-10 and coordinates below 1e14"
                    .into(),
            );
        }
        let tolerance = real(&s.tolerance).map_err(|e| format!("Relative tolerance: {e}"))?;
        if !(0. ..1.).contains(&tolerance) {
            return Err("Relative tolerance must be in [0,1); computations use f64".into());
        }
        let max_nodes = if s.fixed_degree {
            integer(&s.degree, 0, 255).map_err(|e| format!("Degree: {e}"))? + 1
        } else {
            integer(&s.max_nodes, 1, 256).map_err(|e| format!("Max support nodes: {e}"))?
        };
        let lawson = integer(&s.lawson, 0, 100).map_err(|e| format!("Lawson steps: {e}"))?;
        let lawson_damping = if lawson > 0 {
            let d = real(&s.lawson_damping).map_err(|e| format!("Lawson damping: {e}"))?;
            if !(0. ..=1.).contains(&d) {
                return Err("Lawson damping must be between 0 and 1 (1 = standard)".into());
            }
            d
        } else {
            1.
        };
        let (sk_steps, wf_steps, refinement_tolerance) = if s.method == Method::Nonlinear {
            let sk = integer(&s.sk_steps, 1, 100).map_err(|e| format!("SK iterations: {e}"))?;
            let wf =
                integer(&s.wf_steps, 1, 100).map_err(|e| format!("Whitfield iterations: {e}"))?;
            let t =
                real(&s.refinement_tolerance).map_err(|e| format!("Refinement tolerance: {e}"))?;
            if !(0. ..1.).contains(&t) {
                return Err("Refinement tolerance must be in [0,1)".into());
            }
            (sk, wf, t)
        } else {
            (20, 20, 1e-4)
        };
        if !(3..=30).contains(&s.samples_per_gap) {
            return Err("Samples per gap must be between 3 and 30".into());
        }
        Ok(Self {
            function: s.function.clone(),
            method: s.method,
            sk_steps,
            wf_steps,
            refinement_tolerance,
            lawson_damping,
            variables,
            interval,
            tolerance,
            max_nodes,
            fixed_degree: s.fixed_degree,
            sign: s.sign && s.method == Method::Classic,
            cleanup: s.cleanup,
            lawson,
            samples_per_gap: s.samples_per_gap as usize,
            mode: project.settings.mode,
        })
    }
    fn value(&self, x: f64) -> Result<C, String> {
        let mut z = self
            .variables
            .call_function(&self.function, &[C::new(x, 0.)])?;
        if self.mode == Mode::Real {
            if z.im.abs() > 1e-12 {
                return Err("Function returns complex values; select Complex mode".into());
            }
            z.im = 0.;
        }
        Ok(z)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub nodes: usize,
    pub grid: usize,
    pub error: f64,
    #[serde(default)]
    pub relative_l2: f64,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RefinementStage {
    Sk,
    WhitfieldProbe,
    Whitfield,
}
impl std::fmt::Display for RefinementStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Sk => "Sanathanan–Koerner",
            Self::WhitfieldProbe => "Whitfield initialization",
            Self::Whitfield => "Whitfield",
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Progress {
    Sampling {
        completed: usize,
        total: usize,
        validation: bool,
    },
    Solving {
        nodes: usize,
        grid: usize,
    },
    Iteration(Step),
    Refining {
        stage: RefinementStage,
        step: usize,
        total: usize,
        nodes: usize,
        grid: usize,
    },
    Cleanup {
        nodes: usize,
        grid: usize,
    },
    Lawson {
        step: usize,
        total: usize,
        nodes: usize,
        grid: usize,
    },
    Validating {
        nodes: usize,
        grid: usize,
    },
}
impl Progress {
    /// Iteration summaries share a phase with the solve, avoiding a flood of UI messages.
    pub fn phase(&self) -> u8 {
        match self {
            Self::Sampling {
                validation: false, ..
            } => 0,
            Self::Solving { .. } | Self::Iteration(_) | Self::Refining { .. } => 1,
            Self::Cleanup { .. } => 2,
            Self::Lawson { .. } => 3,
            Self::Sampling {
                validation: true, ..
            } => 4,
            Self::Validating { .. } => 5,
        }
    }
    pub fn description(&self) -> String {
        match self {
            Self::Sampling {
                completed,
                total,
                validation,
            } => format!(
                "{} · {completed} / {total} samples",
                if *validation {
                    "Sampling validation grid"
                } else {
                    "Sampling function"
                }
            ),
            Self::Solving { nodes, grid } => {
                format!("Solving weights · {nodes} support nodes · {grid} samples")
            }
            Self::Refining {
                stage,
                step,
                total,
                nodes,
                grid,
            } => {
                format!("NL-AAA · {stage} {step}/{total} · {nodes} support nodes · {grid} samples")
            }
            Self::Iteration(p) => format!(
                "Fitting · {} support nodes · {} samples · max error {:.3e}",
                p.nodes, p.grid, p.error
            ),
            Self::Cleanup { nodes, grid } => {
                format!("Checking pole–zero cleanup · {nodes} support nodes · {grid} samples")
            }
            Self::Lawson {
                step,
                total,
                nodes,
                grid,
            } => format!("Lawson step {step} / {total} · {nodes} support nodes · {grid} samples"),
            Self::Validating { nodes, grid } => {
                format!("Validating fit · {nodes} support nodes · {grid} samples")
            }
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fit {
    pub function: String,
    #[serde(default)]
    pub method: Method,
    #[serde(default)]
    pub relative_l2: f64,
    pub interval: [f64; 2],
    pub nodes: Vec<Node>,
    pub error: f64,
    pub relative_error: f64,
    pub converged: bool,
    pub grid: usize,
    pub evaluations: usize,
    pub validation_samples: usize,
    pub history: Vec<Step>,
    pub elapsed_ms: f64,
    pub note: String,
    pub reference: std::sync::Arc<[[C; 2]]>,
}
fn model(nodes: Vec<Node>) -> Result<Compiled, String> {
    Compiled::from_nodes(nodes, Variables::new())
}
fn column_grain(rows: usize) -> usize {
    (65_536 / rows.max(1)).max(1)
}
fn errors(model: &Compiled, samples: &[Sample]) -> Vec<f64> {
    samples
        .par_iter()
        .with_min_len((32_768 / model.nodes.len().max(1)).max(128))
        .map(|s| {
            model
                .evaluate(C::new(s.x, 0.))
                .map_or(f64::INFINITY, |r| (r - s.value).norm())
        })
        .collect()
}
fn worst(errors: &[f64]) -> (f64, Option<usize>) {
    errors
        .iter()
        .enumerate()
        .fold((0., None), |(worst, index), (i, &e)| {
            if e >= worst {
                (e, Some(i))
            } else {
                (worst, index)
            }
        })
}
fn error(model: &Compiled, samples: &[Sample]) -> (f64, Option<usize>) {
    worst(&errors(model, samples))
}
fn relative_l2(model: &Compiled, samples: &[Sample]) -> f64 {
    let scale = samples.iter().map(|s| s.value.norm()).fold(0_f64, f64::max);
    let scale = if scale > 0. { scale } else { 1. };
    let mut residual = 0_f64;
    let mut data = 0_f64;
    for sample in samples {
        let Some(value) = model.evaluate(C::new(sample.x, 0.)) else {
            return f64::INFINITY;
        };
        residual = residual.hypot((value / scale - sample.value / scale).norm());
        data = data.hypot((sample.value / scale).norm());
    }
    if data > 0. { residual / data } else { residual }
}
fn apply_weights(nodes: &mut [Node], w: Vec<C>) {
    for (n, w) in nodes.iter_mut().zip(w) {
        n.weight = w;
    }
}
pub fn fit(
    req: &Request,
    epoch: u64,
    latest: &AtomicU64,
    progress: impl FnMut(Progress) + Send,
) -> Result<Option<Fit>, String> {
    on_pool(|| fit_impl(req, epoch, latest, progress))
}
fn fit_impl(
    req: &Request,
    epoch: u64,
    latest: &AtomicU64,
    mut progress: impl FnMut(Progress),
) -> Result<Option<Fit>, String> {
    let start = std::time::Instant::now();
    let mut evaluator = Evaluator::new(req, epoch, latest);
    let ends = if req.max_nodes == 1 {
        vec![(req.interval[0] + req.interval[1]) * 0.5]
    } else {
        req.interval.to_vec()
    };
    let Some(initial) = evaluator.samples(&ends, false, &mut progress)? else {
        return Ok(None);
    };
    if initial.len() != ends.len() {
        return Err(
            "Function must be finite at the interval endpoints; move a singular endpoint inward"
                .into(),
        );
    }
    let mut nodes: Vec<_> = initial
        .iter()
        .enumerate()
        .map(|(row, s)| Node {
            row,
            z: C::new(s.x, 0.),
            value: s.value,
            weight: C::new(1., 0.),
        })
        .collect();
    let mut history = Vec::new();
    let mut scaled = false;
    let mut reason = "Support-node budget reached";
    let (mut m, training) = loop {
        if cancelled(epoch, latest) {
            return Ok(None);
        }
        // Continuum AAA uses more probes early, then three per support gap.
        let density = req
            .samples_per_gap
            .max(16_usize.saturating_sub(nodes.len()));
        let positions = gap_positions(&nodes, req.interval, density);
        let Some(samples) = evaluator.samples(&positions, false, &mut progress)? else {
            return Ok(None);
        };
        let samples = if req.method == Method::Nonlinear {
            // NL-AAA minimizes a rational L2 objective. Retain previously learned
            // probes so replacing a support gap cannot erase earlier constraints.
            let mut learned: Vec<_> = evaluator
                .cache
                .iter()
                .filter_map(|(&x, &value)| {
                    value.map(|value| Sample {
                        x: f64::from_bits(x),
                        value,
                    })
                })
                .collect();
            learned.sort_by(|a, b| a.x.total_cmp(&b.x));
            learned
        } else {
            samples
        };
        if samples.is_empty() {
            return Err("No finite function samples on this interval".into());
        }
        progress(Progress::Solving {
            nodes: nodes.len(),
            grid: samples.len(),
        });
        let (w, stalled) = if req.method == Method::Nonlinear {
            let Some(refined) = nonlinear::refine(
                req,
                &samples,
                &nodes,
                !history.is_empty(),
                epoch,
                latest,
                &mut progress,
            )?
            else {
                return Ok(None);
            };
            (refined.weights, refined.stalled)
        } else {
            let Some(w) = weights(
                &samples,
                &nodes,
                req.interval[1] - req.interval[0],
                req.sign,
                &mut scaled,
                || cancelled(epoch, latest),
            )?
            else {
                return Ok(None);
            };
            (w, false)
        };
        apply_weights(&mut nodes, w);
        let current = model(nodes.clone())?;
        let residuals = errors(&current, &samples);
        let maxerr = worst(&residuals).0;
        let scale = samples.iter().map(|s| s.value.norm()).fold(0_f64, f64::max);
        let step = Step {
            nodes: nodes.len(),
            grid: samples.len(),
            error: maxerr,
            relative_l2: relative_l2(&current, &samples),
        };
        history.push(step.clone());
        progress(Progress::Iteration(step));
        if cancelled(epoch, latest) {
            return Ok(None);
        }
        if nodes.len() >= req.max_nodes {
            break (current, samples);
        }
        let mut candidates = samples;
        let mut differences = residuals;
        if !req.fixed_degree && maxerr <= req.tolerance * scale {
            let Some(check) = evaluator.samples(
                &verification_positions(&nodes, req.interval),
                true,
                &mut progress,
            )?
            else {
                return Ok(None);
            };
            progress(Progress::Validating {
                nodes: nodes.len(),
                grid: check.len(),
            });
            if cancelled(epoch, latest) {
                return Ok(None);
            }
            let check_errors = errors(&current, &check);
            let check_scale = check.iter().map(|s| s.value.norm()).fold(scale, f64::max);
            if worst(&check_errors).0 <= req.tolerance * check_scale {
                reason = "Tolerance met";
                break (current, candidates);
            }
            candidates.extend(check);
            differences.extend(check_errors);
        }
        let relative_greedy = stalled
            && candidates
                .iter()
                .any(|s| s.value.norm() > 0. && nodes.iter().all(|n| n.z.re != s.x));
        let next = candidates
            .iter()
            .zip(&differences)
            .filter(|(s, _)| {
                nodes.iter().all(|n| n.z.re != s.x) && (!relative_greedy || s.value.norm() > 0.)
            })
            .max_by(|(sa, a), (sb, b)| {
                let score = |s: &Sample, e: f64| {
                    if relative_greedy {
                        e / s.value.norm()
                    } else {
                        e
                    }
                };
                score(sa, **a).total_cmp(&score(sb, **b))
            })
            .map(|(s, _)| *s);
        let Some(next) = next else {
            reason = "No distinct f64 sample positions remain";
            break (current, candidates);
        };
        nodes.push(Node {
            row: nodes.len(),
            z: C::new(next.x, 0.),
            value: next.value,
            weight: C::new(
                if req.method == Method::Nonlinear {
                    0.
                } else {
                    1.
                },
                0.,
            ),
        });
        // Keep support points ordered so only the split gap changes at fixed density.
        nodes.sort_by(|a, b| a.z.re.total_cmp(&b.z.re));
    };
    if cancelled(epoch, latest) {
        return Ok(None);
    }
    let Some(finished) = finish(req, &training, m, epoch, latest, &mut scaled, &mut progress)?
    else {
        return Ok(None);
    };
    m = finished;
    let Some(check) = evaluator.samples(
        &verification_positions(&m.nodes, req.interval),
        true,
        &mut progress,
    )?
    else {
        return Ok(None);
    };
    progress(Progress::Validating {
        nodes: m.nodes.len(),
        grid: check.len(),
    });
    if cancelled(epoch, latest) {
        return Ok(None);
    }
    let scale = check
        .iter()
        .chain(&training)
        .map(|s| s.value.norm())
        .fold(0_f64, f64::max);
    let maxerr = error(&m, &training).0.max(error(&m, &check).0);
    let converged = maxerr <= req.tolerance * scale;
    if cancelled(epoch, latest) {
        return Ok(None);
    }
    for (i, node) in m.nodes.iter_mut().enumerate() {
        node.row = i;
    }
    let relative_l2 = relative_l2(&m, &check);
    Ok(Some(Fit {
        function: req.function.clone(),
        method: req.method,
        relative_l2,
        interval: req.interval,
        nodes: m.nodes,
        error: maxerr,
        relative_error: if scale > 0. { maxerr / scale } else { maxerr },
        converged,
        grid: training.len(),
        validation_samples: check.len(),
        evaluations: evaluator.cache.len(),
        reference: training
            .iter()
            .map(|s| [C::new(s.x, 0.), s.value])
            .collect::<Vec<_>>()
            .into(),
        history,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.,
        note: if converged {
            "Tolerance met on adaptive and verification samples".into()
        } else {
            format!("{reason}; tolerance not met. Adjust settings and run again.")
        },
    }))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod nonlinear_fit_tests;

#[cfg(test)]
mod lawson_damping_tests;

#[cfg(test)]
mod numeric_body_tests;
