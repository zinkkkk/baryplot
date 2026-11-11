mod batch;
mod definitions;
use crate::expression::Variables;
pub use definitions::{Definition, DefinitionKey, DefinitionKind, Definitions};
use num_complex::Complex64 as C;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Real,
    Complex,
}
impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Real => "Real",
            Self::Complex => "Complex",
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Curve {
    Line,
    Bezier,
    Spline,
    Nurbs,
}
impl fmt::Display for Curve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Line => "Polyline",
            Self::Bezier => "Bézier",
            Self::Spline => "Catmull–Rom",
            Self::Nurbs => "NURBS",
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Extent {
    Percent,
    Absolute,
}
impl fmt::Display for Extent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Percent => "% of view",
            Self::Absolute => "Axis units",
        })
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub expressions: [String; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imaginary: Option<[String; 3]>,
    pub locks: [[bool; 2]; 3],
    pub enabled: bool,
    pub visible: bool,
    pub color: String,
    #[serde(default)]
    pub real_y: [f64; 3],
}
impl Entry {
    pub fn new(index: usize) -> Self {
        let palette = [
            "#61D6C2", "#FFB86C", "#AB9DF2", "#FF7597", "#79B8FF", "#E7D77C",
        ];
        Self {
            name: format!("p{}", index + 1),
            expressions: [index.to_string(), "0".into(), "1".into()],
            imaginary: None,
            locks: [[false; 2]; 3],
            enabled: true,
            visible: true,
            color: palette[index % palette.len()].into(),
            real_y: [
                0.,
                1. + (index % 5) as f64 * 0.2,
                -1. - (index % 5) as f64 * 0.2,
            ],
        }
    }
    /// Preserve legacy combined expressions as independently editable component formulas.
    pub fn split_components(&mut self) {
        if self.imaginary.is_some() {
            return;
        }
        let mut imaginary = std::array::from_fn(|_| "0".to_owned());
        for (j, part) in imaginary.iter_mut().enumerate() {
            let source = &self.expressions[j];
            if source.trim().parse::<f64>().is_err() {
                *part = format!("im({source})");
                self.expressions[j] = format!("re({source})");
            }
        }
        self.imaginary = Some(imaginary);
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Bounds {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}
impl Default for Bounds {
    fn default() -> Self {
        Self {
            x_min: -4.,
            x_max: 4.,
            y_min: -3.,
            y_max: 3.,
        }
    }
}
impl Bounds {
    pub fn valid(self) -> bool {
        [self.x_min, self.x_max, self.y_min, self.y_max]
            .iter()
            .all(|v| v.is_finite() && v.abs() < 1e14)
            && self.x_max - self.x_min > 1e-10
            && self.y_max - self.y_min > 1e-10
    }
    pub fn width(self) -> f64 {
        self.x_max - self.x_min
    }
    pub fn height(self) -> f64 {
        self.y_max - self.y_min
    }
    pub fn expanded(self, x: f64, y: f64) -> Self {
        Self {
            x_min: self.x_min - x,
            x_max: self.x_max + x,
            y_min: self.y_min - y,
            y_max: self.y_max + y,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputKind {
    Interval,
    Curve,
    Point,
}
impl fmt::Display for InputKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Interval => "Real interval",
            Self::Curve => "Curve",
            Self::Point => "Single point",
        })
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputPath {
    #[serde(rename = "path_name")]
    pub name: String,
    #[serde(rename = "path_enabled")]
    pub enabled: bool,
    pub input_kind: Option<InputKind>,
    pub samples: String,
    pub interval: [String; 2],
    pub extrapolation: String,
    pub curve: Curve,
    pub control_points: Vec<[f64; 2]>,
    pub curve_weights: Vec<String>,
    pub point_expressions: Option<Vec<[String; 2]>>,
}
impl Default for InputPath {
    fn default() -> Self {
        Self {
            name: "Path 1".into(),
            enabled: true,
            input_kind: None,
            samples: "1600".into(),
            interval: ["-3".into(), "3".into()],
            extrapolation: "15".into(),
            curve: Curve::Bezier,
            control_points: vec![[-2.4, -1.2], [-1., 2.2], [1.2, 2.2], [2.4, -0.8]],
            curve_weights: vec!["1".into(); 4],
            point_expressions: None,
        }
    }
}
impl InputPath {
    pub fn kind(&self, mode: Mode) -> InputKind {
        // Curves belong to the complex plane. Keep their saved geometry and
        // kind intact when viewing the same project as a real function.
        match (mode, self.input_kind) {
            (_, Some(InputKind::Point)) => InputKind::Point,
            (Mode::Real, _) => InputKind::Interval,
            (Mode::Complex, kind) => kind.unwrap_or(InputKind::Curve),
        }
    }
    pub fn drafts(&self) -> Vec<[String; 2]> {
        self.point_expressions.clone().unwrap_or_else(|| {
            self.control_points
                .iter()
                .map(|p| p.map(|x| number(C::new(x, 0.))))
                .collect()
        })
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub mode: Mode,
    pub bounds: Bounds,
    #[serde(flatten)]
    pub input: InputPath,
    pub additional_paths: Vec<InputPath>,
    pub grid: bool,
    pub unit_circle: bool,
    pub analysis: crate::numerics::AnalysisOptions,
    pub spectrum: crate::numerics::filter::Settings,
    pub grid_step: String,
    pub grid_y_step: String,
    pub guides: crate::axes::Guides,
    pub nodes: bool,
    pub values: bool,
    pub weights: bool,
    pub weighted_values: bool,
    pub output: bool,
    pub input_path: bool,
    pub coloring: bool,
    pub real_coloring: bool,
    pub line_opacity: f32,
    pub input_opacity: f32,
    pub input_line: crate::appearance::LineStyle,
    pub output_line: crate::appearance::LineStyle,
    #[serde(default = "crate::appearance::ColourStyle::classic")]
    pub colour_style: crate::appearance::ColourStyle,
    pub color_opacity: f32,
    pub resolution: u32,
    pub domain_extent: Extent,
    pub domain_margin: [String; 2],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Real,
            bounds: Bounds::default(),
            input: InputPath::default(),
            additional_paths: Vec::new(),
            grid: true,
            unit_circle: false,
            analysis: crate::numerics::AnalysisOptions::default(),
            spectrum: crate::numerics::filter::Settings::default(),
            grid_step: "auto".into(),
            grid_y_step: "same".into(),
            guides: Default::default(),
            nodes: true,
            values: true,
            weights: true,
            weighted_values: false,
            output: true,
            input_path: true,
            coloring: true,
            real_coloring: false,
            line_opacity: 1.,
            input_opacity: 0.9,
            input_line: crate::appearance::LineStyle::input(),
            output_line: Default::default(),
            colour_style: Default::default(),
            color_opacity: 0.75,
            resolution: 1024,
            domain_extent: Extent::Percent,
            domain_margin: ["0".into(), "0".into()],
        }
    }
}
impl Settings {
    pub fn resolve_path_kinds(&mut self) {
        let mode = self.mode;
        for path in std::iter::once(&mut self.input).chain(&mut self.additional_paths) {
            if path.input_kind.is_none() {
                path.input_kind = Some(path.kind(mode));
            }
        }
    }
    pub fn paths(&self) -> impl Iterator<Item = &InputPath> {
        std::iter::once(&self.input).chain(&self.additional_paths)
    }
    pub fn path(&self, i: usize) -> Option<&InputPath> {
        if i == 0 {
            Some(&self.input)
        } else {
            self.additional_paths.get(i - 1)
        }
    }
    pub fn path_mut(&mut self, i: usize) -> Option<&mut InputPath> {
        if i == 0 {
            Some(&mut self.input)
        } else {
            self.additional_paths.get_mut(i - 1)
        }
    }

    pub fn coloring_enabled(&self) -> bool {
        if self.mode == Mode::Real {
            self.real_coloring
        } else {
            self.coloring
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    #[serde(default)]
    pub entry_order: Vec<usize>,
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub globals: Definitions,
    pub settings: Settings,
    #[serde(default)]
    pub fit: crate::numerics::aaa::Settings,
}
impl Default for Project {
    fn default() -> Self {
        let mut entries: Vec<_> = (0..3).map(Entry::new).collect();
        // Three supports represent r(x) = (x + 1) / (x² + 1) exactly.
        entries[0].expressions = ["-3".into(), "-1/5".into(), "1".into()];
        entries[1].expressions = ["0".into(), "1".into(), "-1/5".into()];
        entries[2].expressions = ["3".into(), "2/5".into(), "1".into()];
        Self {
            entry_order: Vec::new(),
            entries,
            globals: Definitions::default(),
            fit: crate::numerics::aaa::Settings::default(),
            settings: Settings::default(),
        }
    }
}
/// Historical polynomial/rational fixtures used by numerical regression tests.
#[cfg(test)]
pub fn reference_project(mode: Mode) -> Project {
    let mut entries: Vec<_> = (0..3).map(Entry::new).collect();
    entries[0].expressions = ["-a".into(), "1".into(), "1".into()];
    entries[1].expressions = ["0".into(), "-1".into(), "-2".into()];
    entries[2].expressions = ["a".into(), "1".into(), "1".into()];
    if mode == Mode::Complex {
        entries[0].expressions = ["-a".into(), "-1".into(), "1".into()];
        entries[0].imaginary = Some(["0".into(), "1".into(), "0".into()]);
        entries[1].expressions = ["0".into(), "1".into(), "-1".into()];
        entries[1].imaginary = Some(["1".into(), "0".into(), "1".into()]);
        entries[2].expressions = ["a".into(), "1".into(), "1".into()];
        entries[2].imaginary = Some(["0".into(), "-1".into(), "0".into()]);
    }
    Project {
        entry_order: Vec::new(),
        entries,
        globals: if mode == Mode::Real {
            "a=2".into()
        } else {
            "a=1.5".into()
        },
        fit: Default::default(),
        settings: Settings {
            mode,
            input: InputPath {
                interval: ["-2".into(), "2".into()],
                control_points: vec![[-2., -1.], [-1., 2.], [1., -2.], [2., 1.]],
                ..Default::default()
            },
            ..Default::default()
        },
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub row: usize,
    pub z: C,
    pub value: C,
    pub weight: C,
}
#[derive(Debug, Clone)]
pub struct Compiled {
    pub nodes: Vec<Node>,
    pub variables: Variables,
    normalized_weights: Vec<C>,
}
impl Project {
    pub fn ordered_entries(&self) -> Vec<usize> {
        let mut seen = vec![false; self.entries.len()];
        let mut order = Vec::with_capacity(seen.len());
        for &i in &self.entry_order {
            if i < seen.len() && !seen[i] {
                seen[i] = true;
                order.push(i);
            }
        }
        order.extend((0..seen.len()).filter(|&i| !seen[i]));
        order
    }
    pub fn split_components(&mut self) {
        for entry in &mut self.entries {
            entry.split_components();
        }
    }
    pub fn compile(&self) -> Result<Compiled, String> {
        if self.entries.len() > 512 {
            return Err("At most 512 rows are supported".into());
        }
        if self.globals.len() > 256 {
            return Err("At most 256 global definitions are supported".into());
        }
        let resolved = self.resolve_definitions();
        if let Some((&key, error)) = resolved.issues.first_key_value() {
            return Err(self.definition_error(key, error));
        }
        let mut nodes = Vec::new();
        for (index, entry) in self.entries.iter().enumerate().filter(|(_, e)| e.enabled) {
            if parse_color(&entry.color).is_none() {
                return Err(format!("{}: use a #RRGGBB colour", entry.name));
            }
            let params = resolved.components[index]
                .map(|[re, im]| C::new(re.expect("resolved Re"), im.expect("resolved Im")));
            if nodes.iter().any(|n: &Node| n.z == params[0]) {
                return Err(format!("{} duplicates another active node", entry.name));
            }
            nodes.push(Node {
                row: index,
                z: params[0],
                value: params[1],
                weight: params[2],
            });
        }
        Compiled::from_nodes(nodes, resolved.variables)
    }
}
impl Compiled {
    pub fn from_nodes(nodes: Vec<Node>, variables: Variables) -> Result<Self, String> {
        if nodes.is_empty() || nodes.iter().all(|n| n.weight == C::new(0., 0.)) {
            return Err("Enable at least one node with a nonzero weight".into());
        }
        let weight_scale = nodes
            .iter()
            .map(|n| n.weight.re.abs().max(n.weight.im.abs()))
            .fold(0_f64, f64::max);
        let normalized_weights = nodes.iter().map(|n| n.weight / weight_scale).collect();
        Ok(Compiled {
            nodes,
            variables,
            normalized_weights,
        })
    }
}
impl Compiled {
    /// Distance and weight scaling avoids overflowing the barycentric terms near nodes.
    pub fn evaluate(&self, z: C) -> Option<C> {
        let mut distance = f64::INFINITY;
        for n in &self.nodes {
            if n.weight == C::new(0., 0.) {
                continue;
            }
            if z == n.z {
                return Some(n.value);
            }
            distance = distance.min((z - n.z).norm());
        }
        let mut num = C::new(0., 0.);
        let mut den = C::new(0., 0.);
        let mut total = 0.;
        for (n, weight) in self.nodes.iter().zip(&self.normalized_weights) {
            if n.weight == C::new(0., 0.) {
                continue;
            }
            let term = *weight * (C::new(distance, 0.) / (z - n.z));
            num += term * n.value;
            den += term;
            total += term.norm();
        }
        if den.norm() <= f64::EPSILON * total * 4. {
            return None;
        }
        let result = num / den;
        (result.re.is_finite() && result.im.is_finite()).then_some(result)
    }
}
pub fn parse_color(s: &str) -> Option<[f32; 3]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    Some([
        u8::from_str_radix(&s[0..2], 16).ok()? as f32 / 255.,
        u8::from_str_radix(&s[2..4], 16).ok()? as f32 / 255.,
        u8::from_str_radix(&s[4..6], 16).ok()? as f32 / 255.,
    ])
}
/// Round-trip literals retain precision when a handle replaces an expression.
pub fn number(z: C) -> String {
    let re = if z.re == 0.0 { 0.0 } else { z.re };
    if z.im == 0.0 {
        re.to_string()
    } else {
        format!("{re}{:+}i", z.im)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod project_tests;

#[cfg(test)]
mod component_tests;
