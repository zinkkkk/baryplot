//! Arithmetic grammar, literals, comments, selectors and precedence.
use super::*;

pub(super) struct Parser<'a> {
    pub(super) source: &'a [u8],
    pub(super) pos: usize,
    pub(super) depth: usize,
}
impl Parser<'_> {
    pub(super) fn space(&mut self) {
        loop {
            while self
                .source
                .get(self.pos)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.pos += 1;
            }
            if self.source.get(self.pos) == Some(&b'#')
                || self.source.get(self.pos..self.pos + 2) == Some(b"//")
            {
                while self.source.get(self.pos).is_some_and(|c| *c != b'\n') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }
    pub(super) fn eat(&mut self, c: u8) -> bool {
        self.space();
        if self.source.get(self.pos) == Some(&c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    pub(super) fn sum(&mut self) -> Result<Expression, String> {
        let mut value = self.product()?;
        loop {
            let op = if self.eat(b'+') {
                b'+'
            } else if self.eat(b'-') {
                b'-'
            } else {
                return Ok(value);
            };
            value = Expression::Binary(op, Box::new(value), Box::new(self.product()?));
        }
    }
    fn product(&mut self) -> Result<Expression, String> {
        let mut value = self.unary()?;
        loop {
            let op = if self.eat(b'*') {
                b'*'
            } else if self.eat(b'/') {
                b'/'
            } else if self.eat(b'%') {
                b'%'
            } else {
                return Ok(value);
            };
            value = Expression::Binary(op, Box::new(value), Box::new(self.unary()?));
        }
    }
    pub(super) fn unary(&mut self) -> Result<Expression, String> {
        self.depth += 1;
        if self.depth > 64 {
            return Err("Expression nesting exceeds 64 levels".into());
        }
        let result = if self.eat(b'+') {
            self.unary()
        } else if self.eat(b'-') {
            self.unary().map(|a| Expression::Negate(Box::new(a)))
        } else if self.eat(b'!') {
            self.unary().map(|a| Expression::Not(Box::new(a)))
        } else {
            self.power()
        };
        self.depth -= 1;
        result
    }
    fn power(&mut self) -> Result<Expression, String> {
        let base = self.postfix()?;
        if self.eat(b'^') {
            Ok(Expression::Binary(
                b'^',
                Box::new(base),
                Box::new(self.unary()?),
            ))
        } else {
            Ok(base)
        }
    }
    fn postfix(&mut self) -> Result<Expression, String> {
        let mut value = self.atom()?;
        while self.eat(b'[') {
            self.space();
            let start = self.pos;
            while self
                .source
                .get(self.pos)
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
            {
                self.pos += 1;
            }
            let selector = std::str::from_utf8(&self.source[start..self.pos])
                .map_err(|_| "Invalid selector")?;
            if !self.eat(b']') {
                return Err("Expected ']' after selector".into());
            }
            value = match selector {
                "re" | "real" | "im" | "imag" => {
                    let component = if matches!(selector, "re" | "real") {
                        "re"
                    } else {
                        "im"
                    };
                    if let Expression::Variable(name) = &value
                        && name.contains('.')
                    {
                        Expression::Variable(format!("{name}.{component}"))
                    } else {
                        Expression::Call(component.into(), vec![value])
                    }
                }
                "n" | "node" | "0" | "v" | "value" | "1" | "w" | "weight" | "2" | "s" | "wv"
                | "3" => {
                    let Expression::Variable(name) = value else {
                        return Err("Parameter selectors require a row name".into());
                    };
                    if name.contains('.') {
                        return Err(
                            "Select a node, value or weight directly from a row name".into()
                        );
                    }
                    let field = match selector {
                        "n" | "node" | "0" => "node",
                        "v" | "value" | "1" => "value",
                        "w" | "weight" | "2" => "weight",
                        _ => "weighted_value",
                    };
                    Expression::Variable(format!("{name}.{field}"))
                }
                _ => {
                    return Err(format!(
                        "Unknown selector '{selector}'; use n, v, w, wv, re or im"
                    ));
                }
            };
        }
        Ok(value)
    }
    fn atom(&mut self) -> Result<Expression, String> {
        if self.keyword("if") {
            let condition = self.expression()?;
            let yes = self.block()?;
            let no = if self.keyword("else") {
                if self.starts_keyword("if") {
                    self.unary()?
                } else {
                    self.block()?
                }
            } else {
                Expression::Number(C::new(0., 0.))
            };
            return Ok(Expression::If(
                Box::new(condition),
                Box::new(yes),
                Box::new(no),
            ));
        }
        self.space();
        if self.source.get(self.pos) == Some(&b'{') {
            return self.block();
        }

        if self.eat(b'(') {
            let value = self.expression()?;
            if !self.eat(b')') {
                return Err("Expected ')'".into());
            }
            return Ok(value);
        }
        self.space();
        let start = self.pos;
        if self
            .source
            .get(start)
            .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
        {
            while self.source.get(self.pos).is_some_and(u8::is_ascii_digit) {
                self.pos += 1;
            }
            if self.source.get(self.pos) == Some(&b'.')
                && self.source.get(self.pos + 1) != Some(&b'.')
            {
                self.pos += 1;
                while self.source.get(self.pos).is_some_and(u8::is_ascii_digit) {
                    self.pos += 1;
                }
            }
            if self
                .source
                .get(self.pos)
                .is_some_and(|c| *c == b'e' || *c == b'E')
            {
                self.pos += 1;
                if self
                    .source
                    .get(self.pos)
                    .is_some_and(|c| *c == b'+' || *c == b'-')
                {
                    self.pos += 1;
                }
                while self.source.get(self.pos).is_some_and(u8::is_ascii_digit) {
                    self.pos += 1;
                }
            }
            let number = std::str::from_utf8(&self.source[start..self.pos])
                .map_err(|_| "Invalid number")?
                .parse::<f64>()
                .map_err(|_| "Invalid number")?;
            return Ok(Expression::Number(if self.keyword("i") {
                C::new(0., number)
            } else {
                C::new(number, 0.)
            }));
        }
        while self
            .source
            .get(self.pos)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
            || self.source.get(self.pos) == Some(&b'.')
                && self
                    .source
                    .get(self.pos + 1)
                    .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        {
            self.pos += 1;
        }
        if start == self.pos {
            return Err("Expected numeric expression".into());
        }
        let name = std::str::from_utf8(&self.source[start..self.pos])
            .map_err(|_| "Use ASCII names")?
            .to_owned();
        if !self.eat(b'(') {
            return Ok(constant(&name).map_or(Expression::Variable(name), Expression::Number));
        }
        let mut args = Vec::new();
        if !self.eat(b')') {
            loop {
                if args.len() == 16 {
                    return Err("Functions accept at most 16 arguments".into());
                }
                args.push(self.expression()?);
                if self.eat(b')') {
                    break;
                }
                if !self.eat(b',') {
                    return Err("Expected ',' or ')' after function argument".into());
                }
            }
        }
        Ok(Expression::Call(name, args))
    }
}
