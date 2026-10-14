# PORTING.md — お引越し可能ファイル

他のプロジェクトへそのまま(または軽微な変更で)移植できる実装パターン一覧。

## `parse()` / `parse_strict()`の二段構え設計(`src/lib.rs`)

「寛容モードはJSON5/JSONC相当の自前パーサー、厳密モードは
`serde_json::from_str`への委譲」という設計は、既存の厳密JSONパーサー
(`serde_json`)を持つプロジェクトに、破壊的変更無しで寛容モードを
追加したい場合にそのまま応用できる。厳密モード自体を自作する必要が
無い点がポイント(厳密JSONの定義そのものが`serde_json`の受理範囲と
一致するため)。

```rust
pub fn parse_strict(input: &str) -> Result<Value, RustJsonError> {
    serde_json::from_str(input).map_err(|e| RustJsonError::StrictModeRejected(e.to_string()))
}
```

## `extract_path()` — ドット/ブラケット記法のパス抽出(`src/lib.rs`)

外部パースクレート非依存の自前実装。`serde_json::Value`を扱う任意の
プロジェクトへそのまま移植可能。

```rust
pub fn extract_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }
    let mut current = value;
    for segment in parse_path_segments(path) {
        current = match segment {
            PathSegment::Key(key) => current.as_object()?.get(key)?,
            PathSegment::Index(idx) => current.as_array()?.get(idx)?,
        };
    }
    Some(current)
}
```

## 手書きJSON5風パーサーの拡張ポイント(`src/lib.rs`)

トレイリングコンマ・コメント・クォート無しキー・シングルクォート
文字列の4拡張は、`skip_whitespace_and_comments`・`parse_object`・
`parse_array`・`parse_key`の4箇所に集約されている。新しい拡張
(例: 16進数リテラル)を追加する場合もこの4箇所を中心に変更すれば
良い設計になっている。

## 安全な厳密モード `parse_secure` と実測(2026-10-08)
信頼できない入力向けに、サイズ上限(既定16 MiB)・ネスト深さ上限(既定64)・重複キー拒否を足した`parse_secure`/`from_str_secure`/`Limits`を追加(`src/secure.rs`、テスト7件)。
実測(7MB・4万件): serde_json 182ms、`parse_strict` 181ms、**`parse_secure` 208ms(+14%)**、寛容`parse` 300ms、`sonic-rs`(SIMD) 31ms(約5.8倍速い)。
**結論**: 今の`RS-JSON`に置き換えるだけでは速くならない(中身は`serde_json`)。`parse_secure`は速度ではなく防御のための機能。速度優先ならSIMD版の検討が必要(未実装、値モデルが変わる)。
次の一手: 外部から取り込むJSONの入口(`aruaru-llm`の`knowledge::sync_from_remote`等)へ`parse_secure`を適用する。

## Secure strict mode `parse_secure` and measurements (2026-10-08)
Added `parse_secure` / `from_str_secure` / `Limits` for untrusted input: size limit (16 MiB default), nesting-depth limit (64 default) and duplicate-key rejection (`src/secure.rs`, 7 tests).
Measured (7 MB, 40,000 records): serde_json 182 ms, `parse_strict` 181 ms, **`parse_secure` 208 ms (+14%)**, lenient `parse` 300 ms, `sonic-rs` (SIMD) 31 ms (about 5.8x faster).
**Conclusion**: simply swapping in today's `RS-JSON` does not make anything faster (it delegates to `serde_json`). `parse_secure` is about defence, not speed. If speed matters, a SIMD backend must be considered (not implemented; the value model would change).
Next: apply `parse_secure` at the entrances where outside JSON is imported (e.g. `knowledge::sync_from_remote` in `aruaru-llm`).
