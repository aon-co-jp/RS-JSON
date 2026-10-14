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
`serde_json::Value`と異なる。**2026-10-14に`fast`機能の`parse_secure_sonic`として実装した(下記)。**

## 速い値の型で読む安全モード `parse_secure_sonic`(任意機能`fast`、2026-10-14追加)

`sonic-rs`(ByteDance、SIMD)の**値の型`sonic_rs::Value`**で読み、サイズ・深さの上限と重複キー拒否を付ける。
`Cargo.toml`: `rust-json = { path = "../RS-JSON", features = ["fast"] }`

```rust
use rust_json::{parse_secure_sonic, Limits};
use sonic_rs::{JsonContainerTrait, JsonValueTrait};
let v = parse_secure_sonic(body, &Limits::small())?;   // 結果は sonic_rs::Value
let name = v.get("name").as_str();
```

### 実測(`cargo run --release --features fast --example bench`、7 MB・4万件、最速7回)

| 方式 | 時間 | serde_json比 |
|---|---|---|
| `serde_json::from_str<Value>` | 200 ms | 1.00 |
| `parse_secure`(安全) | 217 ms | +8% |
| **`parse_secure_sonic`(速い型+安全)** | **83 ms** | **-58%(約2.4倍速い)** |
| `sonic-rs`そのまま(防御なし、参考) | 33 ms | -84%(約6倍) |
| `sonic-rs`→`serde_json::Value`に読む | 188 ms | -6%(**ほぼ速くならない**) |
| `parse_secure_fast`(SIMD+`serde_json::Value`) | 217 ms | +8%(**速くならない**) |

**重要な発見**: 速さの源は「SIMDで文字を走査すること」より**「結果を連続した領域に詰め込む値の型」**にある。結果を`serde_json::Value`に組み立てる限り、
SIMDにしてもほぼ速くならない。**速くするには、結果を`sonic_rs::Value`のまま使う必要がある**(使う側のコードの書き換えが要る)。
安全モードの防御込みで、残りの差(33→83 ms)は重複キーの検査分。
`parse_secure_fast`(`serde_json::Value`を返す版)は互換性のために残すが、**速度目的では使わない**。

**正直な開示**: 依存先の`sonic-rs`は内部で`unsafe`を使う(このクレート自身のコードに`unsafe`は無い)。既定ではビルドに入らず、`fast`機能を選んだときだけ入る。

## 全リポジトリでの使用方針(2026-10-14、ユーザー指示)
JSONを読む所は、**使える所は全て**`parse_secure_sonic`(速い型+安全)を使う。使えない所は`parse_secure`(通常の安全モード)。適用の条件・注意は
[`PORTING.md`](PORTING.md)の「JSON処理の使用方針」を参照。

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

