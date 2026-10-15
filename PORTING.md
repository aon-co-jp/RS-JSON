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