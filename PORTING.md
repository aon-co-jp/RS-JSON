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
