//! Runtime-dispatched f64 barycentric batches. Exceptional lanes use the scalar reference.
use super::{C, Compiled};
use fearless_simd::{Level, dispatch, prelude::*};
use std::sync::OnceLock;

fn level() -> Level {
    *LEVEL.get_or_init(Level::new)
}
static LEVEL: OnceLock<Level> = OnceLock::new();
impl Compiled {
    pub fn evaluate_many(&self, inputs: &[C], output: &mut [Option<C>]) {
        assert_eq!(inputs.len(), output.len());
        dispatch!(level(),simd=>evaluate(simd,self,|i|inputs[i],output));
    }
    pub fn evaluate_row(&self, x_min: f64, step: f64, im: f64, output: &mut [Option<C>]) {
        dispatch!(level(),simd=>evaluate(simd,self,|i|C::new(x_min+(i as f64+0.5)*step,im),output));
    }
}
// Inlining keeps all vector operations inside dispatch!'s target-feature boundary.
#[inline(always)]
fn evaluate<S: Simd>(
    simd: S,
    model: &Compiled,
    input: impl Fn(usize) -> C,
    output: &mut [Option<C>],
) {
    let lanes = S::f64s::LEN;
    let end = output.len() / lanes * lanes;
    for base in (0..end).step_by(lanes) {
        let x = S::f64s::from_fn(simd, |i| input(base + i).re);
        let y = S::f64s::from_fn(simd, |i| input(base + i).im);
        let zero = S::f64s::splat(simd, 0.);
        let mut nearest = S::f64s::splat(simd, f64::INFINITY);
        let mut farthest = zero;
        for n in &model.nodes {
            if n.weight == C::new(0., 0.) {
                continue;
            }
            let dx = x - n.z.re;
            let dy = y - n.z.im;
            let distance = dx * dx + dy * dy;
            nearest = nearest.min(distance);
            farthest = farthest.max(distance);
        }
        let distance = nearest.sqrt();
        let mut nr = zero;
        let mut ni = zero;
        let mut dr = zero;
        let mut di = zero;
        let mut total = zero;
        for (n, w) in model.nodes.iter().zip(&model.normalized_weights) {
            if n.weight == C::new(0., 0.) {
                continue;
            }
            let dx = x - n.z.re;
            let dy = y - n.z.im;
            let factor = distance / (dx * dx + dy * dy);
            let ar = dx * factor;
            let ai = -(dy * factor);
            let tr = ar * w.re - ai * w.im;
            let ti = ar * w.im + ai * w.re;
            nr += tr * n.value.re - ti * n.value.im;
            ni += tr * n.value.im + ti * n.value.re;
            dr += tr;
            di += ti;
            total += (tr * tr + ti * ti).sqrt();
        }
        let denominator = dr * dr + di * di;
        let re = (nr * dr + ni * di) / denominator;
        let im = (ni * dr - nr * di) / denominator;
        let magnitude = denominator.sqrt();
        for lane in 0..lanes {
            // Preserve exact-node interpolation, pole decisions and extreme-range behavior.
            // A generous cancellation guard keeps sensitive evaluations on the reference path.
            let exceptional = !(1e-280..=1e280).contains(&nearest[lane])
                || !farthest[lane].is_finite()
                || farthest[lane] > 1e280
                || magnitude[lane] <= total[lane] * 1e-8
                || !re[lane].is_finite()
                || !im[lane].is_finite();
            output[base + lane] = if exceptional {
                model.evaluate(input(base + lane))
            } else {
                Some(C::new(re[lane], im[lane]))
            };
        }
    }
    for (i, value) in output.iter_mut().enumerate().skip(end) {
        *value = model.evaluate(input(i));
    }
}

#[cfg(test)]
mod tests;
