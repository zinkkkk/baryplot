//! Built-ins operate on complex values unless a function explicitly requires real arguments.
use num_complex::Complex64 as C;
pub const FUNCTION_HELP: &str = concat!(
    "Trig: sin cos tan asin acos atan sec csc cot\n",
    "Hyperbolic: sinh cosh tanh asinh acosh atanh sech csch coth\n",
    "Powers/logs: sqrt cbrt square pow exp exp2 exp10 expm1 ln log log2 log10 log1p\n",
    "Complex: complex polar cis re im real imag conj abs norm abs2 norm_sqr arg sign signum\n",
    "Numeric: floor ceil round trunc fract sinc lerp\n",
    "Real arguments: min max clamp hypot atan2 mod rem radians degrees\n",
    "Two arguments: complex(re,im), polar(r,angle), pow(a,b), min(a,b), max(a,b), ",
    "hypot(x,y), atan2(y,x), mod(a,b), rem(a,b)\n",
    "Three arguments: clamp(x,min,max), lerp(a,b,t). Angles use radians; log = ln.\n",
    "Remainder: a % b = rem(a,b), keeping the dividend sign. mod(a,b) is nonnegative. Real arguments only."
);
pub fn arity(name: &str) -> Option<usize> {
    Some(match name {
        "complex" | "polar" | "pow" | "min" | "max" | "hypot" | "atan2" | "mod" | "rem" => 2,
        "clamp" | "lerp" => 3,
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sec" | "csc" | "cot" | "sinh"
        | "cosh" | "tanh" | "asinh" | "acosh" | "atanh" | "sech" | "csch" | "coth" | "exp"
        | "exp2" | "exp10" | "expm1" | "ln" | "log" | "log2" | "log10" | "log1p" | "sqrt"
        | "cbrt" | "square" | "conj" | "abs" | "norm" | "abs2" | "norm_sqr" | "arg" | "re"
        | "real" | "im" | "imag" | "sign" | "signum" | "floor" | "ceil" | "round" | "trunc"
        | "fract" | "sinc" | "cis" | "radians" | "degrees" => 1,
        _ => return None,
    })
}
pub fn power(a: C, b: C) -> C {
    if b.im == 0. && b.re.fract() == 0. && b.re.abs() <= i32::MAX as f64 {
        a.powi(b.re as i32)
    } else {
        a.powc(b)
    }
}
fn real(name: &str, a: C) -> Result<f64, String> {
    if a.im.abs() <= 1e-12 {
        Ok(a.re)
    } else {
        Err(format!("{name} requires real arguments"))
    }
}
pub fn call(name: &str, args: &[C]) -> Result<C, String> {
    let count = arity(name).ok_or_else(|| format!("Unknown function '{name}'"))?;
    if count != args.len() {
        return Err(format!(
            "{name} expects {count} arguments, got {}",
            args.len()
        ));
    }
    let a = args[0];
    let one = C::new(1., 0.);
    let r = |x| C::new(x, 0.);
    Ok(match name {
        "sin" => a.sin(),
        "cos" => a.cos(),
        "tan" => a.tan(),
        "asin" => a.asin(),
        "acos" => a.acos(),
        "atan" => a.atan(),
        "sec" => one / a.cos(),
        "csc" => one / a.sin(),
        "cot" => one / a.tan(),
        "sinh" => a.sinh(),
        "cosh" => a.cosh(),
        "tanh" => a.tanh(),
        "asinh" => a.asinh(),
        "acosh" => a.acosh(),
        "atanh" => a.atanh(),
        "sech" => one / a.cosh(),
        "csch" => one / a.sinh(),
        "coth" => one / a.tanh(),
        "exp" => a.exp(),
        "exp2" => a.exp2(),
        "exp10" => a.expf(10.),
        "expm1" => C::new(
            a.re.exp_m1() * a.im.cos() - 2. * (a.im * 0.5).sin().powi(2),
            a.re.exp() * a.im.sin(),
        ),
        "ln" | "log" => a.ln(),
        "log2" => a.log2(),
        "log10" => a.log(10.),
        "log1p" => {
            if a.im == 0. && a.re > -1. {
                r(a.re.ln_1p())
            } else {
                (one + a).ln()
            }
        }
        "sqrt" => a.sqrt(),
        "cbrt" => {
            if a.im == 0. {
                r(a.re.cbrt())
            } else {
                a.cbrt()
            }
        }
        "square" => a * a,
        "conj" => a.conj(),
        "abs" | "norm" => r(a.norm()),
        "abs2" | "norm_sqr" => r(a.norm_sqr()),
        "arg" => r(a.arg()),
        "re" | "real" => r(a.re),
        "im" | "imag" => r(a.im),
        "sign" | "signum" => {
            if a == C::new(0., 0.) {
                a
            } else {
                a / a.norm()
            }
        }
        "floor" => C::new(a.re.floor(), a.im.floor()),
        "ceil" => C::new(a.re.ceil(), a.im.ceil()),
        "round" => C::new(a.re.round(), a.im.round()),
        "trunc" => C::new(a.re.trunc(), a.im.trunc()),
        "fract" => C::new(a.re.fract(), a.im.fract()),
        "sinc" => {
            if a.norm() < 1e-4 {
                one - a * a / 6. + a.powi(4) / 120.
            } else {
                a.sin() / a
            }
        }
        "cis" => (C::i() * a).exp(),
        "radians" => r(real(name, a)?.to_radians()),
        "degrees" => r(real(name, a)?.to_degrees()),
        "pow" => power(a, args[1]),
        "complex" => C::new(real(name, a)?, real(name, args[1])?),
        "polar" => C::from_polar(real(name, a)?, real(name, args[1])?),
        "min" => r(real(name, a)?.min(real(name, args[1])?)),
        "max" => r(real(name, a)?.max(real(name, args[1])?)),
        "hypot" => r(real(name, a)?.hypot(real(name, args[1])?)),
        "atan2" => r(real(name, a)?.atan2(real(name, args[1])?)),
        "mod" => r(real(name, a)?.rem_euclid(real(name, args[1])?)),
        "rem" => r(real(name, a)? % real(name, args[1])?),
        "clamp" => {
            let x = real(name, a)?;
            let lo = real(name, args[1])?;
            let hi = real(name, args[2])?;
            if lo > hi {
                return Err("clamp requires min <= max".into());
            }
            r(x.clamp(lo, hi))
        }
        "lerp" => a + (args[1] - a) * args[2],
        _ => unreachable!(),
    })
}
