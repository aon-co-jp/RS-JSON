# 開発方針・開発環境ルール(Rust-JSON)

## このリポジトリの役割(2026-07-17、RJSON/RustJSONから改称・独立)

`Rust-JSON`は、以前`open-runo`/`poem-cosmo-tauri`内のクレート
(`open-runo-rjson`→`open-runo-rustjson`)として存在していた、
ユーザー発案・Claude実装のJSON拡張仕様を、**独立リポジトリ
として切り出し、改称したもの**。作業ドライブは`F:\open-runo\Rust-JSON`、
VPS上は`/root/Rust-JSON`、GitHubは
[aon-co-jp/Rust-JSON](https://github.com/aon-co-jp/Rust-JSON)。
今後の読み書き・pushはすべてこのリポジトリを正とする。

## 設計方針: 「良いとこ取りのハイブリッド」(2026-07-17、EN/JP調査済み)

ユーザー指示により、既存のRust JSON エコシステムをGoogle・GitHub・
crates.ioで調査した上で、ゼロから作るのではなく良い部分を組み合わせる
方針で設計した。調査結果の要約:

| クレート | 強み | 弱み |
|---|---|---|
| `serde_json` | 事実上の標準、`Value`型で動的操作も可能、追加依存なし | 拡張構文(コメント・トレイリングカンマ等)は非対応 |
| `simd-json`/`sonic-rs` | SIMDで`serde_json`比1.5〜4倍高速 | `unsafe`コード依存、`simd-json`はx86限定の高速パスあり |
| `json5`/`json_five` | JSON5(コメント・トレイリングカンマ・クォート無しキー)に対応 | 厳密JSONとは別クレート、両方使うには自分で組み合わせる必要 |
| `json-commons` | JSONC対応+ドット記法のパス抽出(`get_path()`) | この設計が`extract_path`(本クレートが独自に先行実装していたもの)と同じ発想であることを確認できた=収斂進化の裏付け |
| `valq` | 深い階層のJSONから値を抽出するマクロ | マクロベース、本クレートのような汎用ランタイムAPIではない |

**採用したハイブリッド設計**:
- **`unsafe`不使用・外部パースクレート非依存**を維持(`#![deny(unsafe_code)]`
  相当、`[lints.rust] unsafe_code = "deny"`をCargo.tomlに明記)。
  SIMD最適化(`simd-json`/`sonic-rs`相当の速度)は意図的に追わない——
  高速化には`unsafe`コードとのトレードオフが伴うことを調査で確認済みで
  あり、このエコシステムが他の場所(WebSocketフレーミング・multipart
  パース・gRPC Protocol Buffersコーデック)で一貫して採用している
  「プロトコル/データ形状の層は手書き・`unsafe`不使用」という方針を
  JSONだけ特別扱いしない。
- **`parse()`(寛容モード)と`parse_strict()`(RFC 8259厳密モード)の
  両方を1つのクレートから提供**——`json5`/`serde_json`のように別々の
  クレートを自分で組み合わせる必要が無い、という点をハイブリッドの核に
  した。`parse_strict`は`serde_json::from_str`にそのまま委譲する
  (厳密JSONの定義そのものが`serde_json`の受理範囲と一致するため、
  別の厳密パーサーを自作する必要が無い)。
- **`extract_path`(ドット/ブラケット記法のパス抽出)を維持**——
  `json-commons`の`get_path()`と同じ発想が既に独立して実装されていた
  ことを調査で確認、この設計の妥当性の裏付けとした。

## ビルド・テスト

```bash
cargo test
```

外部パーシングクレートへの依存が無いため、追加のセットアップは不要。

## 関連プロジェクト・移行元

- 旧居場所: `open-runo`(`crates/open-runo-rustjson`)・
  `poem-cosmo-tauri`(同名クレート)——**今後はこのリポジトリへ一本化**。
  旧クレートは撤去せず当面残置(open-runo-db/aruaru-db等が参照している
  ため)、実際にこのクレートへの依存に切り替わった後で撤去を検討する。
- **`open-raid-z`/`aruaru-db`**: ZFS互換・ACID互換のデータ層でこれまで
  素の`serde_json`を直接使っていた箇所を、今後`Rust-JSON`
  (`rust_json::parse`/`parse_strict`/`extract_path`)へ置き換える
  (ユーザー指示、2026-07-17)。進捗は`aruaru-db`側CLAUDE.mdの
  HANDOFFを参照。
- **`open-runo`/`poem-cosmo-tauri`**: `open-runo-db`の`rustjson`
  feature経由でDB層バリデーションに使用中(移行元の実装、参照実装)。

## 運用ルール

- 開発中はこの`CLAUDE.md`を、コード変更のコミット/pushと必ず一緒にpush
  する。
- 確認不要でそのまま実装を進める(このエコシステム共通の運用ルール)。
- README・PORTING.mdは日本語版のみ当面維持し、必要に応じて多言語版を
  追加する(旧クレート側で既に10ヶ国語展開していた実績があるため、
  要望があれば同様に展開可能)。

## 現状

- `src/lib.rs`: `parse`(寛容)・`parse_strict`(厳密)・`to_string`・
  `extract_path`。テスト29件(旧25件+`parse_strict`関連4件)。
- 依存: `serde_json`・`thiserror`のみ。`unsafe_code = "deny"`。

## HANDOFF

- **2026-07-17 リポジトリ新設・ハイブリッド設計・`parse_strict`追加**:
  旧`open-runo-rustjson`のコードをベースに、独立リポジトリとして
  新設。EN/JP調査(`simd-json`/`sonic-rs`/`json5`/`json_five`/
  `json-commons`/`valq`)を踏まえ、`parse_strict`(RFC 8259厳密モード、
  `serde_json::from_str`に委譲)を追加——「寛容+厳密の両方を1クレートで
  提供する」という差別化ポイントを実装。テスト・ビルド確認は次回HANDOFF
  エントリで更新。
  次にすべきこと: (1) `aruaru-db`/`open-raid-z`側のJSON使用箇所を
  `Rust-JSON`に置き換える移行作業、(2) GitHubへの初回push、(3) VPS
  `/root/Rust-JSON`へのクローン。

- **2026-07-21 `light`モジュール新設(ブラウザ`JSON.parse`相当、依存ゼロ)
  — `full` featureで既存APIを分離**: ユーザー指示「JSONにブラウザの
  JSON.parse(js_sys::JSON)のRust版も含めて」を受け、
  [RGit](https://github.com/aon-co-jp/RGit)のWASMフロントエンド
  (`web/`crate)が自前で持っていた最小JSONパーサをこちらへ統合。
  1. **`src/light.rs`新設**: `serde_json`に一切依存しない独立の値モデル
     `LightValue`+`parse_light()`。RFC 8259厳密モードのみ受理
     (このクレートの寛容拡張は非対応——ブラウザ`JSON.parse`自体が
     それらを受理しないことに合わせた意図的な仕様)。`extract_path`相当も
     `LightValue::extract_path`として独自実装(依存ゼロという制約上、
     既存の`extract_path`とはコードを共有できないため)。
  2. **既存コードを`src/full.rs`へ切り出し、`full` feature(既定ON)配下に
     分離**: `serde_json`/`serde`/`thiserror`を`optional = true`にし、
     `default-features = false`で無効化すれば依存グラフから完全に
     排除できるようにした。既存の`aruaru-db`/`open-raid-z`等の利用側は
     `default-features`が既定`true`のままなので**変更不要**(公開API・
     挙動とも無変更)。
  3. **検証**: `cargo test`(default、41件)・`cargo test
     --no-default-features`(light単体、6件)の両方で実行、後者は
     `serde_json`が依存グラフに一切現れないことも確認済み。
  4. **RGit側の統合結果**: `web/Cargo.toml`で`rust-json = { path =
     "../../RJSON", default-features = false }`として依存、
     `wasm32-unknown-unknown --release`ビルド成功、`wasm-bindgen`で
     生成した`.wasm`は234KB(`serde_json`を含まないため)。詳細は
     RGit側`CLAUDE.md`参照。
  - 次にすべきこと: 上記(1)(`aruaru-db`/`open-raid-z`の移行)は引き続き
    未着手。
---

## エコシステム全体マップ(2026-07-21追記)

同時並行開発の対象プロジェクト一覧・各リポジトリの現況は
[`open-raid-z`のCLAUDE.md](https://github.com/aon-co-jp/open-raid-z/blob/main/CLAUDE.md)
「関連プロジェクト」節を参照。**どのリポジトリから読み始めても、
この節を起点に他プロジェクトへ辿れる**ようにしてある(このリポジトリ
自身の状況はこの上のHANDOFF節を参照)。

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
