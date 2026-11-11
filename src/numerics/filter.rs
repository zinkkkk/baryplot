//! Display-only spectral filters using the cleanup criteria in current aaa.m.
use super::*;
use crate::expression::Variables;
use std::fmt;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Locations {
    #[default]
    All,
    Real,
    Complex,
}
impl fmt::Display for Locations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::All => "All roots",
            Self::Real => "Real only",
            Self::Complex => "Complex only",
        })
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cleanup {
    #[default]
    Off,
    Method1,
    Method2,
}
impl fmt::Display for Cleanup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Off => "No cleanup filter",
            Self::Method1 => "Cleanup 1 · residue",
            Self::Method2 => "Cleanup 2 · pole/zero distance",
        })
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub locations: Locations,
    pub cleanup: Cleanup,
    pub tolerance: String,
    pub min_pole: String,
    pub max_pole: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            locations: Locations::All,
            cleanup: Cleanup::Off,
            tolerance: "1e-13".into(),
            min_pole: String::new(),
            max_pole: String::new(),
        }
    }
}
#[derive(Debug, Clone, Copy, Default)]
pub struct DisplayFilter {
    pub locations: Locations,
    pub cleanup: Cleanup,
    pub tolerance: f64,
    pub min_pole: Option<f64>,
    pub max_pole: Option<f64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    Tolerance,
    MinPole,
    MaxPole,
}
#[derive(Debug, Clone)]
pub enum Action {
    Locations(Locations),
    Cleanup(Cleanup),
    Text(Field, String),
    Frame,
}
impl DisplayFilter {
    pub fn parse(settings: &Settings, variables: &Variables) -> Result<Self, (Field, String)> {
        let real = |source: &str, field: Field| -> Result<f64, (Field, String)> {
            let z = crate::expression::evaluate(source, variables).map_err(|e| (field, e))?;
            if z.im.abs() > 1e-12 || z.re < 0. {
                Err((field, "Expected a nonnegative real value".into()))
            } else {
                Ok(z.re)
            }
        };
        let tolerance = real(&settings.tolerance, Field::Tolerance)?;
        if tolerance >= 1. {
            return Err((Field::Tolerance, "Cleanup tolerance must be below 1".into()));
        }
        let min_pole = if settings.min_pole.trim().is_empty() {
            None
        } else {
            Some(real(&settings.min_pole, Field::MinPole)?)
        };
        let max_pole = if settings.max_pole.trim().is_empty() {
            None
        } else {
            Some(real(&settings.max_pole, Field::MaxPole)?)
        };
        if let (Some(lo), Some(hi)) = (min_pole, max_pole)
            && lo > hi
        {
            return Err((
                Field::MaxPole,
                "Maximum pole magnitude must be at least the minimum".into(),
            ));
        }
        Ok(Self {
            locations: settings.locations,
            cleanup: settings.cleanup,
            tolerance,
            min_pole,
            max_pole,
        })
    }
    pub fn location(self, z: C) -> bool {
        match self.locations {
            Locations::All => true,
            Locations::Real => near_real(z),
            Locations::Complex => !near_real(z),
        }
    }
    pub fn cleanup_pole(self, s: &Spectrum, i: usize) -> bool {
        let Some(metrics) = &s.cleanup_metrics else {
            return false;
        };
        let Some(p) = s.poles.get(i) else {
            return false;
        };
        let Some(metric) = metrics.poles.get(i) else {
            return false;
        };
        let residue = s.residues.get(i).copied().flatten();
        match self.cleanup {
            Cleanup::Off => false,
            Cleanup::Method1 => residue.is_some_and(|r| {
                r.norm() / metric.distance < self.tolerance * metrics.geometric_mean
            }),
            Cleanup::Method2 => {
                let dz = metric.nearest_zero.map_or(1e100, |(_, d)| d);
                let ds = metric.distance;
                ds == 0.
                    || dz == 0.
                    || (metrics.real_samples
                        && p.im.abs() < f64::EPSILON
                        && p.re >= metrics.min_re
                        && p.re <= metrics.max_re)
                    || (metrics.unit_circle && (p.norm() - 1.).abs() < f64::EPSILON)
                    || (dz / ds < 1. && dz < (self.tolerance * self.tolerance).max(f64::EPSILON))
                    || (dz / ds < self.tolerance.sqrt()
                        && residue.is_some_and(|r|if metrics.real_values {r.re.abs()}else{r.norm()} < 8.*self.tolerance*metric.mean_abs_f_distance))
            }
        }
    }
    pub fn pole(self, s: &Spectrum, i: usize) -> bool {
        let z = s.poles[i];
        let magnitude = z.norm();
        self.location(z)
            && !self.cleanup_pole(s, i)
            && self.min_pole.is_none_or(|min| magnitude >= min)
            && self.max_pole.is_none_or(|max| magnitude <= max)
    }
    pub fn zero(self, s: &Spectrum, i: usize) -> bool {
        if !self.location(s.zeros[i]) {
            return false;
        }
        if self.cleanup == Cleanup::Off {
            return true;
        }
        !s.cleanup_metrics.as_ref().is_some_and(|m| {
            m.poles.iter().enumerate().any(|(j, p)| {
                p.nearest_zero.is_some_and(|(zero, _)| zero == i) && self.cleanup_pole(s, j)
            })
        })
    }
}
pub fn near_real(z: C) -> bool {
    z.im.abs() <= 256. * f64::EPSILON * (1. + z.re.abs())
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PoleMetric {
    pub distance: f64,
    pub nearest_zero: Option<(usize, f64)>,
    pub mean_abs_f_distance: f64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub geometric_mean: f64,
    pub real_samples: bool,
    pub real_values: bool,
    pub unit_circle: bool,
    pub min_re: f64,
    pub max_re: f64,
    pub poles: Vec<PoleMetric>,
}
pub(super) fn metrics(poles: &[C], zeros: &[C], samples: &[[C; 2]]) -> Metrics {
    let logs: Vec<_> = samples
        .iter()
        .map(|s| s[1].norm())
        .filter(|&f| f > 0.)
        .map(f64::ln)
        .collect();
    Metrics {
        geometric_mean: if logs.is_empty() {
            0.
        } else {
            (logs.iter().sum::<f64>() / logs.len() as f64).exp()
        },
        real_samples: samples.iter().all(|s| s[0].im == 0.),
        real_values: samples.iter().all(|s| s[1].im == 0.),
        unit_circle: samples.iter().all(|s| s[0].norm() == 1.),
        min_re: samples
            .iter()
            .map(|s| s[0].re)
            .fold(f64::INFINITY, f64::min),
        max_re: samples
            .iter()
            .map(|s| s[0].re)
            .fold(f64::NEG_INFINITY, f64::max),
        poles: poles
            .iter()
            .map(|&p| PoleMetric {
                distance: samples
                    .iter()
                    .map(|s| (p - s[0]).norm())
                    .fold(f64::INFINITY, f64::min),
                nearest_zero: zeros
                    .iter()
                    .enumerate()
                    .map(|(i, &z)| (i, (p - z).norm()))
                    .min_by(|a, b| a.1.total_cmp(&b.1)),
                mean_abs_f_distance: samples
                    .iter()
                    .map(|s| s[1].norm() * (p - s[0]).norm() / samples.len() as f64)
                    .sum(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests;
