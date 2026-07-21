//! `light` — ブラウザ組み込みの`JSON.parse`(`js_sys::JSON`)相当を、
//! **`serde_json`にも一切依存せず**Rustで実装したもの。
//!
//! ## なぜこのモジュールが必要か(2026-07-21追加)
//!
//! このクレートの[`parse`](crate::parse)/[`parse_strict`](crate::parse_strict)は、
//! パース処理自体は手書き(または`serde_json::from_str`委譲)だが、
//! **出力の値モデルとして`serde_json::Value`/`Map`/`Number`を使う**。
//! WASM(ブラウザ向けビルド)でこのクレートを使う場合、`serde_json`が
//! 引き込む依存グラフの分だけWASMバイナリが肥大化する
//! ([`RGit`](https://github.com/aon-co-jp/RGit)の`web/`クレートで実際に
//! 問題になった)。
//!
//! `light`モジュールは、パース結果を`serde_json`型ではなく本モジュール
//! 独自の[`LightValue`]に格納することで、この依存を完全に断ち切る。
//! ブラウザの`JSON.parse`と同じくRFC 8259厳密モードのみを受理する
//! (このクレートの寛容モードが持つコメント・トレイリングカンマ等の
//! 拡張は`light`では受理しない——ブラウザの`JSON.parse`自体がそれらを
//! 受理しないことに合わせた、意図的な仕様)。

use std::iter::Peekable;
use std::str::Chars;

/// [`crate::RustJsonError`]とは別の、`light`専用の最小エラー型
/// (`serde_json`のエラー型に依存しないための独立型)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightError(pub String);

impl std::fmt::Display for LightError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "light JSON parse error: {}", self.0)
    }
}

impl std::error::Error for LightError {}

/// `serde_json::Value`に相当する、依存ゼロの値モデル。
#[derive(Debug, Clone, PartialEq)]
pub enum LightValue {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<LightValue>),
    Object(Vec<(String, LightValue)>),
}

impl LightValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            LightValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            LightValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            LightValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[LightValue]> {
        match self {
            LightValue::Array(items) => Some(items),
            _ => None,
        }
    }

    /// オブジェクトの1フィールドを名前で取得する。
    pub fn get(&self, key: &str) -> Option<&LightValue> {
        match self {
            LightValue::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// [`crate::extract_path`]と同じドット/ブラケット記法のパス抽出。
    /// `light`は独自の値モデルを持つため、同じロジックを重複させず
    /// ここに小さく再実装する(依存ゼロという`light`の制約上、
    /// `serde_json::Value`ベースの`extract_path`は再利用できない)。
    pub fn extract_path(&self, path: &str) -> Option<&LightValue> {
        if path.is_empty() {
            return Some(self);
        }
        let mut current = self;
        for dot_part in path.split('.') {
            let mut rest = dot_part;
            loop {
                if let Some(bracket_start) = rest.find('[') {
                    let (key_part, bracket_and_after) = rest.split_at(bracket_start);
                    if !key_part.is_empty() {
                        current = current.get(key_part)?;
                    }
                    let bracket_end = bracket_and_after.find(']')?;
                    let idx: usize = bracket_and_after[1..bracket_end].parse().ok()?;
                    current = current.as_array()?.get(idx)?;
                    rest = &bracket_and_after[bracket_end + 1..];
                    if rest.is_empty() {
                        break;
                    }
                } else {
                    if !rest.is_empty() {
                        current = current.get(rest)?;
                    }
                    break;
                }
            }
        }
        Some(current)
    }
}

/// ブラウザの`JSON.parse`相当(RFC 8259厳密モード、依存ゼロ)。
pub fn parse_light(input: &str) -> Result<LightValue, LightError> {
    let mut chars = input.chars().peekable();
    skip_whitespace(&mut chars);
    let value = parse_value(&mut chars)?;
    skip_whitespace(&mut chars);
    if chars.next().is_some() {
        return Err(LightError("trailing data after top-level value".to_string()));
    }
    Ok(value)
}

fn skip_whitespace(chars: &mut Peekable<Chars>) {
    while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
        chars.next();
    }
}

fn parse_value(chars: &mut Peekable<Chars>) -> Result<LightValue, LightError> {
    skip_whitespace(chars);
    match chars.peek() {
        Some('"') => parse_string(chars).map(LightValue::Str),
        Some('{') => parse_object(chars),
        Some('[') => parse_array(chars),
        Some('t') => parse_literal(chars, "true", LightValue::Bool(true)),
        Some('f') => parse_literal(chars, "false", LightValue::Bool(false)),
        Some('n') => parse_literal(chars, "null", LightValue::Null),
        Some(c) if c.is_ascii_digit() || *c == '-' => parse_number(chars),
        other => Err(LightError(format!("unexpected token {other:?}"))),
    }
}

fn parse_literal(chars: &mut Peekable<Chars>, literal: &str, value: LightValue) -> Result<LightValue, LightError> {
    for expected in literal.chars() {
        match chars.next() {
            Some(c) if c == expected => {}
            other => return Err(LightError(format!("expected '{literal}', got {other:?}"))),
        }
    }
    Ok(value)
}

