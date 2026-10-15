//! 高速な読み書きと、JSON <-> RS-JSON(寛容な上位集合)の相互変換(任意機能`fast`)。
//!
//! 少量(数百バイト)から大量(数十MB)まで、`sonic-rs`(SIMD)で処理する。
//! - **型付き構造体**へ直接読む/書く: [`from_str`]・[`from_slice`]・[`to_string`]・[`to_vec`]・[`to_string_pretty`]
//!   (`serde_json::Value`を経由しないので速い。**信頼できる入力用**。外部入力は`parse_secure_sonic`)。
//! - **RS-JSON → JSON**: [`rsjson_to_json`]。厳密JSONならそのまま高速経路、コメント・末尾カンマ・
//!   引用符無しキー・単引用符文字列を含むときだけ寛容パーサへ回す。
//! - **JSON → RS-JSON**: [`json_to_rsjson`]。キーが識別子なら引用符を外した、人が編集しやすい形で出す。
//! - 型付き: [`from_rsjson`](RS-JSON文字列→構造体)・[`to_rsjson`](構造体→RS-JSON文字列)。
//!
//! 依存先の`sonic-rs`は内部で`unsafe`を使う(このクレート自身のコードに`unsafe`は無い)。

use crate::full::RustJsonError;
use serde::{Deserialize, Serialize};
use sonic_rs::{JsonContainerTrait, JsonValueTrait};

pub use sonic_rs;

fn rej(e: impl std::fmt::Display) -> RustJsonError {
    RustJsonError::StrictModeRejected(e.to_string())
}

/// `serde_json::from_str::<T>`の置き換え。
pub fn from_str<'a, T: Deserialize<'a>>(s: &'a str) -> Result<T, sonic_rs::Error> {
    sonic_rs::from_str(s)
}

/// `serde_json::from_slice::<T>`の置き換え。
pub fn from_slice<'a, T: Deserialize<'a>>(b: &'a [u8]) -> Result<T, sonic_rs::Error> {
    sonic_rs::from_slice(b)
}

/// `serde_json::to_string`の置き換え。
pub fn to_string<T: Serialize + ?Sized>(v: &T) -> Result<String, sonic_rs::Error> {
    sonic_rs::to_string(v)
}

/// `serde_json::to_vec`の置き換え。
pub fn to_vec<T: Serialize + ?Sized>(v: &T) -> Result<Vec<u8>, sonic_rs::Error> {
    sonic_rs::to_vec(v)
}

/// `serde_json::to_string_pretty`の置き換え。
pub fn to_string_pretty<T: Serialize + ?Sized>(v: &T) -> Result<String, sonic_rs::Error> {
    sonic_rs::to_string_pretty(v)
}

/// 値を作らず1パスで寛容構文を厳密JSONへ書き換える(コメント削除・末尾カンマ削除・
/// 引用符無しキーと単引用符文字列を二重引用符へ)。結果は**必ず検証される**ので、走査の誤りが
/// そのまま出力されることはない。空白は取り除く(キー順は保持)。
fn lenient_to_strict_text(input: &str) -> String {
    let b = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    let mut pending_comma = false;
    macro_rules! flush_comma {
        () => {
            if pending_comma {
                out.push(b',');
                pending_comma = false;
            }
        };
    }
    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            }
            b',' => {
                pending_comma = true;
                i += 1;
            }
            b']' | b'}' => {
                pending_comma = false; // 末尾カンマ
                out.push(c);
                i += 1;
            }
            b'"' | b'\'' => {
                flush_comma!();
                let q = c;
                out.push(b'"');
                i += 1;
                while i < b.len() && b[i] != q {
                    if b[i] == b'\\' && i + 1 < b.len() {
                        if q == b'\'' && b[i + 1] == b'\'' {
                            out.push(b'\''); // \' は ' に
                        } else {
                            out.extend_from_slice(&b[i..i + 2]);
                        }
                        i += 2;
                    } else {
                        if q == b'\'' && b[i] == b'"' {
                            out.push(b'\\');
                        }
                        out.push(b[i]);
                        i += 1;
                    }
                }
                out.push(b'"');
                i += 1;
            }
            _ if c.is_ascii_alphabetic() || c == b'_' => {
                flush_comma!();
                let s = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                let w = &b[s..i];
                if w == b"true" || w == b"false" || w == b"null" {
                    out.extend_from_slice(w);
                } else {
                    out.push(b'"'); // 引用符無しキー
                    out.extend_from_slice(w);
                    out.push(b'"');
                }
            }
            _ => {
                flush_comma!();
                out.push(c);
                i += 1;
            }
        }
    }
    // 入力は正しいUTF-8で、書き換えはASCII単位のみなのでUTF-8は保たれる
    String::from_utf8(out).unwrap_or_default()
}

