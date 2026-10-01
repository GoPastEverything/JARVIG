//! A small JSON codec for authored documents. Not a general parser and not a GPU format.
//!
//! The writer emits one stable shape. The reader rejects truncated input, trailing commas,
//! and anything after the value. Numbers are JSON numbers, not Rust debug text.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

#[derive(Debug, PartialEq, Eq)]
pub struct JsonError(pub String);

impl fmt::Display for JsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl Json {
    pub fn object(fields: Vec<(&str, Json)>) -> Self {
        Self::Object(fields.into_iter().map(|(key, value)| (key.to_string(), value)).collect())
    }

    pub fn string(value: impl Into<String>) -> Self {
        Self::String(value.into())
    }

    pub fn number(value: f64) -> Self {
        Self::Number(value)
    }

    pub fn int(value: i64) -> Self {
        Self::Number(value as f64)
    }

    pub fn bool(value: bool) -> Self {
        Self::Bool(value)
    }

    pub fn array(values: Vec<Json>) -> Self {
        Self::Array(values)
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Self::Object(fields) => fields.iter().find(|(name, _)| name == key).map(|(_, value)| value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) if value.is_finite() => Some(*value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }

    pub fn write(&self) -> String {
        let mut out = String::new();
        self.write_into(&mut out, 0);
        out.push('\n');
        out
    }

    fn write_into(&self, out: &mut String, indent: usize) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(true) => out.push_str("true"),
            Self::Bool(false) => out.push_str("false"),
            Self::Number(value) => push_number(out, *value),
            Self::String(value) => push_string(out, value),
            Self::Array(values) if values.is_empty() => out.push_str("[]"),
            Self::Array(values) => {
                out.push_str("[\n");
                for (index, value) in values.iter().enumerate() {
                    push_indent(out, indent + 1);
                    value.write_into(out, indent + 1);
                    if index + 1 != values.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                push_indent(out, indent);
                out.push(']');
            }
            Self::Object(fields) if fields.is_empty() => out.push_str("{}"),
            Self::Object(fields) => {
                out.push_str("{\n");
                for (index, (key, value)) in fields.iter().enumerate() {
                    push_indent(out, indent + 1);
                    push_string(out, key);
                    out.push_str(": ");
                    value.write_into(out, indent + 1);
                    if index + 1 != fields.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                push_indent(out, indent);
                out.push('}');
            }
        }
    }
}

pub fn parse_json(text: &str) -> Result<Json, JsonError> {
    let mut parser = Parser { bytes: text.as_bytes(), index: 0, depth: 0 };
    let value = parser.value()?;
    parser.skip();
    if parser.index != parser.bytes.len() {
        return Err(JsonError("trailing data after the JSON value".into()));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    index: usize,
    depth: u32,
}

impl<'a> Parser<'a> {
    fn value(&mut self) -> Result<Json, JsonError> {
        self.skip();
        if self.depth > 64 {
            return Err(JsonError("JSON nesting is too deep".into()));
        }
        self.depth += 1;
        let value = match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::String(self.string()?)),
            Some(b't') => self.literal(b"true", Json::Bool(true)),
            Some(b'f') => self.literal(b"false", Json::Bool(false)),
            Some(b'n') => self.literal(b"null", Json::Null),
            Some(b'-') | Some(b'0'..=b'9') => self.number(),
            Some(other) => Err(JsonError(format!("unexpected JSON byte {other}"))),
            None => Err(JsonError("truncated JSON".into())),
        };
        self.depth -= 1;
        value
    }

    fn object(&mut self) -> Result<Json, JsonError> {
        self.eat(b'{')?;
        let mut fields = Vec::new();
        self.skip();
        if self.peek() == Some(b'}') {
            self.index += 1;
            return Ok(Json::Object(fields));
        }
        loop {
            self.skip();
            if self.peek() != Some(b'"') {
                return Err(JsonError("JSON object key is not a string".into()));
            }
            let key = self.string()?;
            self.skip();
            self.eat(b':')?;
            let value = self.value()?;
            if fields.iter().any(|(name, _)| name == &key) {
                return Err(JsonError(format!("duplicate JSON key {key}")));
            }
            fields.push((key, value));
            self.skip();
            match self.peek() {
                Some(b',') => self.index += 1,
                Some(b'}') => {
                    self.index += 1;
                    break;
                }
                _ => return Err(JsonError("JSON object did not end".into())),
            }
        }
        Ok(Json::Object(fields))
    }