fn parse_number(chars: &mut Peekable<Chars>) -> Result<LightValue, LightError> {
    let mut buf = String::new();
    while matches!(chars.peek(), Some(c) if c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')) {
        buf.push(chars.next().unwrap());
    }
    buf.parse::<f64>().map(LightValue::Number).map_err(|e| LightError(format!("invalid number '{buf}': {e}")))
}

fn parse_string(chars: &mut Peekable<Chars>) -> Result<String, LightError> {
    if chars.next() != Some('"') {
        return Err(LightError("expected opening '\"'".to_string()));
    }
    let mut out = String::new();
    loop {
        match chars.next() {
            None => return Err(LightError("unterminated string".to_string())),
            Some('"') => return Ok(out),
            Some('\\') => match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('b') => out.push('\u{8}'),
                Some('f') => out.push('\u{c}'),
                Some('u') => {
                    let code = parse_hex4(chars)?;
                    if (0xD800..=0xDBFF).contains(&code) {
                        if chars.next() == Some('\\') && chars.next() == Some('u') {
                            let low = parse_hex4(chars)?;
                            let c = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                            if let Some(ch) = char::from_u32(c) {
                                out.push(ch);
                            }
                        }
                    } else if let Some(ch) = char::from_u32(code) {
                        out.push(ch);
                    }
                }
                other => return Err(LightError(format!("invalid escape {other:?}"))),
            },
            Some(c) => out.push(c),
        }
    }
}

fn parse_hex4(chars: &mut Peekable<Chars>) -> Result<u32, LightError> {
    let mut hex = String::with_capacity(4);
    for _ in 0..4 {
        hex.push(chars.next().ok_or_else(|| LightError("truncated \\u escape".to_string()))?);
    }
    u32::from_str_radix(&hex, 16).map_err(|e| LightError(format!("invalid \\u escape '{hex}': {e}")))
}

fn parse_array(chars: &mut Peekable<Chars>) -> Result<LightValue, LightError> {
    chars.next(); // '['
    let mut items = Vec::new();
    skip_whitespace(chars);
    if chars.peek() == Some(&']') {
        chars.next();
        return Ok(LightValue::Array(items));
    }
    loop {
        items.push(parse_value(chars)?);
        skip_whitespace(chars);
        match chars.next() {
            Some(',') => continue,
            Some(']') => break,
            other => return Err(LightError(format!("expected ',' or ']' in array, got {other:?}"))),
        }
    }
    Ok(LightValue::Array(items))
}

fn parse_object(chars: &mut Peekable<Chars>) -> Result<LightValue, LightError> {
    chars.next(); // '{'
    let mut fields = Vec::new();
    skip_whitespace(chars);
    if chars.peek() == Some(&'}') {
        chars.next();
        return Ok(LightValue::Object(fields));
    }
    loop {
        skip_whitespace(chars);
        let key = parse_string(chars)?;
        skip_whitespace(chars);
        match chars.next() {
            Some(':') => {}
            other => return Err(LightError(format!("expected ':' after key, got {other:?}"))),
        }
        let value = parse_value(chars)?;
        fields.push((key, value));
        skip_whitespace(chars);
        match chars.next() {
            Some(',') => continue,
            Some('}') => break,
            other => return Err(LightError(format!("expected ',' or '}}' in object, got {other:?}"))),
        }
    }
    Ok(LightValue::Object(fields))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_array() {
        let v = parse_light(r#"["a.git","b.git"]"#).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].as_str(), Some("a.git"));
        assert_eq!(arr[1].as_str(), Some("b.git"));
    }

    #[test]
    fn parses_object_with_escapes_and_unicode() {
        let v = parse_light(r#"{"branch":"main","content":"line1\nline2 \"quoted\" 日本"}"#).unwrap();
        assert_eq!(v.get("branch").and_then(LightValue::as_str), Some("main"));
        assert_eq!(v.get("content").and_then(LightValue::as_str), Some("line1\nline2 \"quoted\" 日本"));
    }

    #[test]
    fn parses_empty_array_and_object() {
        assert!(parse_light("[]").unwrap().as_array().unwrap().is_empty());
        assert!(matches!(parse_light("{}").unwrap(), LightValue::Object(f) if f.is_empty()));
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(parse_light("{\"a\":").is_err());
        assert!(parse_light("[1,2,").is_err());
        assert!(parse_light("\"unterminated").is_err());
    }

    #[test]
    fn rejects_lenient_extensions_unlike_crates_parse() {
        // browser JSON.parse (and thus `light`) rejects trailing commas,
        // comments, unquoted keys -- unlike this crate's top-level `parse`.
        assert!(parse_light(r#"{"a": 1,}"#).is_err());
        assert!(parse_light("{name: 1}").is_err());
    }

    #[test]
    fn extract_path_matches_crate_level_semantics() {
        let v = parse_light(r#"{"items":[{"name":"sword"},{"name":"shield"}]}"#).unwrap();
        assert_eq!(v.extract_path("items[1].name").and_then(LightValue::as_str), Some("shield"));
        assert_eq!(v.extract_path("items[9].name"), None);
    }
}
