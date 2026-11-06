//! Custom object bounds: numbers, or arithmetic over parameter values (T82).
//!
//! `objectBounds` corners are evaluated on the CPU, so culling, picking, selection
//! and Fit Model follow the parameters without running the shader. Expressions
//! support `+ - * /`, parentheses, unary minus, `min(a, b, …)`, `max(a, b, …)`,
//! `abs(a)`, numbers, and parameter IDs; vector parameters need a component
//! (`size.y`, also `.r .g .b .a`). Booleans read as 0 or 1.
use super::*;

/// One bounds coordinate: a number or an expression over parameters.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum BoundValue {
    Number(f64),
    Expression(String),
}
impl From<f64> for BoundValue {
    fn from(n: f64) -> Self {
        Self::Number(n)
    }
}
impl From<&str> for BoundValue {
    fn from(s: &str) -> Self {
        Self::Expression(s.into())
    }
}

/// Local box enclosing everything a custom object can emit.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ObjectBounds {
    pub min: [BoundValue; 3],
    pub max: [BoundValue; 3],
}

impl BoundValue {
    /// Checks syntax and parameter references without evaluating.
    pub fn check(&self, parameters: &[Parameter]) -> Result<(), String> {
        match self {
            Self::Number(n) if n.is_finite() => Ok(()),
            Self::Number(_) => Err("objectBounds numbers must be finite".into()),
            Self::Expression(text) => Parser::new(text, &Lookup::Check(parameters))
                .parse()
                .map(|_| ()),
        }
    }
    pub fn evaluate(&self, values: &BTreeMap<String, Value>) -> Result<f64, String> {
        let n = match self {
            Self::Number(n) => *n,
            Self::Expression(text) => Parser::new(text, &Lookup::Values(values)).parse()?,
        };
        if n.is_finite() {
            Ok(n)
        } else {
            Err("objectBounds evaluated to a non-finite number".into())
        }
    }
}

enum Lookup<'a> {
    Check(&'a [Parameter]),
    Values(&'a BTreeMap<String, Value>),
}
impl Lookup<'_> {
    fn read(&self, id: &str, component: Option<usize>) -> Result<f64, String> {
        match self {
            Self::Check(parameters) => {
                let p = parameters
                    .iter()
                    .find(|p| p.id == id)
                    .ok_or_else(|| format!("objectBounds uses unknown parameter {id}"))?;
                let width = match p.kind.as_str() {
                    "float" | "integer" | "unsigned" | "seed" | "boolean" => 0,
                    "vec2" => 2,
                    "vec3" => 3,
                    "vec4" | "color" => 4,
                    kind => return Err(format!("objectBounds can't read {kind} parameter {id}")),
                };
                match (width, component) {
                    (0, None) => Ok(1.),
                    (0, Some(_)) => Err(format!("{id} is a number and has no components")),
                    (_, None) => Err(format!("{id} is a vector: use {id}.x, {id}.y …")),
                    (w, Some(c)) if c < w => Ok(1.),
                    (_, Some(_)) => Err(format!("{id} has no such component")),
                }
            }
            Self::Values(values) => {
                let v = values
                    .get(id)
                    .ok_or_else(|| format!("objectBounds uses unknown parameter {id}"))?;
                let v = match component {
                    Some(c) => v.get(c).unwrap_or(&Value::Null),
                    None => v,
                };
                v.as_f64()
                    .or_else(|| v.as_bool().map(|b| f64::from(u8::from(b))))
                    .ok_or_else(|| format!("objectBounds can't read {id}"))
            }
        }
    }
}