/// RS-JSON(寛容)文字列 → 厳密JSON文字列。
/// 厳密JSONならそのまま検証だけ(高速経路)。寛容構文があれば1パス走査で書き換えてから検証する。
/// 走査結果が検証に通らないときだけ、位置つきエラーのために従来の寛容パーサへ回す。
pub fn rsjson_to_json(input: &str) -> Result<String, RustJsonError> {
    use serde::de::IgnoredAny;
    if sonic_rs::from_str::<IgnoredAny>(input).is_ok() {
        return Ok(input.to_string());
    }
    let text = lenient_to_strict_text(input);
    if sonic_rs::from_str::<IgnoredAny>(&text).is_ok() {
        return Ok(text);
    }
    let v = crate::parse(input)?; // 走査の取りこぼし、または位置つきエラー
    sonic_rs::to_string(&v).map_err(rej)
}

/// RS-JSON(寛容)文字列 → 型付き構造体。
pub fn from_rsjson<T: serde::de::DeserializeOwned>(input: &str) -> Result<T, RustJsonError> {
    if let Ok(t) = sonic_rs::from_str::<T>(input) {
        return Ok(t);
    }
    let v = crate::parse(input)?;
    serde_json::from_value(v).map_err(rej)
}

fn is_ident(k: &str) -> bool {
    let mut c = k.chars();
    matches!(c.next(), Some(f) if f.is_ascii_alphabetic() || f == '_') && c.all(|x| x.is_ascii_alphanumeric() || x == '_')
}

fn write_rs(v: &sonic_rs::Value, pretty: bool, depth: usize, out: &mut String) {
    let nl = |out: &mut String, d: usize| {
        if pretty {
            out.push('\n');
            for _ in 0..d {
                out.push_str("  ");
            }
        }
    };
    if let Some(o) = v.as_object() {
        if o.is_empty() {
            out.push_str("{}");
            return;
        }
        out.push('{');
        for (i, (k, val)) in o.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            nl(out, depth + 1);
            if is_ident(k) {
                out.push_str(k);
            } else {
                out.push_str(&sonic_rs::to_string(k).unwrap_or_default());
            }
            out.push_str(if pretty { ": " } else { ":" });
            write_rs(val, pretty, depth + 1, out);
        }
        nl(out, depth);
        out.push('}');
    } else if let Some(a) = v.as_array() {
        if a.is_empty() {
            out.push_str("[]");
            return;
        }
        out.push('[');
        for (i, val) in a.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            nl(out, depth + 1);
            write_rs(val, pretty, depth + 1, out);
        }
        nl(out, depth);
        out.push(']');
    } else {
        // 文字列・数値・真偽・nullは厳密JSONと同じ表記
        out.push_str(&sonic_rs::to_string(v).unwrap_or_default());
    }
}

/// 検証済みの厳密JSONから、識別子だけのキーの引用符を外す(空白・順序はそのまま)。
fn unquote_keys(input: &str) -> String {
    let b = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'"' {
            let s = i;
            i += 1;
            let mut simple = true;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' {
                    simple = false;
                    i += 1;
                }
                i += 1;
            }
            let end = (i + 1).min(b.len()); // 閉じ引用符の次
            let body = &b[s + 1..i.min(b.len())];
            let mut j = end;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            let is_key = b.get(j) == Some(&b':');
            let ident = !body.is_empty()
                && (body[0].is_ascii_alphabetic() || body[0] == b'_')
                && body.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_');
            if is_key && simple && ident && body != b"true" && body != b"false" && body != b"null" {
                out.extend_from_slice(body);
            } else {
                out.extend_from_slice(&b[s..end]);
            }
            i = end;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

