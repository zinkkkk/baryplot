//! Saved vector appearance and a prepared, allocation-free domain-colour shader.
use num_complex::Complex64 as C;
use serde::{Deserialize, Serialize};
use std::fmt;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    Input,
    Output,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LineStyle {
    pub color: [f32; 3],
    pub width: f32,
    pub dashed: bool,
    pub extrapolation: bool,
    pub control_polygon: bool,
}
impl Default for LineStyle {
    fn default() -> Self {
        Self {
            color: [0.42, 0.88, 0.94],
            width: 2.8,
            dashed: false,
            extrapolation: true,
            control_polygon: true,
        }
    }
}
impl LineStyle {
    pub fn input() -> Self {
        Self {
            color: [1., 0.82, 0.5],
            width: 1.6,
            ..Self::default()
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Palette {
    #[default]
    Phase,
    Twilight,
    IceFire,
    Turbo,
    Viridis,
    Plasma,
    Inferno,
    Magma,
    Cividis,
    Spectral,
    Greys,
}
impl Palette {
    pub const ALL: [Self; 11] = [
        Self::Phase,
        Self::Twilight,
        Self::IceFire,
        Self::Turbo,
        Self::Viridis,
        Self::Plasma,
        Self::Inferno,
        Self::Magma,
        Self::Cividis,
        Self::Spectral,
        Self::Greys,
    ];
    pub fn sample(self, t: f64) -> [f32; 3] {
        let t = if t.is_finite() { t.clamp(0., 1.) } else { 0. };
        if self == Self::Phase {
            return hsv(t as f32 * 360., 0.78, 1.);
        }
        if matches!(self, Self::Twilight | Self::IceFire) {
            let stops: &[[f32; 3]] = if self == Self::Twilight {
                &[
                    [0.88, 0.86, 0.89],
                    [0.32, 0.42, 0.68],
                    [0.18, 0.07, 0.27],
                    [0.67, 0.27, 0.32],
                    [0.88, 0.86, 0.89],
                ]
            } else {
                &[
                    [0.85, 0.95, 1.],
                    [0.08, 0.42, 0.72],
                    [0.05, 0.06, 0.12],
                    [0.82, 0.23, 0.08],
                    [0.85, 0.95, 1.],
                ]
            };
            let u = t * (stops.len() - 1) as f64;
            let i = (u.floor() as usize).min(stops.len() - 2);
            let f = (u - i as f64) as f32;
            return std::array::from_fn(|j| stops[i][j] * (1. - f) + stops[i + 1][j] * f);
        }
        let map = match self {
            Self::Turbo => colorous::TURBO,
            Self::Viridis => colorous::VIRIDIS,
            Self::Plasma => colorous::PLASMA,
            Self::Inferno => colorous::INFERNO,
            Self::Magma => colorous::MAGMA,
            Self::Cividis => colorous::CIVIDIS,
            Self::Spectral => colorous::SPECTRAL,
            _ => colorous::GREYS,
        };
        let c = map.eval_continuous(if matches!(self, Self::Greys | Self::Spectral) {
            1. - t
        } else {
            t
        });
        [c.r as f32 / 255., c.g as f32 / 255., c.b as f32 / 255.]
    }
}
impl fmt::Display for Palette {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Phase => "Phase wheel · cyclic",
            Self::Twilight => "Twilight · cyclic",
            Self::IceFire => "Ice / fire · cyclic",
            Self::Turbo => "Turbo",
            Self::Viridis => "Viridis",
            Self::Plasma => "Plasma",
            Self::Inferno => "Inferno",
            Self::Magma => "Magma",
            Self::Cividis => "Cividis",
            Self::Spectral => "Spectral",
            Self::Greys => "Greys",
        })
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Contours {
    Off,
    Phase,
    Modulus,
    #[default]
    Both,
}
impl fmt::Display for Contours {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModulusScale {
    #[default]
    Logarithmic,
    Linear,
}
impl fmt::Display for ModulusScale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColourStyle {
    pub saturation: f64,
    pub shading_floor: f64,
    pub bands: bool,
    pub band_base: f64,
    pub band_floor: f64,
    pub band_strength: f64,
    pub band_exponent: f64,
    pub palette: Palette,
    pub reverse: bool,
    pub shading: bool,
    pub contours: Contours,
    pub phase_step: f64,
    pub phase_offset: f64,
    pub modulus_scale: ModulusScale,
    pub modulus_spacing: f64,
    pub contour_strength: f32,
    pub contour_width: f32,
}
impl Default for ColourStyle {
    fn default() -> Self {
        Self {
            saturation: 0.78,
            shading_floor: 0.35,
            bands: false,
            band_base: 2.,
            band_floor: 0.62,
            band_strength: 0.27,
            band_exponent: 0.5,
            palette: Palette::Phase,
            reverse: false,
            shading: true,
            contours: Contours::Both,
            phase_step: 30.,
            phase_offset: 0.,
            modulus_scale: ModulusScale::Logarithmic,
            modulus_spacing: 2.,
            contour_strength: 0.38,
            contour_width: 0.035,
        }
    }
}
impl ColourStyle {
    /// The original f64 phase wheel, saturation and octave brightness bands.
    pub fn classic() -> Self {
        Self {
            saturation: 0.72,
            shading_floor: 0.3,
            bands: true,
            contours: Contours::Off,
            ..Self::default()
        }
    }
    pub fn palette_colour(self, t: f64) -> [f32; 3] {
        if self.palette == Palette::Phase {
            return phase_rgb(t, self.saturation, 1.).map(|v| v as f32);
        }
        let rgb = self.palette.sample(t);
        if self.saturation == 0.78 {
            return rgb;
        }
        let [h, s, v] = rgb_to_hsv(rgb);
        hsv(h, (s * self.saturation as f32 / 0.78).clamp(0., 1.), v)
    }
    pub fn valid(&self) -> bool {
        (0. ..=1.).contains(&self.saturation)
            && (0. ..=1.).contains(&self.shading_floor)
            && self.band_base.is_finite()
            && self.band_base > 1.
            && (0. ..=1.).contains(&self.band_floor)
            && (0. ..=1.).contains(&self.band_strength)
            && self.band_floor + self.band_strength <= 1.0000001
            && (0.05..=8.).contains(&self.band_exponent)
            && self.phase_step.is_finite()
            && self.phase_step > 0.
            && self.phase_step <= 360.
            && self.phase_offset.is_finite()
            && self.modulus_spacing.is_finite()
            && match self.modulus_scale {
                ModulusScale::Logarithmic => self.modulus_spacing > 1.,
                ModulusScale::Linear => self.modulus_spacing > 0.,
            }
            && (0. ..=1.).contains(&self.contour_strength)
            && (0.001..=0.2).contains(&self.contour_width)
    }
}
pub struct Shader {
    style: ColourStyle,
    lut: [[f32; 3]; 1025],
    modulus_log: f64,
    band_log2: f64,
}
impl Shader {
    pub fn new(style: ColourStyle) -> Self {
        Self {
            style,
            lut: std::array::from_fn(|i| style.palette_colour(i as f64 / 1024.)),
            band_log2: style.band_base.log2(),
            modulus_log: style.modulus_spacing.ln(),
        }
    }
    pub fn color(&self, z: Option<C>) -> [u8; 4] {
        let Some(z) = z else {
            return [242, 242, 250, 255];
        };
        let magnitude = z.norm();
        if !magnitude.is_finite() {
            return [242, 242, 250, 255];
        }
        if magnitude < 1e-14 {
            return [9, 11, 18, 255];
        }
        let phase =
            (z.arg() / std::f64::consts::TAU - self.style.phase_offset / 360.).rem_euclid(1.);
        let t = if self.style.reverse {
            1. - phase
        } else {
            phase
        };
        let u = t * 1024.;
        let i = (u as usize).min(1023);
        let f = (u - i as f64) as f32;
        let mut brightness = if self.style.shading {
            self.style.shading_floor
                + (1. - self.style.shading_floor) * (magnitude * 2.).atan()
                    / std::f64::consts::FRAC_PI_2
        } else {
            1.
        };
        if self.style.bands {
            let ring = (magnitude.log2() / self.band_log2).rem_euclid(1.);
            let ramp = if self.style.band_exponent == 0.5 {
                ring.sqrt()
            } else {
                ring.powf(self.style.band_exponent)
            };
            brightness *= self.style.band_floor + self.style.band_strength * ramp;
        }
        let line = |level: f64| {
            let t = level.rem_euclid(1.);
            let distance = t.min(1. - t);
            (1. - distance / self.style.contour_width as f64).clamp(0., 1.) as f32
        };
        let phase_line = if matches!(self.style.contours, Contours::Phase | Contours::Both) {
            line(phase * 360. / self.style.phase_step)
        } else {
            0.
        };
        let modulus_line = if matches!(self.style.contours, Contours::Modulus | Contours::Both) {
            line(match self.style.modulus_scale {
                ModulusScale::Logarithmic => magnitude.ln() / self.modulus_log,
                ModulusScale::Linear => magnitude / self.style.modulus_spacing,
            })
        } else {
            0.
        };
        brightness *= 1. - f64::from(self.style.contour_strength * phase_line.max(modulus_line));
        // Keep phase HSV in f64, including brightness, to reproduce the original
        // shader exactly. Other palettes use a prepared LUT with no pixel allocations.
        let rgb = if self.style.palette == Palette::Phase {
            phase_rgb(t, self.style.saturation, brightness)
        } else {
            std::array::from_fn(|j| {
                f64::from(self.lut[i][j] * (1. - f) + self.lut[i + 1][j] * f) * brightness
            })
        };
        let rgb = rgb.map(|v| (v * 255.).clamp(0., 255.) as u8);
        [rgb[0], rgb[1], rgb[2], 255]
    }
}
fn phase_rgb(phase: f64, saturation: f64, value: f64) -> [f64; 3] {
    let h = phase.rem_euclid(1.) * 6.;
    let f = h - h.floor();
    let p = value * (1. - saturation);
    let q = value * (1. - f * saturation);
    let t = value * (1. - (1. - f) * saturation);
    match h as u32 % 6 {
        0 => [value, t, p],
        1 => [q, value, p],
        2 => [p, value, t],
        3 => [p, q, value],
        4 => [t, p, value],
        _ => [value, p, q],
    }
}
pub fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(360.) / 60.;
    let f = h - h.floor();
    let p = v * (1. - s);
    let q = v * (1. - f * s);
    let t = v * (1. - (1. - f) * s);
    match h as u32 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}
pub fn rgb_to_hsv(rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0. {
        0.
    } else if max == r {
        60. * ((g - b) / d).rem_euclid(6.)
    } else if max == g {
        60. * ((b - r) / d + 2.)
    } else {
        60. * ((r - g) / d + 4.)
    };
    [h, if max == 0. { 0. } else { d / max }, max]
}
pub const MAX_RESOLUTION: u32 = 16384;
pub const MAX_IMAGE_BYTES: usize = 256 * 1024 * 1024;
pub fn image_size(width: u32, aspect: f64) -> Result<(usize, usize), String> {
    if width == 0 || width > MAX_RESOLUTION {
        return Err(format!("Pixel width must be 1–{MAX_RESOLUTION}"));
    }
    let max_height = (MAX_IMAGE_BYTES / (width as usize * 4))
        .min(MAX_RESOLUTION as usize)
        .max(1);
    let height = (f64::from(width) * aspect)
        .round()
        .clamp(1., max_height as f64) as usize;
    Ok((width as usize, height))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod original_tests;
