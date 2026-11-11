//! Right-vector-only SVD solves and barycentric normalization.
use super::*;

pub(super) fn svd_parallelism(rows: usize, cols: usize) -> faer::Par {
    // Tiny QR/SVD steps dominate early AAA iterations. Spawning a whole pool
    // for those costs much more than the arithmetic. Scale up with dense work.
    let work = rows.saturating_mul(cols).saturating_mul(rows.min(cols));
    let workers = (work / 8_000_000).clamp(1, rayon::current_num_threads());
    if workers == 1 {
        faer::Par::Seq
    } else {
        faer::Par::rayon(workers)
    }
}
/// AAA only needs right singular vectors. Avoid constructing the tall U factor.
pub(super) fn right_svd<T: faer::traits::ComplexField>(
    matrix: &Mat<T>,
) -> Result<(faer::diag::Diag<T>, Mat<T>), String> {
    use faer::{
        dyn_stack::{MemBuffer, MemStack},
        linalg::svd::{self, ComputeSvdVectors},
    };
    let (rows, cols) = matrix.shape();
    let mut s = faer::diag::Diag::zeros(rows.min(cols));
    let mut v = Mat::zeros(cols, cols);
    let par = svd_parallelism(rows, cols);
    let mut memory = MemBuffer::new(svd::svd_scratch::<T>(
        rows,
        cols,
        ComputeSvdVectors::No,
        ComputeSvdVectors::Full,
        par,
        Default::default(),
    ));
    svd::svd(
        matrix.as_ref(),
        s.as_mut(),
        None,
        Some(v.as_mut()),
        par,
        MemStack::new(&mut memory),
        Default::default(),
    )
    .map_err(|e| format!("SVD failed: {e:?}"))?;
    Ok((s, v))
}
pub(super) fn vector(matrix: &Mat<C>, sign: bool) -> Result<(Vec<C>, bool), String> {
    let (rows, cols) = matrix.shape();
    if rows == 0 {
        return Ok((vec![C::new(1. / (cols as f64).sqrt(), 0.); cols], false));
    }
    let real = matrix.col_iter().all(|col| col.iter().all(|z| z.im == 0.));
    let (s, v): (Vec<f64>, Vec<C>) = if real {
        let a = Mat::from_fn(rows, cols, |i, j| matrix[(i, j)].re);
        let (s, v) = right_svd(&a)?;
        (
            (0..cols)
                .map(|i| if i < rows { s[i] } else { 0. })
                .collect(),
            (0..cols * cols)
                .map(|k| C::new(v[(k / cols, k % cols)], 0.))
                .collect(),
        )
    } else {
        let (s, v) = right_svd(matrix)?;
        (
            (0..cols)
                .map(|i| if i < rows { s[i].re } else { 0. })
                .collect(),
            (0..cols * cols).map(|k| v[(k / cols, k % cols)]).collect(),
        )
    };
    let min = s[cols - 1];
    let multiplicity = s.iter().filter(|&&x| x == min).count() as f64;
    let coefficients: Vec<_> = s
        .iter()
        .map(|&x| {
            if sign && min > 0. {
                (min / x).powi(2)
            } else if x == min {
                1. / multiplicity.sqrt()
            } else {
                0.
            }
        })
        .collect();
    let mut w: Vec<_> = (0..cols)
        .map(|i| {
            (0..cols)
                .map(|j| v[i * cols + j] * coefficients[j])
                .sum::<C>()
        })
        .collect();
    normalize(&mut w)?;
    let ill = s[0] > 0. && (min == 0. || s[0] / min > 1. / (3. * f64::EPSILON));
    Ok((w, ill))
}
pub(super) fn normalize(w: &mut [C]) -> Result<(), String> {
    let max = w.iter().map(|w| w.norm()).fold(0_f64, f64::max);
    if max == 0. || !max.is_finite() {
        return Err("Numerically degenerate barycentric weights".into());
    }
    for w in w {
        *w /= max;
    }
    Ok(())
}
pub(super) fn weights(
    samples: &[Sample],
    nodes: &[Node],
    span: f64,
    sign: bool,
    scaled: &mut bool,
    stop: impl Fn() -> bool,
) -> Result<Option<Vec<C>>, String> {
    if stop() {
        return Ok(None);
    }
    let remaining: Vec<_> = samples
        .iter()
        .filter(|s| nodes.iter().all(|n| n.z.re != s.x))
        .collect();
    let fscale = samples
        .iter()
        .map(|s| s.value.norm())
        .fold(0_f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let mut a = Mat::zeros(remaining.len(), nodes.len());
    a.par_col_iter_mut()
        .with_min_len(column_grain(remaining.len()))
        .enumerate()
        .for_each(|(j, mut col)| {
            for (i, sample) in remaining.iter().enumerate() {
                col[i] = (sample.value / fscale - nodes[j].value / fscale)
                    / ((sample.x - nodes[j].z.re) / span);
            }
        });
    if stop() {
        return Ok(None);
    }
    if a.col_iter().any(|col| col.iter().any(|&v| !finite(v))) {
        return Err("Loewner matrix exceeds the supported f64 range".into());
    }
    if !*scaled {
        let (ordinary, ill_conditioned) = vector(&a, sign)?;
        if stop() {
            return Ok(None);
        }
        if !ill_conditioned {
            return Ok(Some(ordinary));
        }
        *scaled = true;
    }
    // Current aaa.m enables diagonal scaling only once the SVD is ill-conditioned.
    let scales: Vec<_> = (0..a.ncols())
        .into_par_iter()
        .with_min_len(column_grain(a.nrows()))
        .map(|j| {
            a.col(j)
                .iter()
                .fold(0_f64, |n, z| n.hypot(z.norm()))
                .max(f64::MIN_POSITIVE)
        })
        .collect();
    let grain = column_grain(a.nrows());
    a.par_col_iter_mut()
        .with_min_len(grain)
        .enumerate()
        .for_each(|(j, col)| {
            for value in col.iter_mut() {
                *value /= scales[j];
            }
        });
    if stop() {
        return Ok(None);
    }
    let (mut w, _) = vector(&a, sign)?;
    if stop() {
        return Ok(None);
    }
    for (w, scale) in w.iter_mut().zip(scales) {
        *w /= scale;
    }
    normalize(&mut w)?;
    Ok(Some(w))
}
