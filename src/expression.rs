//! Bounded complex expressions, reusable user functions, and named row selectors.
mod math;
mod numeric;
mod parser;
pub use math::FUNCTION_HELP;
use num_complex::Complex64 as C;
use parser::Parser;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::{Deref, DerefMut},
    sync::Arc,
};

#[derive(Debug, Clone, Default)]
pub struct Variables {
    values: BTreeMap<String, C>,
    functions: BTreeMap<String, Arc<Function>>,
    rows: BTreeSet<String>,
}
impl Variables {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn define_function(&mut self, name: String, function: Function) {
        self.functions.insert(name, Arc::new(function));
    }
    pub fn call_function(&self, name: &str, args: &[C]) -> Result<C, String> {
        let function = self
            .functions
            .get(name)
            .ok_or_else(|| format!("Unknown function '{name}'"))?;
        if args.len() != function.parameters.len() {
            return Err(format!(
                "{name} expects {} arguments",
                function.parameters.len()
            ));
        }
        function.body.eval(
            self,
            &function.parameters,
            args,
            &mut Vec::new(),
            &mut 16_384,
            0,
        )
    }
    pub fn declare_row(&mut self, name: String) {
        self.rows.insert(name);
    }
    pub fn reference_error(&self, name: &str) -> String {
        if self.rows.contains(name) {
            format!("'{name}' is a data row; select {name}[n], {name}[v], {name}[w] or {name}[wv]")
        } else {
            format!("Unknown variable '{name}'")
        }
    }
}
impl Deref for Variables {
    type Target = BTreeMap<String, C>;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl DerefMut for Variables {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}
#[derive(Debug, Clone)]
pub struct Function {
    pub parameters: Vec<String>,
    pub body: Expression,
}
pub fn parameters(source: &str) -> Result<Vec<String>, String> {
    if source.trim().is_empty() {
        return Ok(Vec::new());
    }
    let args: Vec<_> = source.split(',').map(|s| s.trim().to_owned()).collect();
    if args.len() > 16 {
        return Err("Functions accept at most 16 arguments".into());
    }
    let mut names = BTreeSet::new();
    for arg in &args {
        if !valid_name(arg) || !names.insert(arg) {
            return Err(format!("Invalid or duplicate argument '{arg}'"));
        }
    }
    Ok(args)
}
#[derive(Debug, Clone)]
pub enum Expression {
    Number(C),
    Variable(String),
    Negate(Box<Self>),
    Binary(u8, Box<Self>, Box<Self>),
    Call(String, Vec<Self>),
    Not(Box<Self>),
    Compare(numeric::Comparison, Box<Self>, Box<Self>),
    Logical(bool, Box<Self>, Box<Self>),
    If(Box<Self>, Box<Self>, Box<Self>),
    Block(numeric::Block),
}
#[derive(Clone)]
struct Local {
    name: String,
    value: C,
}
impl Expression {
    pub fn parse(source: &str) -> Result<Self, String> {
        if source.len() > 4096 {
            return Err("Expression exceeds 4096 characters".into());
        }
        let mut parser = Parser {
            source: source.as_bytes(),
            pos: 0,
            depth: 0,
        };
        let expression = parser.expression().map_err(|e| parser.error(e))?;
        parser.space();
        if parser.pos != parser.source.len() {
            return Err(parser.error("Unexpected character"));
        }
        expression.check_depth()?;
        expression.check_locals(&BTreeMap::new())?;
        Ok(expression)
    }
    fn check_depth(&self) -> Result<(), String> {
        let mut pending = vec![(self, 0)];
        while let Some((node, depth)) = pending.pop() {
            if depth > 128 {
                return Err("Expression tree exceeds 128 levels".into());
            }
            pending.extend(node.children().into_iter().map(|child| (child, depth + 1)));
        }
        Ok(())
    }
    fn children(&self) -> Vec<&Self> {
        match self {
            Self::Negate(a) | Self::Not(a) => vec![a],
            Self::Binary(_, a, b) | Self::Compare(_, a, b) | Self::Logical(_, a, b) => vec![a, b],
            Self::If(c, a, b) => vec![c, a, b],
            Self::Call(_, args) => args.iter().collect(),
            Self::Block(block) => block.children(),
            _ => vec![],
        }
    }
    pub fn evaluate(&self, variables: &Variables) -> Result<C, String> {
        self.eval(variables, &[], &[], &mut Vec::new(), &mut 16_384, 0)
    }
    fn eval(
        &self,
        vars: &Variables,
        names: &[String],
        args: &[C],
        locals: &mut Vec<Local>,
        budget: &mut usize,
        depth: usize,
    ) -> Result<C, String> {
        if depth > 128 {
            return Err("Expression or function recursion exceeds 128 levels".into());
        }
        *budget = budget
            .checked_sub(1)
            .ok_or("Function evaluation exceeds the operation limit")?;
        let value = match self {
            Self::Number(v) => *v,
            Self::Variable(name) => {
                if let Some(local) = locals.iter().rev().find(|local| local.name == *name) {
                    local.value
                } else if let Some(index) = names.iter().position(|n| n == name) {
                    args[index]
                } else {
                    vars.get(name)
                        .copied()
                        .ok_or_else(|| vars.reference_error(name))?
                }
            }
            Self::Negate(a) => -a.eval(vars, names, args, locals, budget, depth + 1)?,
            Self::Binary(op, a, b) => {
                let a = a.eval(vars, names, args, locals, budget, depth + 1)?;
                let b = b.eval(vars, names, args, locals, budget, depth + 1)?;
                match op {
                    b'+' => a + b,
                    b'-' => a - b,
                    b'*' => a * b,
                    b'/' => a / b,
                    b'%' => math::call("rem", &[a, b])?,
                    b'^' => math::power(a, b),
                    _ => unreachable!(),
                }
            }
            Self::Not(a) => C::new(
                f64::from(!numeric::truth(a.eval(
                    vars,
                    names,
                    args,
                    locals,
                    budget,
                    depth + 1,
                )?)?),
                0.,
            ),
            Self::Compare(op, a, b) => {
                let a = a.eval(vars, names, args, locals, budget, depth + 1)?;
                let b = b.eval(vars, names, args, locals, budget, depth + 1)?;
                C::new(f64::from(op.evaluate(a, b)?), 0.)
            }
            Self::Logical(and, a, b) => {
                let a = numeric::truth(a.eval(vars, names, args, locals, budget, depth + 1)?)?;
                let value = if a == *and {
                    numeric::truth(b.eval(vars, names, args, locals, budget, depth + 1)?)?
                } else {
                    a
                };
                C::new(f64::from(value), 0.)
            }
            Self::If(condition, yes, no) => {
                let branch = if numeric::truth(condition.eval(
                    vars,
                    names,
                    args,
                    locals,
                    budget,
                    depth + 1,
                )?)? {
                    yes
                } else {
                    no
                };
                branch.eval(vars, names, args, locals, budget, depth + 1)?
            }
            Self::Block(block) => block.eval(vars, names, args, locals, budget, depth + 1)?,
            Self::Call(name, expressions) => {
                let values = expressions
                    .iter()
                    .map(|e| e.eval(vars, names, args, locals, budget, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                if let Some(function) = vars.functions.get(name) {
                    if values.len() != function.parameters.len() {
                        return Err(format!(
                            "{name} expects {} arguments, got {}",
                            function.parameters.len(),
                            values.len()
                        ));
                    }
                    function
                        .body
                        .eval(
                            vars,
                            &function.parameters,
                            &values,
                            &mut Vec::new(),
                            budget,
                            depth + 1,
                        )
                        .map_err(|e| format!("{name}: {e}"))?
                } else {
                    math::call(name, &values)?
                }
            }
        };
        if !value.re.is_finite() || !value.im.is_finite() {
            return Err("Expression is not finite".into());
        }
        Ok(C::new(
            if value.re == 0. { 0. } else { value.re },
            if value.im == 0. { 0. } else { value.im },
        ))
    }
    pub fn symbols(&self, variables: &mut BTreeSet<String>, functions: &mut BTreeSet<String>) {
        self.scoped_symbols(variables, functions, &BTreeSet::new());
    }
    fn scoped_symbols(
        &self,
        variables: &mut BTreeSet<String>,
        functions: &mut BTreeSet<String>,
        bound: &BTreeSet<String>,
    ) {
        match self {
            Self::Variable(n) if !bound.contains(n) => {
                variables.insert(n.clone());
            }
            Self::Call(n, args) => {
                functions.insert(n.clone());
                for arg in args {
                    arg.scoped_symbols(variables, functions, bound);
                }
            }
            Self::Block(block) => block.symbols(variables, functions, bound),
            _ => {
                for child in self.children() {
                    child.scoped_symbols(variables, functions, bound);
                }
            }
        }
    }
    pub fn check_calls(&self, functions: &BTreeMap<String, usize>) -> Result<(), String> {
        match self {
            Self::Call(name, args) => {
                let count = math::arity(name)
                    .or_else(|| functions.get(name).copied())
                    .ok_or_else(|| format!("Unknown function '{name}'"))?;
                if count != args.len() {
                    return Err(format!(
                        "{name} expects {count} arguments, got {}",
                        args.len()
                    ));
                }
                for arg in args {
                    arg.check_calls(functions)?;
                }
            }
            _ => {
                for child in self.children() {
                    child.check_calls(functions)?;
                }
            }
        }
        Ok(())
    }
}
pub fn evaluate(source: &str, variables: &Variables) -> Result<C, String> {
    Expression::parse(source)?.evaluate(variables)
}
pub fn is_function(name: &str) -> bool {
    math::arity(name).is_some()
}
fn constant(name: &str) -> Option<C> {
    Some(match name {
        "pi" => C::new(std::f64::consts::PI, 0.),
        "tau" => C::new(std::f64::consts::TAU, 0.),
        "e" => C::new(std::f64::consts::E, 0.),
        "phi" => C::new((1. + 5_f64.sqrt()) / 2., 0.),
        "i" => C::i(),
        "true" => C::new(1., 0.),
        "false" => C::new(0., 0.),
        _ => return None,
    })
}
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && constant(name).is_none()
        && !is_function(name)
        && !matches!(
            name,
            "let" | "for" | "in" | "if" | "else" | "while" | "return" | "break" | "continue"
        )
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod extended_tests;
