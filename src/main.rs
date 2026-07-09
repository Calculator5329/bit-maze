//! bit-maze CLI.
//!
//! ```text
//! bitmaze play  <level.bm>          run the game            (Phase 3)
//! bitmaze dump  <level.bm>          ASCII-art every plane   (the debugger)
//! bitmaze check <level.bm>          validate all invariants
//! bitmaze asm   <in.asm> <out.bin>  assemble a script       (Phase 5)
//! bitmaze new   <w> <h> <out.bm>    generate a sample level
//! ```

use std::process::ExitCode;

use bitmaze::{check, dump, format::Level, newlevel};

const USAGE: &str = "\
bit-maze — a game whose world is binary

USAGE:
    bitmaze <command> [args]

COMMANDS:
    play  <level.bm>            run the game (not yet implemented)
    dump  <level.bm>            ASCII-art every plane + trigger map
    check <level.bm>            validate the file, exit nonzero if invalid
    asm   <in.asm> <out.bin>    assemble a script (not yet implemented)
    new   <w> <h> <out.bm>      generate a sample walls-only level
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        eprint!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let rest = &args[1..];

    let result = match cmd.as_str() {
        "play" => cmd_todo("play"),
        "dump" => cmd_dump(rest),
        "check" => return cmd_check(rest),
        "asm" => cmd_todo("asm"),
        "new" => cmd_new(rest),
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        other => Err(format!("unknown command '{other}'\n\n{USAGE}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_todo(name: &str) -> Result<(), String> {
    println!("`{name}` not yet implemented");
    Ok(())
}

fn load(path: &str) -> Result<Level, String> {
    let data = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    Level::from_bytes(&data).map_err(|e| format!("{path}: {e}"))
}

fn cmd_dump(args: &[String]) -> Result<(), String> {
    let [path] = args else {
        return Err("usage: bitmaze dump <level.bm>".to_string());
    };
    let level = load(path)?;
    print!("{}", dump::render(&level));
    Ok(())
}

/// `check` owns its exit code so a validation failure exits nonzero with a
/// clear, specific message.
fn cmd_check(args: &[String]) -> ExitCode {
    let [path] = args else {
        eprintln!("usage: bitmaze check <level.bm>");
        return ExitCode::FAILURE;
    };
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("FAIL: cannot read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    match Level::from_bytes(&data) {
        Ok(level) => {
            println!("{}", check::summary(&level));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("FAIL: {path}: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_new(args: &[String]) -> Result<(), String> {
    let [w, h, out] = args else {
        return Err("usage: bitmaze new <w> <h> <out.bm>".to_string());
    };
    let width: u8 = w.parse().map_err(|_| format!("width '{w}' must be 1..=255"))?;
    let height: u8 = h.parse().map_err(|_| format!("height '{h}' must be 1..=255"))?;
    if width == 0 || height == 0 {
        return Err("width and height must be 1..=255".to_string());
    }
    let level = newlevel::generate(width, height);
    let bytes = level.to_bytes();
    std::fs::write(out, &bytes).map_err(|e| format!("cannot write {out}: {e}"))?;
    println!("wrote {out}: {width}x{height} walls-only level ({} bytes)", bytes.len());
    Ok(())
}
