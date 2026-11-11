//! Pure numeric blocks: lexical locals, lazy control flow and bounded range loops.
use super::*;

const MAX_BODY_BYTES: usize = 32_768;
const MAX_LOOP_ITERATIONS: usize = 10_000;

#[derive(Debug, Clone, Copy)]
pub enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}
impl Comparison {
    pub(super) fn evaluate(self, a: C, b: C) -> Result<bool, String> {
        Ok(match self {
            Self::Equal => a == b,
            Self::NotEqual => a != b,
            _ => {
                if a.im != 0. || b.im != 0. {
                    return Err("Ordering requires real values; use re(z), im(z) or abs(z)".into());
                }
                match self {
                    Self::Less => a.re < b.re,
                    Self::LessEqual => a.re <= b.re,
                    Self::Greater => a.re > b.re,
                    Self::GreaterEqual => a.re >= b.re,
                    _ => unreachable!(),
                }
            }
        })
    }
}
pub(super) fn truth(value: C) -> Result<bool, String> {
    if value.im != 0. {
        Err("A condition must be real; use re(z), im(z) or abs(z)".into())
    } else {
        Ok(value.re != 0.)
    }
}
#[derive(Debug, Clone, Copy)]
struct Site {
    line: usize,
    column: usize,
}
impl Site {
    fn error(self, error: impl std::fmt::Display) -> String {
        format!("line {}, column {}: {error}", self.line, self.column)
    }
}
#[derive(Debug, Clone)]
pub struct Block {
    statements: Vec<(Site, Statement)>,
    tail: Option<(Site, Box<Expression>)>,
}
#[derive(Debug, Clone)]
enum Statement {
    Let(String, Expression),
    Assign(String, u8, Expression),
    For {
        name: String,
        start: Expression,
        end: Expression,
        inclusive: bool,
        body: Expression,
    },
    Expression(Expression),
}
impl Expression {
    /// A function body may be a bare sequence of statements or an outer block.
    /// It must finish with a numeric expression, without a trailing semicolon.
    pub fn parse_body(source: &str) -> Result<Self, String> {
        if source.len() > MAX_BODY_BYTES {
            return Err("Numeric body exceeds 32768 bytes".into());
        }
        let mut p = Parser {
            source: source.as_bytes(),
            pos: 0,
            depth: 0,
        };
        let expression = (|| {
            p.space();
            let body = if p.source.get(p.pos) == Some(&b'{') {
                p.block()?
            } else {
                Expression::Block(p.contents(false)?)
            };
            p.space();
            if p.pos != p.source.len() {
                return Err("Unexpected text after function body".into());
            }
            if matches!(&body,Expression::Block(block) if block.tail.is_none()) {
                return Err(
                    "End the function body with a numeric expression without a semicolon".into(),
                );
            }
            Ok(body)
        })()
        .map_err(|e: String| p.error(e))?;
        expression.check_depth()?;
        expression.check_locals(&BTreeMap::new())?;
        Ok(expression)
    }
    pub(super) fn check_locals(&self, locals: &BTreeMap<String, bool>) -> Result<(), String> {
        match self {
            Self::Block(block) => block.check_locals(locals),
            _ => {
                for child in self.children() {
                    child.check_locals(locals)?;
                }
                Ok(())
            }
        }
    }
}
impl Block {
    pub(super) fn children(&self) -> Vec<&Expression> {
        let mut result = Vec::new();
        for (_, statement) in &self.statements {
            match statement {
                Statement::Let(_, e) | Statement::Assign(_, _, e) | Statement::Expression(e) => {
                    result.push(e)
                }
                Statement::For {
                    start, end, body, ..
                } => result.extend([start, end, body]),
            }
        }
        if let Some((_, tail)) = &self.tail {
            result.push(tail);
        }
        result
    }
    fn check_locals(&self, outer: &BTreeMap<String, bool>) -> Result<(), String> {
        let mut locals = outer.clone();
        let mut declared = BTreeSet::new();
        for (site, statement) in &self.statements {
            (|| match statement {
                Statement::Let(name,expr) => {
                    expr.check_locals(&locals)?;
                    if !declared.insert(name) {return Err(format!("Local '{name}' is already declared in this block"));}
                    locals.insert(name.clone(),true);
                    Ok(())
                }
                Statement::Assign(name,_,expr) => {
                    match locals.get(name) {
                        Some(true) => {},
                        Some(false) => return Err(format!("Loop index '{name}' is read-only")),
                        None => return Err(format!("Cannot assign '{name}'; declare a local with let first. Globals and arguments are read-only")),
                    }
                    expr.check_locals(&locals)
                }
                Statement::For{name,start,end,body,..} => {
                    start.check_locals(&locals)?;
                    end.check_locals(&locals)?;
                    let mut inner=locals.clone();inner.insert(name.clone(),false);
                    body.check_locals(&inner)
                }
                Statement::Expression(expr) => expr.check_locals(&locals),
            })().map_err(|e:String|site.error(e))?;
        }
        if let Some((site, tail)) = &self.tail {
            tail.check_locals(&locals).map_err(|e| site.error(e))?;
        }
        Ok(())
    }
    pub(super) fn symbols(
        &self,
        variables: &mut BTreeSet<String>,
        functions: &mut BTreeSet<String>,
        outer: &BTreeSet<String>,
    ) {
        let mut bound = outer.clone();
        for (_, statement) in &self.statements {
            match statement {
                Statement::Let(name, expr) => {
                    expr.scoped_symbols(variables, functions, &bound);
                    bound.insert(name.clone());
                }
                Statement::Assign(_, _, expr) | Statement::Expression(expr) => {
                    expr.scoped_symbols(variables, functions, &bound)
                }
                Statement::For {
                    name,
                    start,
                    end,
                    body,
                    ..
                } => {
                    start.scoped_symbols(variables, functions, &bound);
                    end.scoped_symbols(variables, functions, &bound);
                    let mut inner = bound.clone();
                    inner.insert(name.clone());
                    body.scoped_symbols(variables, functions, &inner);
                }
            }
        }
        if let Some((_, tail)) = &self.tail {
            tail.scoped_symbols(variables, functions, &bound);
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn eval(
        &self,
        vars: &Variables,
        names: &[String],
        args: &[C],
        locals: &mut Vec<Local>,
        budget: &mut usize,
        depth: usize,
    ) -> Result<C, String> {
        let base = locals.len();
        let result = (|| {
            for (site, statement) in &self.statements {
                *budget = budget
                    .checked_sub(1)
                    .ok_or_else(|| site.error("Function evaluation exceeds the operation limit"))?;
                (|| {
                    match statement {
                        Statement::Let(name, expr) => {
                            let value = expr.eval(vars, names, args, locals, budget, depth + 1)?;
                            locals.push(Local {
                                name: name.clone(),
                                value,
                            });
                        }
                        Statement::Assign(name, op, expr) => {
                            let value = expr.eval(vars, names, args, locals, budget, depth + 1)?;
                            let local = locals
                                .iter_mut()
                                .rev()
                                .find(|local| local.name == *name)
                                .ok_or_else(|| format!("Unknown local '{name}'"))?;
                            let next = match op {
                                b'=' => value,
                                b'+' => local.value + value,
                                b'-' => local.value - value,
                                b'*' => local.value * value,
                                b'/' => local.value / value,
                                b'%' => math::call("rem", &[local.value, value])?,
                                _ => unreachable!(),
                            };
                            if !next.re.is_finite() || !next.im.is_finite() {
                                return Err("Local value is not finite".into());
                            }
                            local.value = next;
                        }
                        Statement::Expression(expr) => {
                            expr.eval(vars, names, args, locals, budget, depth + 1)?;
                        }
                        Statement::For {
                            name,
                            start,
                            end,
                            inclusive,
                            body,
                        } => {
                            let start = start.eval(vars, names, args, locals, budget, depth + 1)?;
                            let end = end.eval(vars, names, args, locals, budget, depth + 1)?;
                            let bound = |z: C| {
                                z.im == 0.
                                    && z.re.fract() == 0.
                                    && z.re.abs() <= 9_007_199_254_740_991.
                            };
                            if !bound(start) || !bound(end) {
                                return Err(
                                    "Loop bounds must be real integers with magnitude below 2^53"
                                        .into(),
                                );
                            }
                            let count = if end.re >= start.re {
                                end.re - start.re + f64::from(*inclusive)
                            } else {
                                0.
                            };
                            if count > MAX_LOOP_ITERATIONS as f64 {
                                return Err(format!(
                                    "Loop exceeds {MAX_LOOP_ITERATIONS} iterations"
                                ));
                            }
                            let index = locals.len();
                            locals.push(Local {
                                name: name.clone(),
                                value: start,
                            });
                            for k in 0..count as usize {
                                *budget = budget
                                    .checked_sub(1)
                                    .ok_or("Function evaluation exceeds the operation limit")?;
                                locals[index].value = C::new(start.re + k as f64, 0.);
                                body.eval(vars, names, args, locals, budget, depth + 1)?;
                            }
                            locals.truncate(index);
                        }
                    }
                    Ok(())
                })()
                .map_err(|e: String| site.error(e))?;
            }
            self.tail
                .as_ref()
                .map_or(Ok(C::new(0., 0.)), |(site, tail)| {
                    tail.eval(vars, names, args, locals, budget, depth + 1)
                        .map_err(|e| site.error(e))
                })
        })();
        locals.truncate(base);
        result
    }
}
impl Parser<'_> {
    fn site(&self) -> Site {
        let before = &self.source[..self.pos];
        Site {
            line: before.iter().filter(|&&b| b == b'\n').count() + 1,
            column: before
                .iter()
                .rposition(|&b| b == b'\n')
                .map_or(self.pos + 1, |i| self.pos - i),
        }
    }
    pub(super) fn error(&self, error: impl std::fmt::Display) -> String {
        self.site().error(error)
    }
    fn token(&mut self, token: &[u8]) -> bool {
        self.space();
        if self.source.get(self.pos..self.pos + token.len()) == Some(token) {
            self.pos += token.len();
            true
        } else {
            false
        }
    }
    pub(super) fn starts_keyword(&mut self, keyword: &str) -> bool {
        self.space();
        self.source.get(self.pos..self.pos + keyword.len()) == Some(keyword.as_bytes())
            && !self
                .source
                .get(self.pos + keyword.len())
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
    }
    pub(super) fn keyword(&mut self, keyword: &str) -> bool {
        if self.starts_keyword(keyword) {
            self.pos += keyword.len();
            true
        } else {
            false
        }
    }
    fn identifier(&mut self) -> Result<String, String> {
        self.space();
        let start = self.pos;
        while self
            .source
            .get(self.pos)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
        {
            self.pos += 1;
        }
        let name =
            std::str::from_utf8(&self.source[start..self.pos]).map_err(|_| "Use ASCII names")?;
        if !valid_name(name) {
            return Err(
                "Expected a local name, distinct from keywords, constants and built-in functions"
                    .into(),
            );
        }
        Ok(name.into())
    }
    pub(super) fn expression(&mut self) -> Result<Expression, String> {
        self.logical_or()
    }
    fn logical_or(&mut self) -> Result<Expression, String> {
        let mut expr = self.logical_and()?;
        while self.token(b"||") {
            expr = Expression::Logical(false, Box::new(expr), Box::new(self.logical_and()?));
        }
        Ok(expr)
    }
    fn logical_and(&mut self) -> Result<Expression, String> {
        let mut expr = self.comparison()?;
        while self.token(b"&&") {
            expr = Expression::Logical(true, Box::new(expr), Box::new(self.comparison()?));
        }
        Ok(expr)
    }
    fn comparison(&mut self) -> Result<Expression, String> {
        let lhs = self.sum()?;
        for (token, op) in [
            (b"==".as_slice(), Comparison::Equal),
            (b"!=", Comparison::NotEqual),
            (b"<=", Comparison::LessEqual),
            (b">=", Comparison::GreaterEqual),
            (b"<", Comparison::Less),
            (b">", Comparison::Greater),
        ] {
            if self.token(token) {
                return Ok(Expression::Compare(
                    op,
                    Box::new(lhs),
                    Box::new(self.sum()?),
                ));
            }
        }
        Ok(lhs)
    }
    pub(super) fn block(&mut self) -> Result<Expression, String> {
        if !self.eat(b'{') {
            return Err("Expected '{' to start a block".into());
        }
        self.depth += 1;
        if self.depth > 64 {
            return Err("Expression nesting exceeds 64 levels".into());
        }
        let result = self.contents(true).map(Expression::Block);
        self.depth -= 1;
        result
    }
    fn finished(&mut self, braced: bool) -> bool {
        self.space();
        self.pos == self.source.len() || braced && self.source.get(self.pos) == Some(&b'}')
    }
    fn contents(&mut self, braced: bool) -> Result<Block, String> {
        let mut block = Block {
            statements: Vec::new(),
            tail: None,
        };
        while !self.finished(braced) {
            let site = self.site();
            let statement = if self.keyword("let") {
                let name = self.identifier()?;
                if !self.eat(b'=') {
                    return Err("Expected '=' after local name".into());
                }
                let expr = self.expression()?;
                if !self.eat(b';') {
                    return Err("Expected ';' after let declaration".into());
                }
                Statement::Let(name, expr)
            } else if self.keyword("for") {
                let name = self.identifier()?;
                if !self.keyword("in") {
                    return Err("Expected 'in' after loop index".into());
                }
                let start = self.expression()?;
                if !self.token(b"..") {
                    return Err("Expected '..' or '..=' between loop bounds".into());
                }
                let inclusive = self.eat(b'=');
                let end = self.expression()?;
                let body = self.block()?;
                self.eat(b';');
                Statement::For {
                    name,
                    start,
                    end,
                    inclusive,
                    body,
                }
            } else {
                let saved = self.pos;
                let assign = self.identifier().ok().and_then(|name| {
                    self.space();
                    if self.source.get(self.pos) == Some(&b'=')
                        && self.source.get(self.pos + 1) != Some(&b'=')
                    {
                        self.pos += 1;
                        Some((name, b'='))
                    } else {
                        (*b"+-*/%")
                            .into_iter()
                            .find(|&op| {
                                self.source.get(self.pos..self.pos + 2)
                                    == Some([op, b'='].as_slice())
                            })
                            .map(|op| {
                                self.pos += 2;
                                (name, op)
                            })
                    }
                });
                if let Some((name, op)) = assign {
                    let expr = self.expression()?;
                    if !self.eat(b';') {
                        return Err("Expected ';' after assignment".into());
                    }
                    Statement::Assign(name, op, expr)
                } else {
                    self.pos = saved;
                    if self.starts_keyword("while")
                        || self.starts_keyword("return")
                        || self.starts_keyword("break")
                        || self.starts_keyword("continue")
                    {
                        return Err("Use bounded for loops and a final numeric expression; while/return/break/continue are not supported".into());
                    }
                    let expr = self.expression()?;
                    if self.eat(b';') {
                        Statement::Expression(expr)
                    } else if self.finished(braced) {
                        block.tail = Some((site, Box::new(expr)));
                        break;
                    } else if matches!(expr, Expression::If(..) | Expression::Block(..)) {
                        Statement::Expression(expr)
                    } else {
                        return Err(
                            "Expected ';' between statements, or a final numeric expression".into(),
                        );
                    }
                }
            };
            block.statements.push((site, statement));
        }
        if braced && !self.eat(b'}') {
            return Err("Expected '}' to close block".into());
        }
        Ok(block)
    }
}

#[cfg(test)]
mod tests;
