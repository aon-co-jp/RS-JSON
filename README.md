# Rust-JSON

**開発開始日: 2026-07-18**(このリポジトリのGitHub作成日)

Rustで書かれた、寛容(lenient)モードと厳密(strict)モードの両方を1つの
クレートから提供するJSON処理ライブラリ。パース結果は常に標準の
`serde_json::Value`——新しいデータモデルは導入しない。

## これは何か

- **寛容モード(`parse`)**: JSON5/JSONCスタイルの拡張構文を受理する
  JSONスーパーセット——トレイリングコンマ・コメント(`//`・`/* */`)・
  クォート無しキー・シングルクォート文字列。
- **厳密モード(`parse_strict`)**: RFC 8259準拠のJSONのみを受理
  (内部的に`serde_json::from_str`へ委譲)。
- **`extract_path`**: ドット/ブラケット記法(`stats.damage`・
  `bonuses[1]`・`items[2].name`)によるサーバー側の部分抽出。
- **`unsafe`コード不使用・外部パースクレート非依存**
  (`serde_json`は値モデル/厳密モードの委譲先としてのみ使用)。

## なぜ「ハイブリッド」なのか

既存のRust JSONエコシステム(`serde_json`・`simd-json`・`sonic-rs`・
`json5`・`json_five`・`json-commons`・`valq`等)をGoogle・GitHub・
crates.ioで調査した上で、良い部分を組み合わせて設計した(詳細は
`CLAUDE.md`参照)。要点:

- SIMD最適化(`simd-json`/`sonic-rs`)による高速化は、`unsafe`コードとの
  トレードオフを伴うため意図的に採用しない。
- 寛容パース(`json5`系)と厳密パース(`serde_json`)を、別々のクレートを
  自分で組み合わせるのではなく、1つのクレートの2つの関数として提供する。
- パス抽出(`extract_path`)は`json-commons`の`get_path()`と同じ発想——
  独立に収斂した設計であることを調査で確認済み。

## 使用例

```rust
use rust_json::{parse, parse_strict, extract_path, to_string};

// 寛容モード: コメント・トレイリングコンマ・クォート無しキーを受理
let value = parse(r#"{
    // 装備アイテム
    name: 'longsword',
    stats: { damage: 12, bonuses: [1, 2, 3,] },
}"#).unwrap();

// 厳密モード: 上と同じ入力は拒否される(RFC 8259のみ受理)
assert!(parse_strict(r#"{name: 'longsword'}"#).is_err());

// 部分抽出
let damage = extract_path(&value, "stats.damage");

// 常に厳密JSONとして出力
let canonical = to_string(&value);
```

## 安全な厳密モード(`parse_secure`、2026-10-08追加)

外部から来る信頼できないJSON(HTTPの本文、ネットワーク越しのキャッシュ等)を読む入口向け。
標準の`parse_strict`に、**サイズ上限**(既定16 MiB)・**ネスト深さ上限**(既定64)・
**重複キー拒否**を足す。`unsafe`なし、値モデルは`serde_json::Value`のまま。

```rust
use rust_json::{parse_secure, Limits};
let v = parse_secure(body, &Limits::small())?; // 1 MiB・深さ32・重複キー拒否
```

### 実測(`cargo run --release --example bench`、7 MB・4万件、最速7回)

| 方式 | 時間 | serde_json比 |
|---|---|---|
| `serde_json::from_str<Value>` | 182 ms | 1.00 |
| `parse_strict` | 181 ms | 0.99 |
| **`parse_secure`(安全)** | 208 ms | **+14%** |
| `parse`(寛容) | 300 ms | +65% |
| `sonic-rs`(SIMD、参考) | 31 ms | **-83%(約5.8倍速い)** |

**正直な開示**: `parse_secure`は速度ではなく**防御**のための機能で、標準より約14%遅い。
速度が最優先なら`sonic-rs`(SIMD、依存先が`unsafe`を使う)が約5.8倍速いが、値モデルが
`serde_json::Value`と異なる。安全モードと同じ上限をSIMD版に付ける案は未実装。

## ビルド・テスト

```bash
cargo test
```

## 関連プロジェクト

- [open-runo](https://github.com/aon-co-jp/open-runo)・
  [poem-cosmo-tauri](https://github.com/aon-co-jp/poem-cosmo-tauri) —
  旧居場所(`open-runo-rjson`→`open-runo-rustjson`)。当面は参照実装として
  残置。
- [open-raid-z](https://github.com/aon-co-jp/open-raid-z)・
  [aruaru-db](https://github.com/aon-co-jp/aruaru-db) — ZFS互換・
  ACID互換のデータ層でのJSON処理を、今後本クレートへ置き換える移行先。

## ライセンス

Apache-2.0 OR MIT
