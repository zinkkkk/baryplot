//! NL-AAA Algorithms 2–4 and §4.3, combined with local continuum sampling.
//! Mathematical reference: Ackermann, Balicki, Gugercin & Werner (2026),
//! "A refined nonlinear least-squares method for the rational approximation problem".
//! https://doi.org/10.1007/s40687-026-00668-1
//! Matrix factorizations use faer; function evaluation uses the shared Rust model.
use super::*;

pub(super) struct Refined {
    pub weights: Vec<C>,
    pub stalled: bool,
}
#[derive(Clone)]
struct Candidate {
    weights: Vec<C>,
    error: f64,
}
struct Problem<'a> {
    req: &'a Request,
    samples: &'a [Sample],
    nodes: &'a [Node],
    remaining: Vec<usize>,
    cauchy: Mat<C>,
    loewner: Mat<C>,
    scale: f64,
    epoch: u64,
    latest: &'a AtomicU64,
    progress: &'a mut dyn FnMut(Progress),
}
impl Problem<'_> {
    fn stopped(&self) -> bool {
        cancelled(self.epoch, self.latest)
    }
    fn report(&mut self, stage: RefinementStage, step: usize, total: usize) {
        (self.progress)(Progress::Refining {
            stage,
            step,
            total,
            nodes: self.nodes.len(),
            grid: self.samples.len(),
        });
    }
    fn candidate(&self, weights: Vec<C>) -> Option<Candidate> {
        if weights.iter().any(|&w| !finite(w)) {
            return None;
        }
        let mut nodes = self.nodes.to_vec();
        apply_weights(&mut nodes, weights.clone());
        let m = model(nodes).ok()?;
        let mut error = 0_f64;
        for &index in &self.remaining {
            let sample = self.samples[index];
            let value = m.evaluate(C::new(sample.x, 0.))?;
            error = error.hypot((value / self.scale - sample.value / self.scale).norm());
        }
        error.is_finite().then_some(Candidate { weights, error })
    }
    fn full_error(&self, weights: &[C]) -> f64 {
        let mut nodes = self.nodes.to_vec();
        apply_weights(&mut nodes, weights.to_vec());
        model(nodes).map_or(f64::INFINITY, |m| relative_l2(&m, self.samples))
    }
    /// Algorithm 2: first solve is ordinary Levy/AAA; subsequent rows are
    /// multiplied by 1/|C w|. Retain the best true rational L2 error.
    fn sk(&mut self) -> Option<Candidate> {
        let mut current: Option<Vec<C>> = None;
        let mut best: Option<Candidate> = None;
        for step in 0..self.req.sk_steps {
            self.report(RefinementStage::Sk, step + 1, self.req.sk_steps);
            if self.stopped() {
                break;
            }
            let mut a = self.loewner.clone();
            if let Some(w) = &current {
                let den: Vec<_> = (0..a.nrows())
                    .map(|i| {
                        (0..w.len())
                            .map(|j| self.cauchy[(i, j)] * w[j])
                            .sum::<C>()
                            .norm()
                    })
                    .collect();
                let min = den.iter().copied().fold(f64::INFINITY, f64::min);
                if min <= 0. || !min.is_finite() || den.iter().any(|d| !d.is_finite()) {
                    break;
                }
                // A common row factor cancels from the objective; this keeps
                // reciprocals finite while preserving the SK weighting exactly.
                a.par_col_iter_mut()
                    .with_min_len(column_grain(den.len()))
                    .for_each(|mut col| {
                        for (i, d) in den.iter().enumerate() {
                            col[i] *= min / d;
                        }
                    });
            }
            let Ok((w, _)) = vector(&a, false) else { break };
            if self.stopped() {
                break;
            }
            if let Some(candidate) = self.candidate(w.clone())
                && best.as_ref().is_none_or(|b| candidate.error < b.error)
            {
                best = Some(candidate);
            }
            let converged = current
                .as_ref()
                .is_some_and(|old| projective_change(old, &w) <= self.req.refinement_tolerance);
            current = Some(w);
            if converged {
                break;
            }
        }
        best
    }
    /// Eq. (37): J_rest w_rest = (f-r) - J_pivot, w_pivot=1.
    /// Using 1/d rather than 1/|d| changes only each row's unit complex phase.
    fn wf_step(&self, w: &[C], pivot: usize) -> Option<Vec<C>> {
        let gauge = w[pivot];
        if gauge == C::new(0., 0.) {
            return None;
        }
        let previous: Vec<_> = w.iter().map(|w| *w / gauge).collect();
        if previous.iter().any(|&v| !finite(v)) {
            return None;
        }
        let columns: Vec<_> = (0..w.len()).filter(|&j| j != pivot).collect();
        let mut a = Mat::zeros(self.remaining.len(), columns.len());
        let mut rhs = vec![C::new(0., 0.); self.remaining.len()];
        for (i, &index) in self.remaining.iter().enumerate() {
            if i % 128 == 0 && self.stopped() {
                return None;
            }
            let den = (0..w.len())
                .map(|j| self.cauchy[(i, j)] * previous[j])
                .sum::<C>();
            let num = (0..w.len())
                .map(|j| self.cauchy[(i, j)] * previous[j] * (self.nodes[j].value / self.scale))
                .sum::<C>();
            if den == C::new(0., 0.) || !finite(den) {
                return None;
            }
            let r = num / den;
            let jacobian =
                |j: usize| self.cauchy[(i, j)] / den * (self.nodes[j].value / self.scale - r);
            rhs[i] = self.samples[index].value / self.scale - r - jacobian(pivot);
            for (col, &j) in columns.iter().enumerate() {
                a[(i, col)] = jacobian(j);
            }
        }
        if self.stopped()
            || rhs.iter().any(|&v| !finite(v))
            || a.col_iter().any(|c| c.iter().any(|&v| !finite(v)))
        {
            return None;
        }
        let solution = least_squares(&a, &rhs).ok()?;
        if self.stopped() {
            return None;
        }
        let mut next = vec![C::new(0., 0.); w.len()];
        next[pivot] = C::new(1., 0.);
        for (j, value) in columns.into_iter().zip(solution) {
            next[j] = value;
        }
        normalize(&mut next).ok()?;
        Some(next)
    }
    /// Algorithm 3 includes the initialization in its best-error selection.
    fn wf(&mut self, initial: Candidate, steps: usize, stage: RefinementStage) -> Candidate {
        let mut current = initial.weights.clone();
        let pivot = if current[0] != C::new(0., 0.) {
            0
        } else {
            (0..current.len())
                .max_by(|&a, &b| current[a].norm().total_cmp(&current[b].norm()))
                .unwrap_or(0)
        };
        let mut best = initial;
        for step in 0..steps {
            self.report(stage, step + 1, steps);
            if self.stopped() {
                break;
            }
            let Some(next) = self.wf_step(&current, pivot) else {
                break;
            };
            let Some(candidate) = self.candidate(next.clone()) else {
                break;
            };
            if candidate.error < best.error {
                best = candidate;
            }
            let converged = projective_change(&current, &next) <= self.req.refinement_tolerance;
            current = next;
            if converged {
                break;
            }
        }
        best
    }
}
/// Phase/scale invariant weight change avoids treating an SVD sign flip as
/// nonconvergence. Both vectors are mapped to the same unit-norm gauge.
fn projective_change(a: &[C], b: &[C]) -> f64 {
    let an = a.iter().fold(0_f64, |n, z| n.hypot(z.norm()));
    let bn = b.iter().fold(0_f64, |n, z| n.hypot(z.norm()));
    if an == 0. || bn == 0. {
        return f64::INFINITY;
    }
    let dot = a
        .iter()
        .zip(b)
        .map(|(&a, &b)| (a / an).conj() * (b / bn))
        .sum::<C>();
    let phase = if dot.norm() > 0. {
        dot.conj() / dot.norm()
    } else {
        C::new(1., 0.)
    };
    a.iter().zip(b).fold(0_f64, |n, (&a, &b)| {
        n.hypot((a / an - b / bn * phase).norm())
    })
}
struct ThinFactors<T> {
    s: faer::diag::Diag<T>,
    u: Mat<T>,
    v: Mat<T>,
}
fn thin_svd<T: faer::traits::ComplexField>(a: &Mat<T>) -> Result<ThinFactors<T>, String> {
    use faer::{
        dyn_stack::{MemBuffer, MemStack},
        linalg::svd::{self, ComputeSvdVectors},
    };
    let (rows, cols) = a.shape();
    let rank = rows.min(cols);
    let mut s = faer::diag::Diag::zeros(rank);
    let mut u = Mat::zeros(rows, rank);
    let mut v = Mat::zeros(cols, rank);
    let par = svd_parallelism(rows, cols);
    let mut scratch = MemBuffer::new(svd::svd_scratch::<T>(
        rows,
        cols,
        ComputeSvdVectors::Thin,
        ComputeSvdVectors::Thin,
        par,
        Default::default(),
    ));
    svd::svd(
        a.as_ref(),
        s.as_mut(),
        Some(u.as_mut()),
        Some(v.as_mut()),
        par,
        MemStack::new(&mut scratch),
        Default::default(),
    )
    .map_err(|e| format!("Whitfield SVD failed: {e:?}"))?;
    Ok(ThinFactors { s, u, v })
}
/// Rank-revealing pseudoinverse action, matching MATLAB pinv's default cutoff.
/// Unlike the homogeneous SK solve, WF requires left-vector information too.
fn least_squares(a: &Mat<C>, b: &[C]) -> Result<Vec<C>, String> {
    let (rows, cols) = a.shape();
    if rows == 0 || cols == 0 {
        return Ok(vec![C::new(0., 0.); cols]);
    }
    let real =
        a.col_iter().all(|col| col.iter().all(|z| z.im == 0.)) && b.iter().all(|z| z.im == 0.);
    if real {
        let a = Mat::from_fn(rows, cols, |i, j| a[(i, j)].re);
        let svd = thin_svd(&a)?;
        let cutoff = rows.max(cols) as f64 * f64::EPSILON * svd.s[0];
        let mut out = vec![C::new(0., 0.); cols];
        for j in 0..rows.min(cols) {
            let s = svd.s[j];
            if s <= cutoff {
                continue;
            }
            let coefficient = (0..rows).map(|i| svd.u[(i, j)] * b[i].re).sum::<f64>() / s;
            for (i, value) in out.iter_mut().enumerate() {
                *value += svd.v[(i, j)] * coefficient;
            }
        }
        Ok(out)
    } else {
        let svd = thin_svd(a)?;
        let cutoff = rows.max(cols) as f64 * f64::EPSILON * svd.s[0].re;
        let mut out = vec![C::new(0., 0.); cols];
        for j in 0..rows.min(cols) {
            let s = svd.s[j].re;
            if s <= cutoff {
                continue;
            }
            let coefficient = (0..rows).map(|i| svd.u[(i, j)].conj() * b[i]).sum::<C>() / s;
            for (i, value) in out.iter_mut().enumerate() {
                *value += svd.v[(i, j)] * coefficient;
            }
        }
        Ok(out)
    }
}
fn polynomial_weights(nodes: &[Node]) -> Vec<C> {
    let logs: Vec<_> = nodes
        .iter()
        .enumerate()
        .map(|(j, n)| {
            let mut log = 0.;
            let mut sign = 1.;
            for (k, other) in nodes.iter().enumerate() {
                if j != k {
                    let d = n.z.re - other.z.re;
                    log -= d.abs().ln();
                    sign *= d.signum();
                }
            }
            (log, sign)
        })
        .collect();
    let max = logs
        .iter()
        .map(|(l, _)| *l)
        .fold(f64::NEG_INFINITY, f64::max);
    logs.iter()
        .map(|(l, sign)| C::new(sign * (l - max).exp(), 0.))
        .collect()
}
pub(super) fn refine(
    req: &Request,
    samples: &[Sample],
    nodes: &[Node],
    has_previous: bool,
    epoch: u64,
    latest: &AtomicU64,
    progress: &mut impl FnMut(Progress),
) -> Result<Option<Refined>, String> {
    if cancelled(epoch, latest) {
        return Ok(None);
    }
    if nodes.len() == 1 || nodes.iter().all(|n| n.value == nodes[0].value) {
        let mut weights = vec![C::new(0., 0.); nodes.len()];
        weights[0] = C::new(1., 0.);
        return Ok(Some(Refined {
            weights,
            stalled: false,
        }));
    }
    let remaining: Vec<_> = samples
        .iter()
        .enumerate()
        .filter(|(_, s)| nodes.iter().all(|n| n.z.re != s.x))
        .map(|(i, _)| i)
        .collect();
    let scale = samples
        .iter()
        .map(|s| s.value.norm())
        .fold(0_f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let cauchy = Mat::from_fn(remaining.len(), nodes.len(), |i, j| {
        C::new(
            (req.interval[1] - req.interval[0]) / (samples[remaining[i]].x - nodes[j].z.re),
            0.,
        )
    });
    let loewner = Mat::from_fn(remaining.len(), nodes.len(), |i, j| {
        cauchy[(i, j)] * (nodes[j].value / scale - samples[remaining[i]].value / scale)
    });
    if loewner
        .col_iter()
        .any(|col| col.iter().any(|&v| !finite(v)))
    {
        return Err("NL-AAA matrix exceeds the supported f64 range".into());
    }
    let mut problem = Problem {
        req,
        samples,
        nodes,
        remaining,
        cauchy,
        loewner,
        scale,
        epoch,
        latest,
        progress,
    };
    let baseline = problem.candidate(if has_previous {
        nodes.iter().map(|n| n.weight).collect()
    } else {
        polynomial_weights(nodes)
    });
    let sk = problem.sk();
    if problem.stopped() {
        return Ok(None);
    }
    // Algorithm 4's adaptive initialization compares best SK with a one-step
    // WF probe from the previous approximant, extended with a zero weight.
    let probe = baseline
        .clone()
        .filter(|_| has_previous)
        .map(|initial| problem.wf(initial, 1, RefinementStage::WhitfieldProbe));
    if problem.stopped() {
        return Ok(None);
    }
    let initial = match (sk, probe) {
        (Some(sk), Some(probe)) if sk.error < probe.error => sk,
        (_, Some(_)) => baseline.clone().expect("probe requires baseline"),
        (Some(sk), None) => sk,
        _ => baseline
            .clone()
            .ok_or("NL-AAA could not construct finite initial weights")?,
    };
    let mut best = problem.wf(initial, req.wf_steps, RefinementStage::Whitfield);
    if problem.stopped() {
        return Ok(None);
    }
    let mut stalled = has_previous
        && baseline
            .as_ref()
            .is_some_and(|old| best.error >= old.error * (1. - 32. * f64::EPSILON));
    if let Some(old) = baseline
        && (old.error < best.error
            || has_previous && problem.full_error(&best.weights) > problem.full_error(&old.weights))
    {
        best = old;
        stalled = has_previous;
    }
    // The paper also changes greedy selection when the newest support still
    // has exactly zero weight. Supports are sorted here, so identify the new
    // support by its incoming zero coefficient rather than its list position.
    stalled |= has_previous
        && nodes
            .iter()
            .zip(&best.weights)
            .any(|(node, &weight)| node.weight == C::new(0., 0.) && weight == C::new(0., 0.));
    Ok(Some(Refined {
        weights: best.weights,
        stalled,
    }))
}

#[cfg(test)]
mod tests;
