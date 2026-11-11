//! Cached function evaluation and local continuum/verification probes.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct Sample {
    pub(super) x: f64,
    pub(super) value: C,
}
/// Cache evaluations by exact coordinates, including rejected nonfinite values.
/// Unchanged gaps reuse their samples when a new support point splits another gap.
pub(super) struct Evaluator<'a> {
    request: &'a Request,
    epoch: u64,
    latest: &'a AtomicU64,
    pub(super) cache: std::collections::BTreeMap<u64, Option<C>>,
}
impl<'a> Evaluator<'a> {
    pub(super) fn new(request: &'a Request, epoch: u64, latest: &'a AtomicU64) -> Self {
        Self {
            request,
            epoch,
            latest,
            cache: Default::default(),
        }
    }
    pub(super) fn samples(
        &mut self,
        positions: &[f64],
        validation: bool,
        progress: &mut impl FnMut(Progress),
    ) -> Result<Option<Vec<Sample>>, String> {
        let key = |x: f64| if x == 0. { 0 } else { x.to_bits() };
        let mut missing: Vec<_> = positions
            .iter()
            .copied()
            .filter(|x| !self.cache.contains_key(&key(*x)))
            .collect();
        missing.sort_by(f64::total_cmp);
        missing.dedup();
        let mut completed = positions.len() - missing.len();
        progress(Progress::Sampling {
            completed,
            total: positions.len(),
            validation,
        });
        for batch in missing.chunks(512) {
            if cancelled(self.epoch, self.latest) {
                return Ok(None);
            }
            let request = self.request;
            let epoch = self.epoch;
            let latest = self.latest;
            let values: Vec<_> = batch
                .par_iter()
                .with_min_len(128)
                .map(|&x| {
                    if cancelled(epoch, latest) {
                        return Ok(None);
                    }
                    match request.value(x) {
                        Ok(value) => Ok(Some(value)),
                        Err(e) if e.contains("not finite") => Ok(None),
                        Err(e) => Err(format!("{}({x}): {e}", request.function)),
                    }
                })
                .collect();
            if cancelled(self.epoch, self.latest) {
                return Ok(None);
            }
            for (&x, value) in batch.iter().zip(values) {
                self.cache.insert(key(x), value?);
            }
            completed += batch.len();
            progress(Progress::Sampling {
                completed,
                total: positions.len(),
                validation,
            });
        }
        if cancelled(self.epoch, self.latest) {
            return Ok(None);
        }
        Ok(Some(
            positions
                .iter()
                .filter_map(|&x| {
                    self.cache
                        .get(&key(x))
                        .copied()
                        .flatten()
                        .map(|value| Sample { x, value })
                })
                .collect(),
        ))
    }
}
/// The continuum grid has p interior points per gap, not a global dense mesh.
pub(super) fn gap_positions(nodes: &[Node], interval: [f64; 2], p: usize) -> Vec<f64> {
    let mut anchors: Vec<_> = nodes.iter().map(|n| n.z.re).chain(interval).collect();
    anchors.sort_by(f64::total_cmp);
    anchors.dedup();
    let mut positions = anchors.clone();
    for pair in anchors.windows(2) {
        let [a, b] = [pair[0], pair[1]];
        for j in 1..=p {
            let x = a + (b - a) * (j as f64 / (p + 1) as f64);
            if x > a && x < b {
                positions.push(x);
            }
        }
    }
    positions.sort_by(f64::total_cmp);
    positions.dedup();
    positions
}
pub(super) fn verification_positions(nodes: &[Node], interval: [f64; 2]) -> Vec<f64> {
    let mut positions = gap_positions(nodes, interval, 30);
    // Extra off-grid probes reduce accidental agreement for oscillatory functions.
    for t in [-0.357_998_918_959_666, 0.036_785_641_195_074] {
        positions.push((interval[0] + interval[1]) * 0.5 + (interval[1] - interval[0]) * 0.5 * t);
    }
    positions.sort_by(f64::total_cmp);
    positions.dedup();
    positions
}
