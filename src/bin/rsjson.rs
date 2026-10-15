//! `rsjson` — JSON と RS-JSON(コメント・末尾カンマ・引用符無しキー・単引用符が書ける上位集合)の
//! 変換・検証・整形をするコマンド。`cargo install --path . --features fast` で入る。
use std::io::{Read, Write};

const HELP: &str = "rsjson — JSON <-> RS-JSON 変換ツール

使い方:
  rsjson to-json  [入力] [-o 出力] [--pretty]   RS-JSON -> 厳密JSON
  rsjson to-rs    [入力] [-o 出力] [--pretty]   JSON -> RS-JSON(識別子のキーは引用符無し)
  rsjson check    [入力]                         構文を検証(RS-JSONも可)。正しければ終了コード0
  rsjson fmt      [入力] [-o 出力]               RS-JSON/JSONを整形した厳密JSONにする

入力を省略するか - なら標準入力、-o を省略すれば標準出力。";

fn read_input(path: Option<&str>) -> Result<String, String> {
    let mut s = String::new();
    match path {
        None | Some("-") => std::io::stdin().read_to_string(&mut s).map_err(|e| e.to_string())?,
        Some(p) => {
            s = std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?;
            s.len()
        }
    };
    Ok(s.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(s))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().map(String::as_str) else {
        println!("{HELP}");
        return Ok(());
    };
    if matches!(cmd, "-h" | "--help" | "help") {
        println!("{HELP}");
        return Ok(());
    }
    let (mut input, mut output, mut pretty) = (None, None, false);
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" => output = Some(it.next().ok_or("-o には出力先が要ります")?.clone()),
            "--pretty" => pretty = true,
            _ if input.is_none() => input = Some(a.clone()),
            _ => return Err(format!("余分な引数: {a}")),
        }
    }
    let text = read_input(input.as_deref())?;
    let result = match cmd {
        "to-json" => {
            let j = rust_json::fast::rsjson_to_json(&text).map_err(|e| e.to_string())?;
            if pretty { pretty_json(&j)? } else { j }
        }
        "to-rs" => rust_json::fast::json_to_rsjson(&rust_json::fast::rsjson_to_json(&text).map_err(|e| e.to_string())?, pretty).map_err(|e| e.to_string())?,
        "fmt" => pretty_json(&rust_json::fast::rsjson_to_json(&text).map_err(|e| e.to_string())?)?,
        "check" => {
            rust_json::fast::rsjson_to_json(&text).map_err(|e| e.to_string())?;
            eprintln!("OK");
            return Ok(());
        }
        other => return Err(format!("不明なコマンド: {other}(rsjson --help を参照)")),
    };
    match output {
        Some(p) => std::fs::write(&p, result).map_err(|e| format!("{p}: {e}")),
        None => {
            let mut o = std::io::stdout().lock();
            o.write_all(result.as_bytes()).and_then(|_| o.write_all(b"\n")).map_err(|e| e.to_string())
        }
    }
}

fn pretty_json(j: &str) -> Result<String, String> {
    let v: rust_json::fast::sonic_rs::Value = rust_json::fast::sonic_rs::from_str(j).map_err(|e| e.to_string())?;
    rust_json::fast::sonic_rs::to_string_pretty(&v).map_err(|e| e.to_string())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("エラー: {e}");
        std::process::exit(1);
    }
}
