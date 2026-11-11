//! Field diagnostics and independent, last-valid handle positions during an invalid edit.
use super::*;
use crate::expression::{self, Variables};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum InputKey {
    Parameter(usize, usize, usize),
    Name(usize),
    Color(usize),
    SpectrumFilter(crate::numerics::filter::Field),
    GlobalName(usize),
    GlobalParameters(usize),
    GlobalExpression(usize),
    Axis(usize),
    Setting(Field),
    PathSetting(usize, Field),
    Control(usize, usize, usize),
    ControlWeight(usize, usize),
}
fn real(source: &str, variables: &Variables) -> Result<f64, String> {
    let value = expression::evaluate(source, variables)?;
    if value.im.abs() > 1e-12 {
        Err("Expected a real component".into())
    } else {
        Ok(value.re)
    }
}
impl App {
    /// These values are only for editing handles. Invalid inputs never reach the function sampler.
    pub(super) fn refresh_invalid_inputs(&mut self) {
        let resolved = self.project.resolve_definitions();
        let variables = resolved.variables;
        let mut issues: BTreeMap<_, _> = resolved
            .issues
            .into_iter()
            .map(|(key, error)| {
                use model::DefinitionKey as K;
                (
                    match key {
                        K::GlobalName(i) => InputKey::GlobalName(i),
                        K::GlobalParameters(i) => InputKey::GlobalParameters(i),
                        K::GlobalExpression(i) => InputKey::GlobalExpression(i),
                        K::RowName(i) => InputKey::Name(i),
                        K::Parameter(i, j, k) => InputKey::Parameter(i, j, k),
                    },
                    error,
                )
            })
            .collect();
        let previous: BTreeMap<_, _> = self.points.iter().map(|n| (n.row, n)).collect();
        let mut points = Vec::new();
        let mode = self.project.settings.mode;
        for (i, entry) in self
            .project
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.enabled)
        {
            if model::parse_color(&entry.color).is_none() {
                issues.insert(InputKey::Color(i), "Use #RRGGBB".into());
            }
            let mut point = previous
                .get(&i)
                .map(|n| (**n).clone())
                .unwrap_or(model::Node {
                    row: i,
                    z: C::new(0., 0.),
                    value: C::new(0., 0.),
                    weight: C::new(1., 0.),
                });
            let mut parameters = [point.z, point.value, point.weight];
            for (j, p) in parameters.iter_mut().enumerate() {
                if let Some(re) = resolved.components[i][j][0] {
                    p.re = re;
                }
                if let Some(im) = resolved.components[i][j][1] {
                    p.im = im;
                }
                if mode == Mode::Real {
                    p.im = 0.;
                }
            }
            point.z = parameters[0];
            point.value = parameters[1];
            point.weight = parameters[2];
            points.push(point);
        }
        for (i, a) in points.iter().enumerate() {
            for b in &points[..i] {
                if a.z == b.z {
                    for row in [a.row, b.row] {
                        issues
                            .entry(InputKey::Parameter(row, 0, 0))
                            .or_insert_with(|| "Active nodes must be distinct".into());
                    }
                }
            }
        }
        if !points.is_empty() && points.iter().all(|p| p.weight == C::new(0., 0.)) {
            for p in &points {
                issues
                    .entry(InputKey::Parameter(p.row, 2, 0))
                    .or_insert_with(|| "At least one weight must be nonzero".into());
            }
        }
        let mut axes = [None; 4];
        for (i, source) in self.axis.iter().enumerate() {
            match real(source, &variables) {
                Ok(v) if v.abs() < 1e14 => axes[i] = Some(v),
                Ok(_) => {
                    issues.insert(
                        InputKey::Axis(i),
                        "Axis magnitude must be below 1e14".into(),
                    );
                }
                Err(e) => {
                    issues.insert(InputKey::Axis(i), e);
                }
            }
        }
        for (a, b) in [(0, 1), (2, 3)] {
            if let (Some(min), Some(max)) = (axes[a], axes[b])
                && {
                    let m = self.project.settings.guides.mapping;
                    let scale = if a == 0 { m.x } else { m.y };
                    let lo = scale.forward(min, m.threshold);
                    let hi = scale.forward(max, m.threshold);
                    !lo.is_finite() || !hi.is_finite() || hi - lo <= 1e-10
                }
            {
                for i in [a, b] {
                    issues.insert(
                        InputKey::Axis(i),
                        "Bounds must increase in this scale; log10 requires positive values".into(),
                    );
                }
            }
        }
        let s = &self.project.settings;
        let mut settings = vec![(
            InputKey::Setting(Field::GridStep),
            Field::GridStep,
            &s.grid_step,
        )];
        if s.coloring_enabled() {
            settings.extend([
                (
                    InputKey::Setting(Field::MarginX),
                    Field::MarginX,
                    &s.domain_margin[0],
                ),
                (
                    InputKey::Setting(Field::MarginY),
                    Field::MarginY,
                    &s.domain_margin[1],
                ),
            ]);
        }
        for (index, input) in s.paths().enumerate().filter(|(_, p)| p.enabled) {
            let kind = input.kind(mode);
            if kind != model::InputKind::Point {
                settings.extend([
                    (
                        InputKey::PathSetting(index, Field::Samples),
                        Field::Samples,
                        &input.samples,
                    ),
                    (
                        InputKey::PathSetting(index, Field::Extrapolation),
                        Field::Extrapolation,
                        &input.extrapolation,
                    ),
                ]);
            }
            if kind == model::InputKind::Interval {
                settings.extend([
                    (
                        InputKey::PathSetting(index, Field::Start),
                        Field::Start,
                        &input.interval[0],
                    ),
                    (
                        InputKey::PathSetting(index, Field::End),
                        Field::End,
                        &input.interval[1],
                    ),
                ]);
            }
            if kind == model::InputKind::Interval
                && let (Ok(start), Ok(end)) = (
                    real(&input.interval[0], &variables),
                    real(&input.interval[1], &variables),
                )
                && start >= end
            {
                for field in [Field::Start, Field::End] {
                    issues.insert(
                        InputKey::PathSetting(index, field),
                        "Input start must be below end".into(),
                    );
                }
            }
            if kind != model::InputKind::Interval {
                for (i, point) in input
                    .drafts()
                    .iter()
                    .take(if kind == model::InputKind::Point {
                        1
                    } else {
                        64
                    })
                    .enumerate()
                {
                    for (j, source) in point
                        .iter()
                        .take(if mode == Mode::Complex { 2 } else { 1 })
                        .enumerate()
                    {
                        if let Err(e) = real(source, &variables).and_then(|v| {
                            if v.abs() <= 1e14 {
                                Ok(v)
                            } else {
                                Err("Coordinate magnitude must be at most 1e14".into())
                            }
                        }) {
                            issues.insert(InputKey::Control(index, i, j), e);
                        }
                    }
                    if kind == model::InputKind::Curve
                        && input.curve == Curve::Nurbs
                        && let Some(source) = input.curve_weights.get(i)
                        && let Err(e) = real(source, &variables).and_then(|v| {
                            if v > 0. && v <= 1e12 {
                                Ok(v)
                            } else {
                                Err("NURBS weight must be positive and at most 1e12".into())
                            }
                        })
                    {
                        issues.insert(InputKey::ControlWeight(index, i), e);
                    }
                }
            }
        }
        for (input_key, key, source) in settings {
            if key == Field::GridStep && source.trim() == "auto" {
                continue;
            }
            let result = real(source, &variables).and_then(|v| {
                let valid = match key {
                    Field::Samples => v.fract() == 0. && (16. ..=100_000.).contains(&v),
                    Field::Extrapolation => (0. ..=1000.).contains(&v),
                    Field::GridStep => v > 0.,
                    Field::MarginX | Field::MarginY => (0. ..=1e6).contains(&v),
                    _ => true,
                };
                if valid {
                    Ok(v)
                } else {
                    Err("Value is outside the supported range".into())
                }
            });
            if let Err(e) = result {
                issues.insert(input_key, e);
            }
        }
        self.points = points;
        self.issues = issues;
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod dependency_tests;

#[cfg(test)]
mod definition_tests;
