//! **Rust-JSON** — a lenient, human-authorable JSON superset with an
//! optional strict-mode escape hatch (concept: user; grammar
//! design and this implementation: Claude, 2026-07-14 as "RJSON"/"RustJSON",
//! renamed and split into its own repository 2026-07-17).
//!
//! **Value model**: the `full`-feature half of this crate (see [`full`],
//! re-exported at the crate root) does not introduce a new data model —
//! the parsed result is a plain `serde_json::Value`, the same type every
//! other JSON-consuming piece of Rust code already uses. RustJSON's entire
//! contribution there is a *more lenient input grammar*: it accepts several
//! common human-authoring conveniences that strict JSON (RFC 8259)
//! rejects, then normalizes down to the exact same value tree strict JSON
//! would produce.
//!
//! ## Grammar extensions over strict JSON (`full::parse`)
//!
//! 1. **Trailing commas** — `[1, 2, 3,]` and `{"a": 1,}` are accepted.
//! 2. **Comments** — `//` line comments and `/* ... */` block comments,
//!    anywhere whitespace is allowed.
//! 3. **Unquoted object keys** — `{name: "sword"}` is accepted wherever
//!    the key is a valid identifier (`[A-Za-z_][A-Za-z0-9_]*`); keys that
//!    aren't valid identifiers (spaces, leading digits, etc.) still
//!    require quotes.
//! 4. **Single-quoted strings** — `'hello'` is accepted anywhere a
//!    double-quoted string is, with the same escape sequences.
//!
//! ## Two independent halves (2026-07-21)
//!
//! This crate ships two independent value models behind a Cargo feature:
//!
//! - **[`light`]** (always compiled, zero dependencies): a browser
//!   `JSON.parse` (`js_sys::JSON`) equivalent with its own [`LightValue`]
//!   model. No `serde_json`/`serde`/`thiserror` in the dependency graph at
//!   all — added so size-constrained targets (WASM in particular; see
//!   [RGit](https://github.com/aon-co-jp/RGit)'s `web/` crate) can depend
//!   on this crate with `default-features = false` and pull in nothing but
//!   this module.
//! - **`full`** (default-on Cargo feature, module private, re-exported at
//!   the crate root): the original hand-rolled lenient/strict parser pair
//!   ([`parse`]/[`parse_strict`]) plus [`extract_path`], using
//!   `serde_json::Value` as the value model. This is what every non-WASM
//!   consumer of this crate (`aruaru-db`, `open-raid-z`, ...) should keep
//!   using unchanged — `default-features = true` is the default, so
//!   nothing about the existing public API changed for them.
//!
//! ## The "hybrid" design decision (2026-07-17, `full` half)
//!
//! This crate was renamed from RJSON/RustJSON to **Rust-JSON** and split
//! into its own repository with an explicit mandate to survey the existing
//! Rust JSON ecosystem (`serde_json`, `simd-json`, `sonic-rs`, `json5`,
//! `json_five`, `json-commons`, ...) and combine the good parts rather than
//! reinvent from a blank slate. That survey (2026-07-17, EN/JP Google +
//! GitHub/crates.io search) found:
//!
//! - **`serde_json`** is the ecosystem's stable, well-understood baseline
//!   (`Value` model, zero extra deps) but strict-only — no comments,
//!   trailing commas, or unquoted keys.
//! - **`simd-json`/`sonic-rs`** are 1.5–4x faster via SIMD, but that speed
//!   comes from `unsafe` code and (for `simd-json`) an x86-only fast path —
//!   a real tradeoff against this ecosystem's "no `unsafe`"
//!   (`#![deny(unsafe_code)]`-style) convention, not a free lunch.
//! - **`json5`/`json_five`** already solve the leniency half well, but are
//!   separate crates from the strict baseline, so a project wanting "mostly
//!   strict JSON, leniency only where explicitly opted into" has to wire
//!   two different parsers together itself.
//! - **`json-commons`** already pairs JSONC support with dot-notation path
//!   extraction — validating that this crate's own `extract_path` design
//!   (independently arrived at in the original RJSON proposal) is a
//!   reasonable, converged-upon shape for this problem, not an outlier.
//!
//! **The hybrid**: keep the hand-rolled, `unsafe`-free, single-crate design
//! (no separate strict/lenient parser to wire together, no SIMD/`unsafe`
//! dependency), but expose *both* ends of the strict/lenient spectrum from
//! one crate: [`parse`] (lenient, the JSON5/JSONC-style superset) and
//! [`parse_strict`] (RFC 8259 only, delegates straight to `serde_json`) —
//! plus [`extract_path`] for the server-side partial-extraction use case
//! `json-commons` also identified as valuable. Raw SIMD-level parsing
//! throughput was **deliberately not pursued**: the honest tradeoff is
//! that `sonic-rs`/`simd-json` would be faster, at the cost of `unsafe`
//! code and (for the SIMD fast paths) less portability — a call this
//! workspace has made consistently elsewhere (hand-rolled WebSocket
//! framing, multipart parsing, gRPC Protocol Buffers codec) and repeats
//! here rather than special-casing JSON.
//!
//! ## Design lineage
//!
//! The extension set (trailing commas, comments, unquoted keys) mirrors
//! [JSON5](https://json5.org/) and [JSONC](https://code.visualstudio.com/docs/languages/json#_json-with-comments),
//! two established, widely-implemented conventions — Rust-JSON does not
//! invent new *kinds* of leniency, it combines an established, minimal
//! subset of them into a single hand-rolled Rust parser with no external
//! parsing-crate dependency (`serde_json` is used only for the output
//! *value model* and for [`parse_strict`], not for parsing this crate's
//! own lenient grammar).

/// ブラウザ組み込みの`JSON.parse`(`js_sys::JSON`)相当を、`serde_json`にも
/// 一切依存せず提供するモジュール(WASM等バイナリサイズ制約のある用途向け、
/// 2026-07-21追加)。`full` featureの有無に関わらず常にビルド対象になる。
pub mod light;
pub use light::{parse_light, LightError, LightValue};

#[cfg(feature = "full")]
pub mod full;
#[cfg(feature = "full")]
pub use full::*;
