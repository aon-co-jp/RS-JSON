//! 実測: serde_json / RS-JSON(strict・secure・lenient) / sonic-rs(SIMD) のパース速度。
//! 実行: `cargo run --release --example bench`
//! 結果は環境依存。数値は`PORTING.md`の実測表に転記する。
use std::time::Instant;

fn make_doc(n: usize) -> String {
    let mut s = String::from("[");
    for i in 0..n {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            r#"{{"id":{i},"name":"user-{i}","email":"user{i}@example.com","active":{},"score":{}.5,"tags":["a","b","日本語タグ"],"profile":{{"age":{},"city":"Tokyo","bio":"line1\nline2 \"quoted\""}}}}"#,
            i % 2 == 0,
            i % 1000,
            20 + i % 50
        ));
    }
    s.push(']');
    s
}

fn bench<F: FnMut() -> usize>(label: &str, bytes: usize, mut f: F) -> f64 {
    for _ in 0..2 {
        f(); // ウォームアップ
    }
    let runs = 7;
    let mut best = f64::MAX;
    for _ in 0..runs {
        let t = Instant::now();
        let n = f();
        let dt = t.elapsed().as_secs_f64();
        std::hint::black_box(n);
        best = best.min(dt);
    }
    println!("{label:<34} {:>8.2} ms  {:>7.1} MB/s", best * 1000.0, bytes as f64 / best / 1e6);
    best
}

fn main() {
    let doc = make_doc(40_000);
    let bytes = doc.len();
    println!("入力: {} MB / {} 件のオブジェクト (最速の7回を採用)\n", bytes / 1_000_000, 40_000);

    let base = bench("serde_json::from_str<Value>", bytes, || {
        serde_json::from_str::<serde_json::Value>(&doc).unwrap().as_array().unwrap().len()
    });
    let strict = bench("rust_json::parse_strict", bytes, || rust_json::parse_strict(&doc).unwrap().as_array().unwrap().len());
    let lim = rust_json::Limits::default();
    let secure = bench("rust_json::parse_secure (安全)", bytes, || rust_json::parse_secure(&doc, &lim).unwrap().as_array().unwrap().len());
    let lenient = bench("rust_json::parse (寛容)", bytes, || rust_json::parse(&doc).unwrap().as_array().unwrap().len());
    let mut rows = vec![("parse_strict", strict), ("parse_secure (安全)", secure), ("parse (寛容)", lenient)];

    // SIMD版(`--features fast`)。「sonic-rs自身の値の型」と「serde_json::Valueに直接読む」の両方を測る
    #[cfg(feature = "fast")]
    {
        let sonic_own = bench("sonic_rs::from_str<sonic Value> (SIMD)", bytes, || {
            let v: sonic_rs::Value = sonic_rs::from_str(&doc).unwrap();
            sonic_rs::JsonContainerTrait::as_array(&v).unwrap().len()
        });
        let sonic_std = bench("sonic_rs::from_str<serde_json::Value>", bytes, || {
            sonic_rs::from_str::<serde_json::Value>(&doc).unwrap().as_array().unwrap().len()
        });
        let fast = bench("rust_json::parse_secure_fast (SIMD+安全)", bytes, || {
            rust_json::parse_secure_fast(&doc, &lim).unwrap().as_array().unwrap().len()
        });
        let secure_sonic = bench("rust_json::parse_secure_sonic (速い型+安全)", bytes, || {
            use sonic_rs::JsonContainerTrait;
            rust_json::parse_secure_sonic(&doc, &lim).unwrap().as_array().unwrap().len()
        });
        rows.push(("parse_secure_sonic (速い型+安全)", secure_sonic));
        rows.push(("sonic-rs 自身の型", sonic_own));
        rows.push(("sonic-rs→serde_json::Value", sonic_std));
        rows.push(("parse_secure_fast (SIMD+安全)", fast));
    }

    println!("\nserde_json を 1.00 とした比:");
    for (l, t) in rows {
        println!("  {l:<30} {:.2}x の時間 ({:+.0}%)", t / base, (t / base - 1.0) * 100.0);
    }
}