## JSON処理の使用方針: `parse_secure_sonic`(速い型+安全)と`sonic-rs`は使える所では全て使う(2026-10-14、ユーザー指示)
正本は[`RS-JSON`](https://github.com/aon-co-jp/RS-JSON)(`README.md`・`PORTING.md`)。

**方針**: JSONを読む所は、使える所では**全て** `rust-json`(RS-JSON)の `parse_secure_sonic`(`fast`機能、結果は`sonic_rs::Value`)を使う。
`sonic-rs` を直接使える所(型付き構造体への読み込みなど)も同様。使えない所は `parse_secure`(結果は`serde_json::Value`)を使う。
外部から来る信頼できない入力(HTTP本文・ネットワーク越しのファイル・アップロード)は、**必ずサイズ・深さ・重複キーの上限つき**で読む。

**実測(7 MB・4万件)**: `serde_json` 200 ms / `parse_secure` 217 ms / **`parse_secure_sonic` 83 ms(約2.4倍速い、防御込み)** / `sonic-rs`そのまま 33 ms(約6倍、防御なし)。
`serde_json::Value`に読む限りSIMDでもほぼ速くならない(188 ms)。**速さの源は、結果を連続した領域に詰め込む値の型**。

**使える所 / 使えない所**
- ✅ 読み取り中心で、JSONを大きく・頻繁に読む所(キャッシュJSON、設定、APIの応答、ログ、インデックス)。
- ✅ 外部からの入力の入口(速さより防御が目的。`parse_secure`でも可)。
- ⚠️ `serde_json::Value`を関数間で受け渡している既存コードは、型を`sonic_rs::Value`に直す書き換えが要る(`JsonValueTrait`/`JsonContainerTrait`のメソッド)。書き換えずに`serde_json::Value`へ読むと速くならない。
- ❌ WASM(`rust_json::light`、依存ゼロ)、PHP/JavaScript側のJSON処理(Rustの部品は使えない)。
- 注意: 依存先の`sonic-rs`は内部で`unsafe`を使う(自前コードに`unsafe`は無い)。小さなJSON(数KB)では体感差が出ない。速度目的の書き換えは、実測で効く所から。

**導入**: `Cargo.toml`に `rust-json = { path = "../RS-JSON", features = ["fast"] }`。使用例は`RS-JSON/README.md`。

**このリポジトリでの適用状況**: 提供元。`parse_secure`(50件のテスト)・`parse_secure_sonic`(`fast`機能)を実装、実測済み。

## JSON handling policy: use `parse_secure_sonic` (fast type + secure) and `sonic-rs` everywhere they can be used (2026-10-14, user instruction)
The source of truth is [`RS-JSON`](https://github.com/aon-co-jp/RS-JSON) (`README.md`, `PORTING.md`).

**Policy**: wherever JSON is read, use `parse_secure_sonic` from `rust-json` (RS-JSON, the `fast` feature; the result is `sonic_rs::Value`) **everywhere it can be used**.
Use `sonic-rs` directly where possible too (e.g. reading into typed structs). Where it cannot be used, use `parse_secure` (result: `serde_json::Value`).
Untrusted input from outside (HTTP bodies, files over the network, uploads) **must always be read with size / depth / duplicate-key limits**.

**Measured (7 MB, 40,000 records)**: `serde_json` 200 ms / `parse_secure` 217 ms / **`parse_secure_sonic` 83 ms (about 2.4x faster, with the defences)** / raw `sonic-rs` 33 ms (about 6x, no defences).
Reading into `serde_json::Value` gives almost no speed-up even with SIMD (188 ms). **The speed comes from the value type that packs the result into contiguous memory.**

**Where it can / cannot be used**
- ✅ Read-heavy code that parses large or frequent JSON (cache JSON, config, API responses, logs, indexes).
- ✅ Entrances for outside input (the goal there is defence rather than speed; `parse_secure` is fine).
- ⚠️ Existing code that passes `serde_json::Value` between functions needs its type changed to `sonic_rs::Value` (methods from `JsonValueTrait` / `JsonContainerTrait`). Reading into `serde_json::Value` without that change is not faster.
- ❌ WASM (`rust_json::light`, zero dependencies) and the JSON handling on the PHP / JavaScript side (Rust components cannot be used there).
- Note: the `sonic-rs` dependency uses `unsafe` internally (our own code has none). For small JSON (a few KB) the difference is not noticeable; do speed-motivated rewrites where measurements show a benefit.

**Adoption**: in `Cargo.toml`, `rust-json = { path = "../RS-JSON", features = ["fast"] }`. Example in `RS-JSON/README.md`.

**Status in this repository**: Provider. Implements `parse_secure` and `parse_secure_sonic` (`fast` feature), with tests and measurements.

## 姉妹リポジトリ: `sonic-rs` と `RS-JSON`(2026-10-15)
[`sonic-rs`](https://github.com/aon-co-jp/sonic-rs)と[`RS-JSON`](https://github.com/aon-co-jp/RS-JSON)は**姉妹リポジトリ**。2つで1組として扱う。

| リポジトリ | 役割 |
|---|---|
| [`RS-JSON`](https://github.com/aon-co-jp/RS-JSON) | **コードの実体**。`parse_secure`(安全)・`parse_secure_sonic`(速い型+安全、`fast`機能)・`light`(WASM用)。テストと実測ベンチ(`examples/bench.rs`)を持つ |
| [`sonic-rs`](https://github.com/aon-co-jp/sonic-rs) | **評価の記録と導入方針**(文書のみ)。実測の読み方、使える所/使えない所、各リポジトリへの適用状況 |

- このリポジトリ(`RS-JSON`)ははコードの実体と実測ベンチを持つ。`sonic-rs`は評価の記録と導入方針を持つ(文書のみ)。
- **片方を更新したら、もう片方も追従する**(実測値・方針・適用状況が食い違わないようにする)。数値の正本は`RS-JSON`の`examples/bench.rs`の実測、方針の正本は`sonic-rs`の`PORTING.md`。
- 本家の`sonic-rs`(CloudWeGo/ByteDance、Apache-2.0)とは別物。`sonic-rs`リポジトリは本家のコードを含まず、提携関係もない。

## Sister repositories: `sonic-rs` and `RS-JSON` (2026-10-15)
[`sonic-rs`](https://github.com/aon-co-jp/sonic-rs) and [`RS-JSON`](https://github.com/aon-co-jp/RS-JSON) are **sister repositories**. Treat the two as a pair.

| Repository | Role |
|---|---|
| [`RS-JSON`](https://github.com/aon-co-jp/RS-JSON) | **The code.** `parse_secure` (secure), `parse_secure_sonic` (fast type + secure, `fast` feature) and `light` (for WASM). Holds the tests and the measurement bench (`examples/bench.rs`) |
| [`sonic-rs`](https://github.com/aon-co-jp/sonic-rs) | **Evaluation notes and adoption policy** (documents only): how to read the measurements, where it can / cannot be used, and each repository's adoption status |

- This repository (`RS-JSON`) holds the code and the measurement bench. `sonic-rs` holds the evaluation notes and the adoption policy (documents only).
- **When you update one, update the other too** so the measured numbers, the policy and the adoption status never disagree. The source of truth for numbers is the measurement in `RS-JSON`'s `examples/bench.rs`; for the policy it is `PORTING.md` in `sonic-rs`.
- It is unrelated to the upstream `sonic-rs` (CloudWeGo/ByteDance, Apache-2.0). The `sonic-rs` repository contains none of its code and has no affiliation with it.
