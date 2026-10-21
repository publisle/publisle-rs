use publisle_pdf::{
    compile,
    model::{Diagnostic, Input},
};
use std::{
    fs,
    io::{Read, Write},
    process::ExitCode,
};

fn run() -> Result<(), Diagnostic> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--version" {
        println!("publisle-pdf {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.len() != 2 {
        return Err(Diagnostic::new(
            "usage",
            "Usage: publisle-pdf INPUT.json NEW-OUTPUT.pdf",
        ));
    }
    let file =
        fs::File::open(&args[0]).map_err(|e| Diagnostic::new("input-read", e.to_string()))?;
    let mut bytes = Vec::new();
    file.take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Diagnostic::new("input-read", e.to_string()))?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(Diagnostic::new(
            "input-limit",
            "Compiler input exceeds 4 MiB.",
        ));
    }
    let input: Input = serde_json::from_slice(&bytes)
        .map_err(|e| Diagnostic::new("invalid-input", e.to_string()))?;
    let (bytes, report) = compile(&input)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])
        .map_err(|e| Diagnostic::new("output-write", e.to_string()))?;
    output
        .write_all(&bytes)
        .map_err(|e| Diagnostic::new("output-write", e.to_string()))?;
    println!("{}", serde_json::to_string(&report).unwrap());
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            println!("{}", serde_json::json!({"ok":false,"diagnostics":[error]}));
            ExitCode::FAILURE
        }
    }
}
