//! Table definitions and order-independent resolution shared by compilation and field diagnostics.
use super::{Mode, Project};
use crate::expression::{self, Expression, Function, Variables};
use num_complex::Complex64 as C;
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    ops::{Deref, DerefMut},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefinitionKind {
    Variable,
    Function,
    NumericFunction,
}
impl DefinitionKind {
    pub fn is_function(self) -> bool {
        self != Self::Variable
    }
}
impl fmt::Display for DefinitionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Variable => "Variable",
            Self::Function => "Function",
            Self::NumericFunction => "Numeric function",
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Definition {
    pub kind: DefinitionKind,
    pub name: String,
    #[serde(default)]
    pub parameters: String,
    pub expression: String,
}
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct Definitions(Vec<Definition>);
impl Deref for Definitions {
    type Target = Vec<Definition>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Definitions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl From<&str> for Definitions {
    fn from(source: &str) -> Self {
        Self(
            source
                .lines()
                .filter_map(|line| {
                    let line = line.split('#').next().unwrap_or("").trim();
                    if line.is_empty() {
                        return None;
                    }
                    let (signature, body) = line.split_once('=').unwrap_or((line, ""));
                    let signature = signature.trim();
                    let (kind, name, parameters) =
                        if let Some((name, args)) = signature.split_once('(') {
                            (
                                DefinitionKind::Function,
                                name.trim(),
                                args.strip_suffix(')').unwrap_or(args).trim(),
                            )
                        } else {
                            (DefinitionKind::Variable, signature, "")
                        };
                    Some(Definition {
                        kind,
                        name: name.into(),
                        parameters: parameters.into(),
                        expression: body.trim().into(),
                    })
                })
                .collect(),
        )
    }
}
impl From<String> for Definitions {
    fn from(s: String) -> Self {
        s.as_str().into()
    }
}
impl<'de> Deserialize<'de> for Definitions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Saved {
            Legacy(String),
            Table(Vec<Definition>),
        }
        Ok(match Saved::deserialize(deserializer)? {
            Saved::Legacy(s) => s.into(),
            Saved::Table(rows) => Self(rows),
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DefinitionKey {
    GlobalName(usize),
    GlobalParameters(usize),
    GlobalExpression(usize),
    RowName(usize),
    Parameter(usize, usize, usize),
}
pub struct Resolved {
    pub variables: Variables,
    pub components: Vec<[[Option<f64>; 2]; 3]>,
    pub issues: BTreeMap<DefinitionKey, String>,
}
const FIELDS: [&str; 3] = ["node", "value", "weight"];
struct Work {
    key: DefinitionKey,
    expression: Expression,
    combined: bool,
}
impl Resolved {
    fn queue(&mut self, tasks: &mut Vec<Work>, key: DefinitionKey, source: &str, combined: bool) {
        match Expression::parse(source) {
            Ok(expression) => tasks.push(Work {
                key,
                expression,
                combined,
            }),
            Err(e) => {
                self.issues.insert(key, e);
            }
        }
    }
    fn assign(&mut self, project: &Project, task: &Work, value: C) -> Result<(), String> {
        match task.key {
            DefinitionKey::GlobalExpression(i) => {
                self.variables
                    .insert(project.globals[i].name.clone(), value);
            }
            DefinitionKey::Parameter(i, j, k) => {
                let entry = &project.entries[i];
                let mut components = if task.combined {
                    [Some(value.re), Some(value.im)]
                } else {
                    if value.im.abs() > 1e-12 {
                        return Err("Expected a real component".into());
                    }
                    let mut parts = [None; 2];
                    parts[k] = Some(value.re);
                    parts
                };
                for (part, v) in components.iter_mut().enumerate() {
                    if let Some(v) = v {
                        if entry.locks[j][part] {
                            *v = v.round();
                        }
                        self.components[i][j][part] = Some(*v);
                        self.variables.insert(
                            format!("{}.{}.{}", entry.name, FIELDS[j], ["re", "im"][part]),
                            C::new(*v, 0.),
                        );
                    }
                }
                if let [Some(re), Some(im)] = self.components[i][j] {
                    let z = C::new(re, im);
                    self.variables
                        .insert(format!("{}.{}", entry.name, FIELDS[j]), z);
                }
                if let ([Some(vr), Some(vi)], [Some(wr), Some(wi)]) =
                    (self.components[i][1], self.components[i][2])
                {
                    let s = C::new(vr, vi) * C::new(wr, wi);
                    self.variables
                        .insert(format!("{}.weighted_value", entry.name), s);
                    self.variables.insert(
                        format!("{}.weighted_value.re", entry.name),
                        C::new(s.re, 0.),
                    );
                    self.variables.insert(
                        format!("{}.weighted_value.im", entry.name),
                        C::new(s.im, 0.),
                    );
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}
impl Project {
    pub fn resolve_definitions(&self) -> Resolved {
        let mut result = Resolved {
            variables: Variables::new(),
            components: vec![[[None; 2]; 3]; self.entries.len()],
            issues: BTreeMap::new(),
        };
        let mut namespace: BTreeMap<&str, Vec<DefinitionKey>> = BTreeMap::new();
        for (i, g) in self.globals.iter().enumerate() {
            namespace
                .entry(&g.name)
                .or_default()
                .push(DefinitionKey::GlobalName(i));
        }
        for (i, e) in self.entries.iter().enumerate().filter(|(_, e)| e.enabled) {
            namespace
                .entry(&e.name)
                .or_default()
                .push(DefinitionKey::RowName(i));
        }
        for (name, keys) in namespace {
            if !expression::valid_name(name) || keys.len() > 1 {
                for key in keys {
                    result.issues.insert(key,format!("Invalid or duplicate name '{name}'; use a unique ASCII identifier, distinct from built-ins and constants"));
                }
            }
        }
        let mut known_values = BTreeSet::new();
        let mut known_functions = BTreeSet::new();
        for g in self.globals.iter() {
            if g.kind == DefinitionKind::Variable {
                known_values.insert(g.name.clone());
            } else {
                known_functions.insert(g.name.clone());
            }
        }
        for e in self.entries.iter().filter(|e| e.enabled) {
            result.variables.declare_row(e.name.clone());
            for field in ["node", "value", "weight", "weighted_value"] {
                known_values.insert(format!("{}.{field}", e.name));
                for part in ["re", "im"] {
                    known_values.insert(format!("{}.{field}.{part}", e.name));
                }
            }
        }
        let mut functions = BTreeMap::new();
        let arities: BTreeMap<_, _> = self
            .globals
            .iter()
            .filter(|g| g.kind.is_function())
            .filter_map(|g| {
                expression::parameters(&g.parameters)
                    .ok()
                    .map(|p| (g.name.clone(), p.len()))
            })
            .collect();
        let mut calls: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut tasks = Vec::new();
        for (i, g) in self.globals.iter().enumerate() {
            if result.issues.contains_key(&DefinitionKey::GlobalName(i)) {
                continue;
            }
            if g.kind == DefinitionKind::Variable {
                result.queue(
                    &mut tasks,
                    DefinitionKey::GlobalExpression(i),
                    &g.expression,
                    false,
                );
                continue;
            }
            let args = match expression::parameters(&g.parameters) {
                Ok(p) => p,
                Err(e) => {
                    result.issues.insert(DefinitionKey::GlobalParameters(i), e);
                    continue;
                }
            };
            let body = match if g.kind == DefinitionKind::NumericFunction {
                Expression::parse_body(&g.expression)
            } else {
                Expression::parse(&g.expression)
            } {
                Ok(e) => e,
                Err(e) => {
                    result.issues.insert(DefinitionKey::GlobalExpression(i), e);
                    continue;
                }
            };
            if let Err(e) = body.check_calls(&arities) {
                result.issues.insert(DefinitionKey::GlobalExpression(i), e);
                continue;
            }
            let (mut vars, mut used) = (BTreeSet::new(), BTreeSet::new());
            body.symbols(&mut vars, &mut used);
            let error = vars
                .iter()
                .find(|n| !known_values.contains(*n) && !args.contains(n))
                .map(|n| format!("{} in function body", result.variables.reference_error(n)))
                .or_else(|| {
                    used.iter()
                        .find(|n| !expression::is_function(n) && !known_functions.contains(*n))
                        .map(|n| format!("Unknown function '{n}'"))
                });
            if let Some(e) = error {
                result.issues.insert(DefinitionKey::GlobalExpression(i), e);
                continue;
            }
            calls.insert(
                g.name.clone(),
                used.into_iter()
                    .filter(|n| known_functions.contains(n))
                    .collect(),
            );
            functions.insert(
                i,
                Function {
                    parameters: args,
                    body,
                },
            );
        }
        for (i, function) in functions {
            let name = &self.globals[i].name;
            let mut todo: Vec<_> = calls.get(name).into_iter().flatten().cloned().collect();
            let mut visited = BTreeSet::new();
            let mut recursive = false;
            while let Some(next) = todo.pop() {
                if next == *name {
                    recursive = true;
                    break;
                }
                if visited.insert(next.clone()) {
                    todo.extend(calls.get(&next).into_iter().flatten().cloned());
                }
            }
            if recursive {
                result.issues.insert(
                    DefinitionKey::GlobalExpression(i),
                    format!("Recursive function dependency involving '{name}'"),
                );
            } else {
                result.variables.define_function(name.clone(), function);
            }
        }
        for (i, e) in self.entries.iter().enumerate().filter(|(_, e)| e.enabled) {
            if result.issues.contains_key(&DefinitionKey::RowName(i)) {
                continue;
            }
            for j in 0..3 {
                if self.settings.mode == Mode::Real {
                    result.components[i][j][1] = Some(0.);
                    result
                        .variables
                        .insert(format!("{}.{}.im", e.name, FIELDS[j]), C::new(0., 0.));
                }
                let combined = self.settings.mode == Mode::Complex && e.imaginary.is_none();
                result.queue(
                    &mut tasks,
                    DefinitionKey::Parameter(i, j, 0),
                    &e.expressions[j],
                    combined,
                );
                if self.settings.mode == Mode::Complex
                    && let Some(parts) = &e.imaginary
                {
                    result.queue(
                        &mut tasks,
                        DefinitionKey::Parameter(i, j, 1),
                        &parts[j],
                        false,
                    );
                }
            }
        }
        while !tasks.is_empty() {
            let before = tasks.len();
            tasks.retain(|task| {
                match task
                    .expression
                    .evaluate(&result.variables)
                    .and_then(|v| result.assign(self, task, v))
                {
                    Ok(()) => {
                        result.issues.remove(&task.key);
                        false
                    }
                    Err(e) => {
                        result.issues.insert(task.key, e);
                        true
                    }
                }
            });
            if tasks.len() == before {
                for task in &tasks {
                    if let Some(e) = result.issues.get_mut(&task.key)
                        && e.contains("Unknown variable")
                    {
                        e.push_str("; check for a missing name or circular dependency");
                    }
                }
                break;
            }
        }
        result
    }
    pub fn definition_error(&self, key: DefinitionKey, error: &str) -> String {
        match key {
            DefinitionKey::GlobalName(i)
            | DefinitionKey::GlobalExpression(i)
            | DefinitionKey::GlobalParameters(i) => {
                format!("Definition {} ({}): {error}", i + 1, self.globals[i].name)
            }
            DefinitionKey::RowName(i) => format!("Row {}: {error}", i + 1),
            DefinitionKey::Parameter(i, j, k) => format!(
                "{}.{} {}: {error}",
                self.entries[i].name,
                FIELDS[j],
                ["Re", "Im"][k]
            ),
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod numeric_tests;