/// 厳密JSON文字列 → RS-JSON文字列(識別子のキーは引用符無し)。`pretty`で整形。
pub fn json_to_rsjson(input: &str, pretty: bool) -> Result<String, RustJsonError> {
    if !pretty {
        // 値を作らず、検証してから1パスでキーの引用符だけ外す
        sonic_rs::from_str::<serde::de::IgnoredAny>(input).map_err(rej)?;
        return Ok(unquote_keys(input));
    }
    let v: sonic_rs::Value = sonic_rs::from_str(input).map_err(rej)?;
    let mut out = String::with_capacity(input.len());
    write_rs(&v, pretty, 0, &mut out);
    Ok(out)
}

/// 構造体 → RS-JSON文字列。
pub fn to_rsjson<T: Serialize + ?Sized>(v: &T, pretty: bool) -> Result<String, RustJsonError> {
    let val: sonic_rs::Value = sonic_rs::to_value(v).map_err(rej)?;
    let mut out = String::new();
    write_rs(&val, pretty, 0, &mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct S {
        name: String,
        n: Vec<i64>,
        #[serde(rename = "a-b")]
        ab: bool,
    }

    #[test]
    fn typed_roundtrip() {
        let s = S { name: "x\"y".into(), n: vec![1, 2], ab: true };
        let j = to_string(&s).unwrap();
        assert_eq!(from_str::<S>(&j).unwrap(), s);
        assert_eq!(from_slice::<S>(&to_vec(&s).unwrap()).unwrap(), s);
        assert_eq!(from_str::<S>(&to_string_pretty(&s).unwrap()).unwrap(), s);
    }

    #[test]
    fn rsjson_to_json_handles_lenient_and_strict() {
        let lenient = "{ // c\n name: 'sword', n: [1,2,], }";
        let j = rsjson_to_json(lenient).unwrap();
        assert_eq!(j, r#"{"name":"sword","n":[1,2]}"#);
        assert_eq!(rsjson_to_json(r#"{ "a" : 1 }"#).unwrap(), r#"{ "a" : 1 }"#);
        let tricky = r#"{u:"http://x//y", s:'it\'s "q"', t:true, n:null, /* c */ a:[{k:1,},],}"#;
        let got: serde_json::Value = serde_json::from_str(&rsjson_to_json(tricky).unwrap()).unwrap();
        assert_eq!(got, serde_json::json!({"u":"http://x//y","s":"it's \"q\"","t":true,"n":null,"a":[{"k":1}]}));
        assert!(rsjson_to_json("{").is_err());
    }

    #[test]
    fn json_to_rsjson_and_back_same_value() {
        let src = r#"{"name":"a b","a-b":[1,2.5,null,true],"o":{},"e":[]}"#;
        for pretty in [false, true] {
            let rs = json_to_rsjson(src, pretty).unwrap();
            assert!(rs.contains("name:") && rs.contains("\"a-b\""), "{rs}");
            let back = rsjson_to_json(&rs).unwrap();
            let a: serde_json::Value = serde_json::from_str(src).unwrap();
            let b: serde_json::Value = serde_json::from_str(&back).unwrap();
            assert_eq!(a, b);
        }
    }

    #[test]
    fn typed_rsjson() {
        let s = S { name: "n".into(), n: vec![3], ab: false };
        let rs = to_rsjson(&s, true).unwrap();
        assert_eq!(from_rsjson::<S>(&rs).unwrap(), s);
        let lenient = "{name:'n', n:[3,], \"a-b\":false, /*c*/}";
        assert_eq!(from_rsjson::<S>(lenient).unwrap(), s);
    }
}
