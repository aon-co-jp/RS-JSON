use std::time::Instant;
fn make_doc(n: usize) -> String {
    let mut s = String::from("[");
    for i in 0..n {
        if i > 0 { s.push(','); }
        s.push_str(&format!(r#"{{"id":{i},"name":"user-{i}","email":"user{i}@example.com","active":{},"score":{}.5,"tags":["a","b","日本語タグ"],"profile":{{"age":{},"city":"Tokyo","bio":"line1\nline2 \"quoted\""}}}}"#, i % 2 == 0, i % 1000, 20 + i % 50));
    }
    s.push(']'); s
}
fn depth_exceeds(bytes: &[u8], limit: usize) -> Option<usize> {
    let (mut depth, mut i) = (0usize, 0usize); let mut in_str = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str { match b { b'\\' => i += 1, b'"' => in_str = false, _ => {} } }
        else { match b { b'"' => in_str = true, b'[' | b'{' => { depth += 1; if depth > limit { return Some(depth); } } b']' | b'}' => depth = depth.saturating_sub(1), _ => {} } }
        i += 1;
    }
    None
}
fn best<F: FnMut() -> usize>(label: &str, mut f: F) -> f64 {
    for _ in 0..2 { f(); }
    let mut b = f64::MAX;
    for _ in 0..9 { let t = Instant::now(); std::hint::black_box(f()); b = b.min(t.elapsed().as_secs_f64()); }
    println!("{label:<28} {:>8.2} ms", b * 1000.0); b
}
fn main() {
    let doc = make_doc(40_000);
    println!("{} MB", doc.len() / 1_000_000);
    let lim = rust_json::Limits::default();
    best("depth_exceeds (byte loop)", || depth_exceeds(doc.as_bytes(), 64).is_some() as usize);
    best("sonic_rs::from_str", || { let v: sonic_rs::Value = sonic_rs::from_str(&doc).unwrap(); sonic_rs::JsonContainerTrait::as_array(&v).unwrap().len() });
    best("parse_secure_sonic (total)", || sonic_rs::JsonContainerTrait::as_array(&rust_json::parse_secure_sonic(&doc, &lim).unwrap()).unwrap().len());
    let small = r#"{"table":"t","key":"k","value":{"a":1,"b":[1,2,3]}}"#;
    let n = 200_000;
    let t = Instant::now(); for _ in 0..n { std::hint::black_box(rust_json::parse_secure_sonic(small, &lim).unwrap()); } println!("small body parse_secure_sonic: {:.0} ns/call", t.elapsed().as_nanos() as f64 / n as f64);
    let t = Instant::now(); for _ in 0..n { std::hint::black_box(serde_json::from_str::<serde_json::Value>(small).unwrap()); } println!("small body serde_json:          {:.0} ns/call", t.elapsed().as_nanos() as f64 / n as f64);
}