    fn array(&mut self) -> Result<Json, JsonError> {
        self.eat(b'[')?;
        let mut values = Vec::new();
        self.skip();
        if self.peek() == Some(b']') {
            self.index += 1;
            return Ok(Json::Array(values));
        }
        loop {
            values.push(self.value()?);
            self.skip();
            match self.peek() {
                Some(b',') => self.index += 1,
                Some(b']') => {
                    self.index += 1;
                    break;
                }
                _ => return Err(JsonError("JSON array did not end".into())),
            }
        }
        Ok(Json::Array(values))
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.eat(b'"')?;
        let mut out = String::new();
        while let Some(byte) = self.peek() {
            self.index += 1;
            match byte {
                b'"' => return Ok(out),
                b'\\' => {
                    let escaped = self.peek().ok_or_else(|| JsonError("truncated JSON string".into()))?;
                    self.index += 1;
                    match escaped {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut hex = [0u8; 4];
                            for slot in &mut hex {
                                *slot = self.peek().ok_or_else(|| JsonError("truncated JSON unicode escape".into()))?;
                                self.index += 1;
                            }
                            let text = std::str::from_utf8(&hex).map_err(|_| JsonError("bad JSON unicode escape".into()))?;
                            let code = u32::from_str_radix(text, 16).map_err(|_| JsonError("bad JSON unicode escape".into()))?;
                            let character = char::from_u32(code).ok_or_else(|| JsonError("bad JSON unicode escape".into()))?;
                            out.push(character);
                        }
                        _ => return Err(JsonError("bad JSON string escape".into())),
                    }
                }
                byte if byte < 0x20 => return Err(JsonError("raw control character in a JSON string".into())),
                byte => {
                    let width = utf8_width(byte).ok_or_else(|| JsonError("bad UTF-8 in a JSON string".into()))?;
                    let start = self.index - 1;
                    if self.index + width - 1 > self.bytes.len() {
                        return Err(JsonError("truncated UTF-8 in a JSON string".into()));
                    }
                    self.index += width - 1;
                    let text = std::str::from_utf8(&self.bytes[start..self.index]).map_err(|_| JsonError("bad UTF-8 in a JSON string".into()))?;
                    out.push_str(text);
                }
            }
        }
        Err(JsonError("truncated JSON string".into()))
    }

    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.index;
        if self.peek() == Some(b'-') {
            self.index += 1;
        }
        match self.peek() {
            Some(b'0') => self.index += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.index += 1;
                }
            }
            _ => return Err(JsonError("truncated JSON number".into())),
        }
        if self.peek() == Some(b'.') {
            self.index += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(JsonError("truncated JSON fraction".into()));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.index += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.index += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.index += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(JsonError("truncated JSON exponent".into()));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.index += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.index]).map_err(|_| JsonError("bad JSON number".into()))?;
        let value = text.parse::<f64>().map_err(|_| JsonError("JSON number is not finite".into()))?;
        if !value.is_finite() {
            return Err(JsonError("JSON number is not finite".into()));
        }
        Ok(Json::Number(value))
    }

    fn literal(&mut self, text: &[u8], value: Json) -> Result<Json, JsonError> {
        if self.bytes[self.index..].starts_with(text) {
            self.index += text.len();
            Ok(value)
        } else {
            Err(JsonError("truncated JSON literal".into()))
        }
    }

    fn eat(&mut self, expected: u8) -> Result<(), JsonError> {
        if self.peek() == Some(expected) {
            self.index += 1;
            Ok(())
        } else {
            Err(JsonError(format!("expected JSON byte {expected}")))
        }
    }

    fn skip(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.index += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }
}

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

fn push_number(out: &mut String, value: f64) {
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        out.push_str(&format!("{}", value as i64));
        return;
    }
    // Authored floats are f32 factors stored in f64. Print the f32 text when that is exact.
    let narrow = value as f32;
    if (narrow as f64) == value {
        out.push_str(&format!("{narrow}"));
        return;
    }
    out.push_str(&format!("{value}"));
}

/// The f64 this writer will read back. Exact f32 values often print as a shorter
/// spelling, and that spelling is a different f64. Callers that compare a parsed
/// document with `PartialEq` store this value.
pub(crate) fn round_trip_number(value: f64) -> f64 {
    let mut text = String::new();
    push_number(&mut text, value);
    text.parse::<f64>().unwrap_or(value)
}

fn push_string(out: &mut String, value: &str) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            character if (character as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", character as u32)),
            character => out.push(character),
        }
    }
    out.push('"');
}

fn utf8_width(byte: u8) -> Option<usize> {
    if byte < 0x80 {
        Some(1)
    } else if byte & 0b1110_0000 == 0b1100_0000 {
        Some(2)
    } else if byte & 0b1111_0000 == 0b1110_0000 {
        Some(3)
    } else if byte & 0b1111_1000 == 0b1111_0000 {
        Some(4)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_number_is_stable_for_exact_f32_values() {
        for bits in [0xbe40000000000000_u64, 0x3feecf1180000000, 0xc000305160000000] {
            let value = f64::from_bits(bits);
            let once = round_trip_number(value);
            assert_eq!(round_trip_number(once), once);
            let text = Json::number(once).write();
            assert_eq!(parse_json(&text).expect("number"), Json::number(once), "{text}");
        }
        assert_eq!(Json::number(1.5).write().trim(), "1.5");
        assert_eq!(Json::number(100.0).write().trim(), "100");
    }
}