struct Parser<'a> {
    text: &'a [u8],
    at: usize,
    lookup: &'a Lookup<'a>,
}
impl<'a> Parser<'a> {
    fn new(text: &'a str, lookup: &'a Lookup<'a>) -> Self {
        Self {
            text: text.as_bytes(),
            at: 0,
            lookup,
        }
    }
    fn parse(mut self) -> Result<f64, String> {
        if self.text.len() > 256 {
            return Err("objectBounds expressions are limited to 256 characters".into());
        }
        let v = self.sum(0)?;
        self.space();
        if self.at < self.text.len() {
            return Err(self.error("unexpected text"));
        }
        Ok(v)
    }
    fn error(&self, what: &str) -> String {
        format!(
            "objectBounds expression \"{}\": {what} at character {}",
            String::from_utf8_lossy(self.text),
            self.at + 1
        )
    }
    fn space(&mut self) {
        while self.text.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        self.space();
        if self.text.get(self.at) == Some(&c) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn sum(&mut self, depth: u8) -> Result<f64, String> {
        let mut v = self.product(depth)?;
        loop {
            if self.eat(b'+') {
                v += self.product(depth)?;
            } else if self.eat(b'-') {
                v -= self.product(depth)?;
            } else {
                return Ok(v);
            }
        }
    }
    fn product(&mut self, depth: u8) -> Result<f64, String> {
        let mut v = self.unary(depth)?;
        loop {
            if self.eat(b'*') {
                v *= self.unary(depth)?;
            } else if self.eat(b'/') {
                v /= self.unary(depth)?;
            } else {
                return Ok(v);
            }
        }
    }
    fn unary(&mut self, depth: u8) -> Result<f64, String> {
        if depth > 32 {
            return Err(self.error("nesting is too deep"));
        }
        if self.eat(b'-') {
            return Ok(-self.unary(depth + 1)?);
        }
        if self.eat(b'+') {
            return self.unary(depth + 1);
        }
        if self.eat(b'(') {
            let v = self.sum(depth + 1)?;
            if !self.eat(b')') {
                return Err(self.error("expected )"));
            }
            return Ok(v);
        }
        self.space();
        let start = self.at;
        match self.text.get(self.at) {
            Some(c) if c.is_ascii_digit() || *c == b'.' => {
                while self
                    .text
                    .get(self.at)
                    .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
                {
                    self.at += 1;
                }
                if self
                    .text
                    .get(self.at)
                    .is_some_and(|c| *c == b'e' || *c == b'E')
                {
                    self.at += 1;
                    if self
                        .text
                        .get(self.at)
                        .is_some_and(|c| *c == b'+' || *c == b'-')
                    {
                        self.at += 1;
                    }
                    while self.text.get(self.at).is_some_and(u8::is_ascii_digit) {
                        self.at += 1;
                    }
                }
                std::str::from_utf8(&self.text[start..self.at])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| self.error("invalid number"))
            }
            Some(c) if c.is_ascii_alphabetic() || *c == b'_' => {
                while self
                    .text
                    .get(self.at)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
                {
                    self.at += 1;
                }
                let name = String::from_utf8_lossy(&self.text[start..self.at]).into_owned();
                if self.eat(b'(') {
                    return self.call(&name, depth);
                }
                let component = if self.eat(b'.') {
                    let c = self.text.get(self.at).copied();
                    self.at += 1;
                    Some(match c {
                        Some(b'x' | b'r') => 0,
                        Some(b'y' | b'g') => 1,
                        Some(b'z' | b'b') => 2,
                        Some(b'w' | b'a') => 3,
                        _ => return Err(self.error("expected a component x, y, z or w")),
                    })
                } else {
                    None
                };
                self.lookup.read(&name, component)
            }
            _ => Err(self.error("expected a number, parameter or (")),
        }
    }
    fn call(&mut self, name: &str, depth: u8) -> Result<f64, String> {
        let mut args = vec![self.sum(depth + 1)?];
        while self.eat(b',') {
            args.push(self.sum(depth + 1)?);
        }
        if !self.eat(b')') {
            return Err(self.error("expected )"));
        }
        match (name, args.as_slice()) {
            ("abs", [a]) => Ok(a.abs()),
            ("min", [_, _, ..]) => Ok(args.iter().copied().fold(f64::INFINITY, f64::min)),
            ("max", [_, _, ..]) => Ok(args.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
            _ => Err(self.error(&format!(
                "unknown function {name}: use abs(a), min(a, b, …) or max(a, b, …)"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn parameter(id: &str, kind: &str, default: Value) -> Parameter {
        Parameter {
            id: id.into(),
            kind: kind.into(),
            default,
            label: String::new(),
            description: String::new(),
            unit: String::new(),
            min: None,
            max: None,
            step: None,
            options: BTreeMap::new(),
            extensions: BTreeMap::new(),
        }
    }
    #[test]
    fn expressions_follow_parameters_and_reject_bad_input() {
        let parameters = [
            parameter("size", "vec3", json!([2.0, 3.0, 4.0])),
            parameter("count", "integer", json!(5)),
            parameter("open", "boolean", json!(true)),
            parameter("style", "enum", json!("a")),
        ];
        let values = BTreeMap::from([
            ("size".to_owned(), json!([2.0, 3.0, 4.0])),
            ("count".to_owned(), json!(5)),
            ("open".to_owned(), json!(true)),
        ]);
        for (text, expected) in [
            ("count * size.y", 15.),
            ("-size.x / 2", -1.),
            ("(count - 1) * 2 + 1", 9.),
            ("max(size.x, size.z, 1)", 4.),
            ("min(size.x, 1.5e0)", 1.5),
            ("abs(-count) + open", 6.),
            ("size.r + size.b", 6.),
            (" 0.25 ", 0.25),
        ] {
            let v = BoundValue::from(text);
            v.check(&parameters).unwrap();
            assert_eq!(v.evaluate(&values).unwrap(), expected, "{text}");
        }
        for text in [
            "",
            "count *",
            "size",
            "count.x",
            "size.q",
            "missing + 1",
            "style",
            "pow(2, 3)",
            "min(1)",
            "(1 + 2",
            "1 2",
            "#",
        ] {
            assert!(BoundValue::from(text).check(&parameters).is_err(), "{text}");
        }
        assert!(
            BoundValue::from("1 / (count - 5)")
                .evaluate(&values)
                .is_err()
        );
        assert!(BoundValue::Number(f64::NAN).check(&parameters).is_err());
        let deep = format!("{}1{}", "(".repeat(40), ")".repeat(40));
        assert!(BoundValue::from(deep.as_str()).check(&parameters).is_err());
        assert!(
            BoundValue::from("1+".repeat(200).as_str())
                .check(&parameters)
                .is_err()
        );
        // Plain numbers and expressions serialize as themselves.
        let bounds = ObjectBounds {
            min: [0.0.into(), "-1".into(), 0.0.into()],
            max: [1.0.into(), "count".into(), 1.0.into()],
        };
        let text = serde_json::to_value(&bounds).unwrap();
        assert_eq!(
            text,
            json!({"min": [0.0, "-1", 0.0], "max": [1.0, "count", 1.0]})
        );
        assert_eq!(
            serde_json::from_value::<ObjectBounds>(text).unwrap(),
            bounds
        );
    }
}
