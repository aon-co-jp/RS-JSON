//! **安全な厳密モード**(`full` feature、2026-10-08追加)。
//!
//! 外部から来る信頼できないJSON(HTTPのリクエスト本文・ネットワーク越しに取り込む
//! キャッシュや知識ファイル等)を読む入口向け。標準の[`parse_strict`](crate::parse_strict)
//! (中身は`serde_json`)に、次の3つの防御を足す:
//!
//! 1. **サイズ上限**(既定16 MiB): 巨大な入力でメモリを使い切らせる攻撃を、読む前に止める。
//! 2. **ネストの深さ上限**(既定64): 先に1回走査して数え、超えたらパースせずに拒否する。
//!    `serde_json`にも再帰上限(128)はあるが、より浅い上限を呼び出し側が選べる。
//! 3. **重複キーの拒否**: `{"role":"user","role":"admin"}`のように同じキーが2回出る入力を、
//!    後勝ちで黙って通さず、エラーにする(解釈のずれを突く攻撃への対策)。
//!
//! `unsafe`は使わない。値モデルは従来どおり`serde_json::Value`。
//! **正直な開示**: これは速度を上げる機能ではない(防御のための追加走査が1回入る分、
//! 標準の`parse_strict`よりわずかに遅い。実測は`examples/bench.rs`)。

use std::fmt;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use crate::full::RustJsonError;

/// 安全な厳密モードの上限。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// 入力の最大バイト数。
    pub max_bytes: usize,
    /// 配列・オブジェクトの最大ネスト深さ(`[[1]]`は深さ2)。
    pub max_depth: usize,
    /// 同じオブジェクト内の重複キーを拒否するか。
    pub reject_duplicate_keys: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_bytes: 16 * 1024 * 1024, max_depth: 64, reject_duplicate_keys: true }
    }
}

impl Limits {
    /// 小さな設定ファイルや知識ファイル向けの厳しめの上限(1 MiB・深さ32)。
    pub fn small() -> Self {
        Self { max_bytes: 1024 * 1024, max_depth: 32, reject_duplicate_keys: true }
    }
}

/// 文字列の中の括弧を数えないようにしながら、ネストの最大深さを1回の走査で求める。
/// `limit`を超えた時点で打ち切る(巨大な入力でも、超えた位置までしか読まない)。
fn depth_exceeds(bytes: &[u8], limit: usize) -> Option<usize> {
    let (mut depth, mut i) = (0usize, 0usize);
    let mut in_str = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            match b {
                b'\\' => i += 1, // 次の1文字(\" など)を読み飛ばす
                b'"' => in_str = false,
                _ => {}
            }
        } else {
            match b {
                b'"' => in_str = true,
                b'[' | b'{' => {
                    depth += 1;
                    if depth > limit {
                        return Some(depth);
                    }
                }
                b']' | b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// 重複キーを拒否しながら`serde_json::Value`を組み立てる。
struct StrictValue {
    reject_dups: bool,
}

impl<'de> DeserializeSeed<'de> for StrictValue {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(StrictVisitor { reject_dups: self.reject_dups })
    }
}

struct StrictVisitor {
    reject_dups: bool,
}

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("any valid JSON value")
    }
    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v).map(Value::Number).ok_or_else(|| E::custom("non-finite number"))
    }
    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }
    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
        while let Some(v) = seq.next_element_seed(StrictValue { reject_dups: self.reject_dups })? {
            out.push(v);
        }
        Ok(Value::Array(out))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut out = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if self.reject_dups && out.contains_key(&key) {
                return Err(de::Error::custom(format!("duplicate key `{key}`")));
            }
            let v = map.next_value_seed(StrictValue { reject_dups: self.reject_dups })?;
            out.insert(key, v);
        }
        Ok(Value::Object(out))
    }
}

/// 信頼できない入力を、上限つきの厳密モードで`serde_json::Value`にパースする。
pub fn parse_secure(input: &str, limits: &Limits) -> Result<Value, RustJsonError> {
    if input.len() > limits.max_bytes {
        return Err(RustJsonError::TooLarge { size: input.len(), max: limits.max_bytes });
    }
    if let Some(depth) = depth_exceeds(input.as_bytes(), limits.max_depth) {
        return Err(RustJsonError::TooDeep { depth, max: limits.max_depth });
    }
    let mut de = serde_json::Deserializer::from_str(input);
    let value = StrictValue { reject_dups: limits.reject_duplicate_keys }
        .deserialize(&mut de)
        .map_err(|e| duplicate_or_strict(e.to_string()))?;
    de.end().map_err(|e| RustJsonError::StrictModeRejected(e.to_string()))?;
    Ok(value)
}

