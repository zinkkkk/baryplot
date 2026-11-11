//! View transforms and saved guide appearance. Mathematical inputs remain in world coordinates.
use crate::model::Bounds;
use num_complex::Complex64 as C;
use serde::{Deserialize, Serialize};
use std::{fmt, ops::Deref};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scale {
    #[default]
    Linear,
    Log10,
    SymLog,
}
impl fmt::Display for Scale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Linear => "Linear",
            Self::Log10 => "Log10",
            Self::SymLog => "Symmetric log",
        })
    }
}
impl Scale {
    pub fn forward(self, x: f64, threshold: f64) -> f64 {
        match self {
            Self::Linear => x,
            Self::Log10 => {
                if x > 0. {
                    x.log10()
                } else {
                    f64::NAN
                }
            }
            Self::SymLog => {
                if x.abs() <= threshold {
                    x / threshold
                } else {
                    x.signum() * (1. + (x.abs() / threshold).log10())
                }
            }
        }
    }
    pub fn inverse(self, x: f64, threshold: f64) -> f64 {
        match self {
            Self::Linear => x,
            Self::Log10 => 10f64.powf(x),
            Self::SymLog => {
                if x.abs() <= 1. {
                    x * threshold
                } else {
                    x.signum() * threshold * 10f64.powf(x.abs() - 1.)
                }
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mapping {
    pub x: Scale,
    pub y: Scale,
    pub threshold: f64,
}
impl Default for Mapping {
    fn default() -> Self {
        Self {
            x: Scale::Linear,
            y: Scale::Linear,
            threshold: 1.,
        }
    }
}
impl Mapping {
    pub fn forward(self, z: C) -> C {
        C::new(
            self.x.forward(z.re, self.threshold),
            self.y.forward(z.im, self.threshold),
        )
    }
    pub fn inverse(self, z: C) -> C {
        C::new(
            self.x.inverse(z.re, self.threshold),
            self.y.inverse(z.im, self.threshold),
        )
    }
    pub fn projected(self, b: Bounds) -> Bounds {
        Bounds {
            x_min: self.x.forward(b.x_min, self.threshold),
            x_max: self.x.forward(b.x_max, self.threshold),
            y_min: self.y.forward(b.y_min, self.threshold),
            y_max: self.y.forward(b.y_max, self.threshold),
        }
    }
    pub fn unprojected(self, b: Bounds) -> Bounds {
        Bounds {
            x_min: self.x.inverse(b.x_min, self.threshold),
            x_max: self.x.inverse(b.x_max, self.threshold),
            y_min: self.y.inverse(b.y_min, self.threshold),
            y_max: self.y.inverse(b.y_max, self.threshold),
        }
    }
    pub fn valid(self, b: Bounds) -> bool {
        let p = self.projected(b);
        self.threshold.is_finite()
            && self.threshold > 0.
            && [b.x_min, b.x_max, b.y_min, b.y_max]
                .iter()
                .all(|v| v.is_finite() && v.abs() < 1e14)
            && [p.x_min, p.x_max, p.y_min, p.y_max]
                .iter()
                .all(|v| v.is_finite())
            && p.width() > 1e-10
            && p.height() > 1e-10
    }
    /// Select a useful positive interval when switching a signed viewport to log10.
    pub fn fit(self, mut b: Bounds) -> Bounds {
        for (scale, min, max) in [
            (self.x, &mut b.x_min, &mut b.x_max),
            (self.y, &mut b.y_min, &mut b.y_max),
        ] {
            if scale == Scale::Log10 && *min <= 0. {
                if *max > 0. {
                    *min = *max / 1000.;
                } else {
                    *min = 0.1;
                    *max = 10.;
                }
            }
        }
        b
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    pub bounds: Bounds,
    pub mapping: Mapping,
}
impl Projection {
    pub fn new(bounds: Bounds, mapping: Mapping) -> Self {
        Self { bounds, mapping }
    }
}
impl From<Bounds> for Projection {
    fn from(bounds: Bounds) -> Self {
        Self::new(bounds, Mapping::default())
    }
}
impl Deref for Projection {
    type Target = Bounds;
    fn deref(&self) -> &Bounds {
        &self.bounds
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guide {
    Circle,
    Major,
    Minor,
    Zero,
    Border,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StrokeStyle {
    pub color: [f32; 3],
    pub opacity: f32,
    pub width: f32,
    pub dashed: bool,
}
impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            color: [0.54, 0.64, 0.8],
            opacity: 0.28,
            width: 1.,
            dashed: false,
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelFormat {
    #[default]
    Auto,
    Fixed,
    Scientific,
}
impl fmt::Display for LabelFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AxisStyle {
    pub ticks: u16,
    pub subdivisions: u8,
    pub labels: bool,
    pub label_stride: u8,
}
impl Default for AxisStyle {
    fn default() -> Self {
        Self {
            ticks: 10,
            subdivisions: 5,
            labels: true,
            label_stride: 1,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Guides {
    pub mapping: Mapping,
    pub x: AxisStyle,
    pub y: AxisStyle,
    pub major: bool,
    pub minor: bool,
    pub zero: bool,
    pub border: bool,
    pub circle_style: StrokeStyle,
    pub major_style: StrokeStyle,
    pub minor_style: StrokeStyle,
    pub zero_style: StrokeStyle,
    pub border_style: StrokeStyle,
    pub circle_marks: bool,
    pub background: [f32; 3],
    pub label_color: [f32; 3],
    pub cursor_readout: bool,
    pub cursor_color: [f32; 3],
    pub label_format: LabelFormat,
    pub precision: u8,
}
impl Default for Guides {
    fn default() -> Self {
        Self {
            mapping: Mapping::default(),
            x: AxisStyle::default(),
            y: AxisStyle {
                ticks: 8,
                ..Default::default()
            },
            major: true,
            minor: false,
            zero: true,
            border: true,
            circle_marks: false,
            circle_style: StrokeStyle {
                color: [0.7, 0.77, 0.91],
                opacity: 0.65,
                width: 1.3,
                dashed: false,
            },
            major_style: StrokeStyle::default(),
            minor_style: StrokeStyle {
                opacity: 0.1,
                width: 0.6,
                ..Default::default()
            },
            zero_style: StrokeStyle {
                color: [0.6, 0.67, 0.78],
                opacity: 0.5,
                ..Default::default()
            },
            border_style: StrokeStyle {
                color: [0.23, 0.3, 0.4],
                opacity: 1.,
                ..Default::default()
            },
            background: [0.039, 0.055, 0.083],
            label_color: [0.46, 0.53, 0.64],
            cursor_readout: true,
            cursor_color: [0.46, 0.53, 0.64],
            label_format: LabelFormat::Auto,
            precision: 3,
        }
    }
}
impl Guides {
    pub fn valid(self) -> bool {
        let rgb = |c: [f32; 3]| {
            c.into_iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(&v))
        };
        rgb(self.background)
            && rgb(self.label_color)
            && rgb(self.cursor_color)
            && [
                self.circle_style,
                self.major_style,
                self.minor_style,
                self.zero_style,
                self.border_style,
            ]
            .into_iter()
            .all(|s| {
                rgb(s.color) && (0. ..=1.).contains(&s.opacity) && (0.2..=12.).contains(&s.width)
            })
            && [self.x, self.y].into_iter().all(|a| {
                (2..=250).contains(&a.ticks)
                    && (1..=20).contains(&a.subdivisions)
                    && (1..=250).contains(&a.label_stride)
            })
    }

    pub fn stroke(self, g: Guide) -> StrokeStyle {
        match g {
            Guide::Circle => self.circle_style,
            Guide::Major => self.major_style,
            Guide::Minor => self.minor_style,
            Guide::Zero => self.zero_style,
            Guide::Border => self.border_style,
        }
    }
    pub fn stroke_mut(&mut self, g: Guide) -> &mut StrokeStyle {
        match g {
            Guide::Circle => &mut self.circle_style,
            Guide::Major => &mut self.major_style,
            Guide::Minor => &mut self.minor_style,
            Guide::Zero => &mut self.zero_style,
            Guide::Border => &mut self.border_style,
        }
    }
    pub fn label(self, x: f64) -> String {
        let x = if x.abs() < f64::EPSILON { 0. } else { x };
        let p = usize::from(self.precision.min(10));
        match self.label_format {
            LabelFormat::Scientific => format!("{x:.p$e}"),
            LabelFormat::Fixed => format!("{x:.p$}"),
            LabelFormat::Auto => {
                if x != 0. && (x.abs() < 0.001 || x.abs() >= 10000.) {
                    format!("{x:.p$e}")
                } else {
                    let s = format!("{x:.p$}");
                    if p > 0 {
                        s.trim_end_matches('0').trim_end_matches('.').to_owned()
                    } else {
                        s
                    }
                }
            }
        }
    }
}
/// Major tick positions are in transformed coordinates. Manual spacing uses decades on log axes.
pub fn ticks(min: f64, max: f64, step: Option<f64>, target: u16) -> Vec<f64> {
    let step = step.unwrap_or_else(|| nice_step((max - min) / f64::from(target.max(2))));
    if !step.is_finite() || step <= 0. {
        return Vec::new();
    }
    let start = (min / step).ceil();
    let count = ((max - min) / step).ceil() as usize;
    if count > 250 {
        return Vec::new();
    }
    (0..=count)
        .map(|i| (start + i as f64) * step)
        .filter(|v| *v <= max)
        .collect()
}
pub fn nice_step(raw: f64) -> f64 {
    let power = 10f64.powf(raw.log10().floor());
    let n = raw / power;
    power
        * if n < 1.5 {
            1.
        } else if n < 3.5 {
            2.
        } else if n < 7.5 {
            5.
        } else {
            10.
        }
}
/// Preserve the current relative x/y scale as the plot's physical aspect changes.
pub fn resize_view(bounds: Bounds, mapping: Mapping, old: iced::Size, new: iced::Size) -> Bounds {
    if [old.width, old.height, new.width, new.height]
        .iter()
        .any(|x| !x.is_finite() || *x <= 0.)
    {
        return bounds;
    }
    let mut p = mapping.projected(bounds);
    let centre = (p.x_min + p.x_max) * 0.5;
    let factor = (f64::from(new.width) / f64::from(new.height))
        / (f64::from(old.width) / f64::from(old.height));
    let half = p.width() * factor * 0.5;
    p.x_min = centre - half;
    p.x_max = centre + half;
    let next = mapping.unprojected(p);
    if mapping.valid(next) { next } else { bounds }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod resize_tests;
