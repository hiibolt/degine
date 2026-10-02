//! Formula syntax used in fact warrants and rule conclusions.
//!
//! ```text
//! fact:<id>
//! and(<formula>, <formula>, ...)
//! or(<formula>, ...)
//! imp(<formula>, <formula>)
//! not(<formula>)
//! true
//! false
//! ```
//!
//! `and` takes two or more arguments. `or` takes one or more, so a single
//! case can grow later. `imp` takes two. `fact:<id>` names an atomic proposition.

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Formula {
    Atom(String),
    And(Vec<Formula>),
    Or(Vec<Formula>),
    Imp(Box<Formula>, Box<Formula>),
    Not(Box<Formula>),
    True,
    False,
}

impl std::fmt::Display for Formula {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Formula::Atom(id) => write!(f, "fact:{id}"),
            Formula::And(parts) => {
                let refs: Vec<&Formula> = parts.iter().collect();
                write_call(f, "and", &refs)
            }
            Formula::Or(parts) => {
                let refs: Vec<&Formula> = parts.iter().collect();
                write_call(f, "or", &refs)
            }
            Formula::Imp(left, right) => write_call(f, "imp", &[left.as_ref(), right.as_ref()]),
            Formula::Not(inner) => write_call(f, "not", &[inner.as_ref()]),
            Formula::True => write!(f, "true"),
            Formula::False => write!(f, "false"),
        }
    }
}

fn write_call(f: &mut std::fmt::Formatter<'_>, name: &str, args: &[&Formula]) -> std::fmt::Result {
    write!(f, "{name}(")?;
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{arg}")?;
    }
    write!(f, ")")
}

pub(crate) fn parse(input: &str) -> Result<Formula> {
    let mut parser = Parser { s: input, i: 0 };
    let formula = parser
        .parse_formula()
        .with_context(|| format!("invalid formula `{input}`"))?;
    parser.skip();
    if parser.i != parser.s.len() {
        bail!(
            "invalid formula `{input}`: trailing `{}`",
            &parser.s[parser.i..]
        );
    }
    Ok(formula)
}

/// A one-case `or` is just that case. Lean never sees a unary disjunction.
pub(crate) fn normalize(formula: &Formula) -> Formula {
    match formula {
        Formula::Or(parts) if parts.len() == 1 => normalize(&parts[0]),
        Formula::Or(parts) => Formula::Or(parts.iter().map(normalize).collect()),
        Formula::And(parts) => Formula::And(parts.iter().map(normalize).collect()),
        Formula::Imp(left, right) => {
            Formula::Imp(Box::new(normalize(left)), Box::new(normalize(right)))
        }
        Formula::Not(inner) => Formula::Not(Box::new(normalize(inner))),
        other => other.clone(),
    }
}

pub(crate) fn atoms(formula: &Formula, out: &mut Vec<String>) {
    match formula {
        Formula::Atom(id) => {
            if !out.iter().any(|existing| existing == id) {
                out.push(id.clone());
            }
        }
        Formula::And(parts) => {
            for part in parts {
                atoms(part, out);
            }
        }
        Formula::Or(parts) => {
            for part in parts {
                atoms(part, out);
            }
        }
        Formula::Imp(left, right) => {
            atoms(left, out);
            atoms(right, out);
        }
        Formula::Not(inner) => atoms(inner, out),
        Formula::True | Formula::False => {}
    }
}

pub fn formula_atoms(input: &str) -> Vec<String> {
    let Ok(formula) = parse(input) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    atoms(&formula, &mut out);
    out
}

pub fn formula_then(input: &str) -> Option<String> {
    match parse(input).ok()? {
        Formula::Imp(_, right) => match *right {
            Formula::Atom(id) => Some(id),
            _ => None,
        },
        _ => None,
    }
}

struct Parser<'a> {
    s: &'a str,
    i: usize,
}

impl<'a> Parser<'a> {
    fn skip(&mut self) {
        while let Some(c) = self.rest().chars().next() {
            if c.is_whitespace() {
                self.i += c.len_utf8();
            } else {
                break;
            }
        }
    }

    fn rest(&self) -> &'a str {
        &self.s[self.i..]
    }

    fn bump(&mut self, lit: &str) -> bool {
        if self.rest().starts_with(lit) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }

    fn keyword(&mut self, kw: &str) -> bool {
        if !self.rest().starts_with(kw) {
            return false;
        }
        let after = &self.rest()[kw.len()..];
        let boundary = after
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        if boundary {
            self.i += kw.len();
            true
        } else {
            false
        }
    }

    fn parse_formula(&mut self) -> Result<Formula> {
        self.skip();
        if self.keyword("and") {
            let args = self.parse_call("and")?;
            if args.len() < 2 {
                bail!("and() needs at least two formulas");
            }
            return Ok(Formula::And(args));
        }
        if self.keyword("or") {
            let args = self.parse_call("or")?;
            if args.is_empty() {
                bail!("or() needs at least one formula");
            }
            return Ok(Formula::Or(args));
        }
        if self.keyword("imp") {
            let args = self.parse_call("imp")?;
            let [left, right] = args.try_into().ok().context("imp() needs two formulas")?;
            return Ok(Formula::Imp(Box::new(left), Box::new(right)));
        }
        if self.keyword("not") {
            let args = self.parse_call("not")?;
            let [inner] = args.try_into().ok().context("not() needs one formula")?;
            return Ok(Formula::Not(Box::new(inner)));
        }
        if self.keyword("true") {
            return Ok(Formula::True);
        }
        if self.keyword("false") {
            return Ok(Formula::False);
        }
        if self.keyword("fact") {
            if !self.bump(":") {
                bail!("expected ':' after fact");
            }
            let id = self.parse_id()?;
            return Ok(Formula::Atom(id));
        }
        let snippet: String = self.rest().chars().take(40).collect();
        bail!("expected a formula, found `{snippet}`");
    }

    fn parse_call(&mut self, name: &str) -> Result<Vec<Formula>> {
        self.skip();
        if !self.bump("(") {
            bail!("expected '(' after {name}");
        }
        self.skip();
        if self.bump(")") {
            return Ok(Vec::new());
        }
        let mut args = Vec::new();
        loop {
            args.push(self.parse_formula()?);
            self.skip();
            if self.bump(")") {
                break;
            }
            if !self.bump(",") {
                bail!("expected ',' or ')' in {name}()");
            }
        }
        Ok(args)
    }

    fn parse_id(&mut self) -> Result<String> {
        let start = self.i;
        for c in self.rest().chars() {
            if c == ',' || c == '(' || c == ')' || c.is_whitespace() {
                break;
            }
            self.i += c.len_utf8();
        }
        if start == self.i {
            bail!("expected an id after 'fact:'");
        }
        Ok(self.s[start..self.i].to_string())
    }
}