/// 型付きで読む版(`#[derive(Deserialize)]`の構造体向け)。サイズと深さの上限だけを検査する
/// (構造体のフィールド重複は`serde`のderiveが標準でエラーにする)。
pub fn from_str_secure<T: serde::de::DeserializeOwned>(input: &str, limits: &Limits) -> Result<T, RustJsonError> {
    if input.len() > limits.max_bytes {
        return Err(RustJsonError::TooLarge { size: input.len(), max: limits.max_bytes });
    }
    if let Some(depth) = depth_exceeds(input.as_bytes(), limits.max_depth) {
        return Err(RustJsonError::TooDeep { depth, max: limits.max_depth });
    }
    serde_json::from_str(input).map_err(|e| RustJsonError::StrictModeRejected(e.to_string()))
}

fn duplicate_or_strict(msg: String) -> RustJsonError {
    if let Some(rest) = msg.strip_prefix("duplicate key `") {
        if let Some(end) = rest.find('`') {
            return RustJsonError::DuplicateKey(rest[..end].to_string());
        }
    }
    RustJsonError::StrictModeRejected(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lim(max_bytes: usize, max_depth: usize) -> Limits {
        Limits { max_bytes, max_depth, reject_duplicate_keys: true }
    }

    #[test]
    fn same_result_as_serde_json_for_normal_documents() {
        let doc = r#"{"a":[1,2.5,-3,true,null,"x"],"b":{"c":"d","e":[]},"u":"日本語\n\"q\""}"#;
        let secure = parse_secure(doc, &Limits::default()).unwrap();
        let std: Value = serde_json::from_str(doc).unwrap();
        assert_eq!(secure, std);
    }

    #[test]
    fn rejects_oversized_input_before_parsing() {
        let err = parse_secure(r#"{"a":"0123456789"}"#, &lim(8, 64)).unwrap_err();
        assert!(matches!(err, RustJsonError::TooLarge { max: 8, .. }), "{err:?}");
    }

    #[test]
    fn rejects_excessive_nesting_without_stack_risk() {
        let deep = "[".repeat(10_000) + &"]".repeat(10_000);
        let err = parse_secure(&deep, &lim(1 << 20, 64)).unwrap_err();
        assert!(matches!(err, RustJsonError::TooDeep { max: 64, .. }), "{err:?}");
        // 上限ちょうどは通る
        let ok = "[".repeat(64) + &"]".repeat(64);
        assert!(parse_secure(&ok, &lim(1 << 20, 64)).is_ok());
    }

    #[test]
    fn brackets_inside_strings_do_not_count_as_depth() {
        let s = format!(r#"{{"k":"{}"}}"#, "[".repeat(1000));
        assert!(parse_secure(&s, &lim(1 << 20, 4)).is_ok());
        // エスケープされた引用符の直後の括弧も文字列の一部
        let s2 = r#"{"k":"\"[[[[[[[[["}"#;
        assert!(parse_secure(s2, &lim(1 << 20, 4)).is_ok());
    }

    #[test]
    fn rejects_duplicate_keys_but_can_be_turned_off() {
        let dup = r#"{"role":"user","role":"admin"}"#;
        let err = parse_secure(dup, &Limits::default()).unwrap_err();
        assert_eq!(err, RustJsonError::DuplicateKey("role".into()));
        let lax = Limits { reject_duplicate_keys: false, ..Limits::default() };
        assert_eq!(parse_secure(dup, &lax).unwrap()["role"], "admin");
        // 入れ子の中の重複も検出する
        let nested = r#"{"a":{"x":1,"x":2}}"#;
        assert!(matches!(parse_secure(nested, &Limits::default()), Err(RustJsonError::DuplicateKey(_))));
        // 別々のオブジェクトで同じキーが出るのは重複ではない
        assert!(parse_secure(r#"[{"x":1},{"x":2}]"#, &Limits::default()).is_ok());
    }

    #[test]
    fn still_strict_rfc8259() {
        for bad in ["{a:1}", "[1,2,]", "{'a':1}", "// c\n{}", "NaN", "[1] x", ""] {
            assert!(parse_secure(bad, &Limits::default()).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn typed_variant_checks_limits() {
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct P {
            n: u32,
        }
        assert_eq!(from_str_secure::<P>(r#"{"n":3}"#, &Limits::default()).unwrap(), P { n: 3 });
        assert!(matches!(from_str_secure::<P>(r#"{"n":3}"#, &lim(3, 8)), Err(RustJsonError::TooLarge { .. })));
    }
}
