//! Optional Lawson reweighting and Froissart cleanup after support selection.
use super::*;

pub(super) fn finish(
    req: &Request,
    samples: &[Sample],
    mut m: Compiled,
    epoch: u64,
    latest: &AtomicU64,
    scaled: &mut bool,
    progress: &mut impl FnMut(Progress),
) -> Result<Option<Compiled>, String> {
    if req.lawson > 0 {
        m = lawson(req, samples, m, epoch, latest, progress)?;
    } else if req.cleanup && !req.fixed_degree && m.nodes.len() > 1 {
        if cancelled(epoch, latest) {
            return Ok(None);
        }
        progress(Progress::Cleanup {
            nodes: m.nodes.len(),
            grid: samples.len(),
        });
        if cancelled(epoch, latest) {
            return Ok(None);
        }
        let nodes: Vec<_> = m
            .nodes
            .iter()
            .filter(|n| n.weight != C::new(0., 0.))
            .cloned()
            .collect();
        m = model(nodes)?;
        let poles = super::super::root_pencil(&m.nodes, false)?;
        if cancelled(epoch, latest) {
            return Ok(None);
        }
        let logs: Vec<_> = samples
            .iter()
            .map(|s| s.value.norm())
            .filter(|&v| v > 0.)
            .map(f64::ln)
            .collect();
        let scale = if logs.is_empty() {
            0.
        } else {
            (logs.iter().sum::<f64>() / logs.len() as f64).exp()
        };
        let tol = if req.tolerance == 0. {
            1e-13
        } else {
            req.tolerance
        };
        let mut remove = Vec::new();
        for pole in poles {
            let distance = samples
                .iter()
                .map(|s| (pole - C::new(s.x, 0.)).norm())
                .fold(f64::INFINITY, f64::min);
            if let Some(res) = super::super::formula_residue(&m.nodes, pole)
                && res.norm() / distance < tol * scale
                && let Some((i, _)) =
                    m.nodes.iter().enumerate().min_by(|(_, a), (_, b)| {
                        (a.z - pole).norm().total_cmp(&(b.z - pole).norm())
                    })
            {
                remove.push(i);
            }
        }
        remove.sort_unstable();
        remove.dedup();
        if !remove.is_empty() && remove.len() < m.nodes.len() {
            let mut nodes: Vec<_> = m
                .nodes
                .into_iter()
                .enumerate()
                .filter(|(i, _)| !remove.contains(i))
                .map(|(_, n)| n)
                .collect();
            let w = if req.method == Method::Nonlinear {
                let Some(refined) =
                    nonlinear::refine(req, samples, &nodes, true, epoch, latest, progress)?
                else {
                    return Ok(None);
                };
                refined.weights
            } else {
                let Some(w) = weights(
                    samples,
                    &nodes,
                    req.interval[1] - req.interval[0],
                    req.sign,
                    scaled,
                    || cancelled(epoch, latest),
                )?
                else {
                    return Ok(None);
                };
                w
            };
            apply_weights(&mut nodes, w);
            m = model(nodes)?;
        }
    }
    Ok(Some(m))
}
pub(super) fn lawson(
    req: &Request,
    samples: &[Sample],
    initial: Compiled,
    epoch: u64,
    latest: &AtomicU64,
    progress: &mut impl FnMut(Progress),
) -> Result<Compiled, String> {
    let count = initial.nodes.len();
    let span = req.interval[1] - req.interval[0];
    let fscale = samples
        .iter()
        .map(|s| s.value.norm())
        .fold(0_f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let mut row_weights = vec![1_f64; samples.len()];
    let mut best = initial.clone();
    progress(Progress::Lawson {
        step: 1,
        total: req.lawson,
        nodes: count,
        grid: samples.len(),
    });
    if cancelled(epoch, latest) {
        return Ok(best);
    }
    let mut best_error = error(&best, samples).0;
    let support: Vec<_> = samples
        .iter()
        .map(|s| initial.nodes.iter().position(|n| n.z.re == s.x))
        .collect();
    let mut base = Mat::zeros(samples.len(), 2 * count);
    base.par_col_iter_mut()
        .with_min_len(column_grain(samples.len()))
        .enumerate()
        .for_each(|(j, mut col)| {
            let target = j / 2;
            for (i, s) in samples.iter().enumerate() {
                let c = if let Some(k) = support[i] {
                    if k == target { 1. } else { 0. }
                } else {
                    span / (s.x - initial.nodes[target].z.re)
                };
                col[i] = c * if j % 2 == 0 {
                    C::new(1., 0.)
                } else {
                    s.value / fscale
                };
            }
        });
    if base.col_iter().any(|col| col.iter().any(|&z| !finite(z))) {
        return Err("Lawson matrix exceeds the supported f64 range".into());
    }
    for step in 0..req.lawson {
        if cancelled(epoch, latest) {
            break;
        }
        progress(Progress::Lawson {
            step: step + 1,
            total: req.lawson,
            nodes: count,
            grid: samples.len(),
        });
        if cancelled(epoch, latest) {
            break;
        }
        let mut a = base.clone();
        a.par_col_iter_mut()
            .with_min_len(column_grain(samples.len()))
            .for_each(|mut col| {
                for (i, weight) in row_weights.iter().enumerate() {
                    col[i] *= weight.sqrt();
                }
            });
        if cancelled(epoch, latest) {
            break;
        }
        let (coefficients, _) = vector(&a, req.sign)?;
        if cancelled(epoch, latest) {
            break;
        }
        let mut nodes = initial.nodes.clone();
        let mut valid = true;
        for (i, n) in nodes.iter_mut().enumerate() {
            n.weight = coefficients[2 * i + 1];
            if n.weight == C::new(0., 0.) {
                valid = false;
                break;
            }
            n.value = -coefficients[2 * i] / n.weight * fscale;
            if !finite(n.value) {
                valid = false;
                break;
            }
        }
        if !valid {
            break;
        }
        let m = model(nodes)?;
        let (maxerr, _) = error(&m, samples);
        if maxerr < best_error {
            best_error = maxerr;
            best = m.clone();
        }
        if maxerr == 0. || !maxerr.is_finite() {
            break;
        }
        for (weight, s) in row_weights.iter_mut().zip(samples) {
            let e = m
                .evaluate(C::new(s.x, 0.))
                .map_or(maxerr, |r| (r - s.value).norm());
            *weight = (*weight * ((1. - req.lawson_damping) + req.lawson_damping * (e / maxerr)))
                .max(1e-300);
        }
        let max = row_weights.iter().copied().fold(0_f64, f64::max);
        for w in &mut row_weights {
            *w /= max;
        }
    }
    Ok(best)
}
