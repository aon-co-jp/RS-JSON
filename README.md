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

## 高速な読み書きとJSON <-> RS-JSON変換(`fast`機能、`sonic-rs`リポジトリを統合、2026-10-15)
旧`sonic-rs`リポジトリ(評価記録と導入方針の文書のみ)は**このリポジトリに統合した**(内容はこの文書群に取り込み済み)。本家の`sonic-rs`(CloudWeGo/ByteDance、Apache-2.0)とは別物で、依存先として使うだけ。

**`rust_json::fast`**(`fast`機能): `from_str`/`from_slice`/`to_string`/`to_vec`/`to_string_pretty`(型付き構造体、`sonic-rs`)、
`rsjson_to_json`/`json_to_rsjson`(RS-JSON<->JSON、値を作らない1パス走査+検証)、`from_rsjson`/`to_rsjson`(型付き)。

**実測(7 MB・4万件、`cargo run --release --features fast --example bench`、最速7回)**
| 処理 | 標準 | fast | 結果 |
|---|---|---|---|
| `sonic_rs::Value`へ読む(`parse_secure_sonic`、防御込み) | `serde_json::Value` 178 ms | 88 ms | **約2倍速い** |
| 型付き構造体へ読む | 76 ms | 71 ms | **約1.1倍(ほぼ変わらない)** |
| 型付き構造体を書く | 30 ms | 37 ms | **むしろ遅い(0.8倍)** |
| JSON → RS-JSON(`json_to_rsjson`) | 寛容パーサ経由 144 ms | 40 ms | 約3.6倍 |
| RS-JSON → JSON(厳密JSON入力) | 85 ms | 17 ms | 約5倍 |
| RS-JSON → JSON(寛容構文あり) | 344 ms | 48 ms | **約7倍** |

**正直な結論**: 速いのは「`sonic_rs::Value`に読む」場合と「RS-JSON<->JSONの変換」。**型付き構造体の読み書きは、文字列ごとの確保が支配的でほぼ速くならず、書き出しは`serde_json`より遅い。**
そのため型付き読み込みの置き換えは効果が小さい(害も小さい)。`fast::to_string`は速度目的では使わない。

**導入手順**: `Cargo.toml`に `rust-json = { path = "../RS-JSON", features = ["fast"] }`(GitHubのCIが兄弟ディレクトリを持たないリポジトリは、型付き読み込みに限り`sonic-rs = "0.3"`を直接依存にしてよい)。
読む場所を`parse_secure_sonic`に替え、使う側を`sonic_rs::{JsonContainerTrait, JsonValueTrait}`のメソッドに書き換える。公開APIの戻り値が`serde_json::Error`の関数は、型が変わるので置き換えない。

**適用状況(2026-10-15)**: 型付き読み込みを`sonic_rs::from_str/from_slice`へ置き換え済み(ビルド確認済み): `aruaru-IME`(`aruaru-ime-core`、テスト178件合格)・`aruaru-llm`・`aruaru-search`・`realdata.pro`・`open-backup`・`open-web-server`・`open-bar`・`webeditor-sftp`。
`aruaru-llm`の外部取り込みは`parse_secure`。`Value`を受け渡している箇所・`audiocafe-tokyo-rust`の`fetch_cache`は未切り替え(型の書き換えが要る)。
## Fast read/write and JSON <-> RS-JSON conversion (`fast` feature, absorbs the `sonic-rs` repository, 2026-10-15)
The former `sonic-rs` repository (evaluation notes and adoption policy, documents only) has been **merged into this repository**. It was never the upstream `sonic-rs` (CloudWeGo/ByteDance, Apache-2.0); that crate is only a dependency here.

**`rust_json::fast`** (`fast` feature): `from_str`/`from_slice`/`to_string`/`to_vec`/`to_string_pretty` (typed structs, via `sonic-rs`),
`rsjson_to_json`/`json_to_rsjson` (RS-JSON <-> JSON, a single value-less scan plus validation), `from_rsjson`/`to_rsjson` (typed).

**Measured (7 MB, 40,000 records, `cargo run --release --features fast --example bench`, best of 7)**
| Operation | Baseline | fast | Result |
|---|---|---|---|
| Read into `sonic_rs::Value` (`parse_secure_sonic`, with defences) | `serde_json::Value` 178 ms | 88 ms | **about 2x faster** |
| Read into typed structs | 76 ms | 71 ms | **about 1.1x (almost no change)** |
| Write typed structs | 30 ms | 37 ms | **slower (0.8x)** |
| JSON -> RS-JSON (`json_to_rsjson`) | via lenient parser 144 ms | 40 ms | about 3.6x |
| RS-JSON -> JSON (strict input) | 85 ms | 17 ms | about 5x |
| RS-JSON -> JSON (lenient syntax) | 344 ms | 48 ms | **about 7x** |

**Honest conclusion**: the wins are reading into `sonic_rs::Value` and the RS-JSON <-> JSON conversions. **Typed reads/writes are dominated by per-string allocation and barely speed up; writing is slower than `serde_json`.**
So replacing typed reads yields a small gain (and small risk). Do not use `fast::to_string` for speed.

**Adoption**: `rust-json = { path = "../RS-JSON", features = ["fast"] }` (repositories whose GitHub CI has no sibling directory may depend on `sonic-rs = "0.3"` directly, for typed reads only).
Switch reads to `parse_secure_sonic` and rewrite the consumers with `sonic_rs::{JsonContainerTrait, JsonValueTrait}`. Do not replace functions whose public return type is `serde_json::Error` (the type would change).

**Status (2026-10-15)**: typed reads switched to `sonic_rs::from_str/from_slice` (build-checked): `aruaru-IME` (`aruaru-ime-core`, 178 tests pass), `aruaru-llm`, `aruaru-search`, `realdata.pro`, `open-backup`, `open-web-server`, `open-bar`, `webeditor-sftp`.
`aruaru-llm`'s external imports use `parse_secure`. Code passing `Value` around and `fetch_cache` in `audiocafe-tokyo-rust` are not switched yet (type rewrite needed).

### 使い方の早見(`fast`機能、2026-10-16追加)
```rust
use rust_json::fast::{read_file, write_file, convert_file, rsjson_to_json, json_to_rsjson};
let cfg: Config = read_file("config.rsjson")?;        // コメント・末尾カンマ入りでも読める(.rsjson/.json5/.jsonc)
write_file("out.json", &cfg)?;                         // 一時ファイル経由で安全に書く(.rsjsonならRS-JSON形式)
convert_file("a.rsjson", "a.json", true)?;             // ファイル変換(向きは拡張子で決まる)
let json = rsjson_to_json("{ name: 'x', /* c */ }")?;  // 文字列の変換
```
コマンド: `cargo install --path . --features fast` で `rsjson` が入る。`rsjson to-json a.rsjson --pretty` / `rsjson to-rs a.json -o a.rsjson` / `rsjson check a.rsjson` / `rsjson fmt a.json`。

### Quick reference (`fast` feature, added 2026-10-16)
`read_file` / `write_file` (atomic via a temp file) / `convert_file` (direction chosen by extension: `.rsjson`/`.json5`/`.jsonc` are lenient) / `rsjson_to_json` / `json_to_rsjson`.
CLI: `cargo install --path . --features fast` installs `rsjson` (`to-json`, `to-rs`, `check`, `fmt`; stdin/stdout when no file is given).

---

作者 / Author: 石塚正浩（MASAHIRO ISHIZUKA） (https://github.com/aon-co-jp)
