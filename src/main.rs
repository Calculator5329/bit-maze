//! bit-maze CLI.
//!
//! ```text
//! bitmaze play  <level.bm>          play in a window        (Phase 3)
//! bitmaze dump  <level.bm>          ASCII-art every plane   (the debugger)
//! bitmaze check <level.bm>          validate all invariants
//! bitmaze asm   <in.asm> <out.bin>  assemble a script       (Phase 5)
//! bitmaze new   <w> <h> <out.bm>    generate a sample level
//! ```

use std::process::ExitCode;

use bitmaze::{asm, check, dump, format::Level, newlevel, play, window, world::World};

const USAGE: &str = "\
bit-maze — a game whose world is binary

USAGE:
    bitmaze <command> [args]

COMMANDS:
    play  [--term] <level.bm>   play in a window (w/a/s/d or arrows, esc/q quit);
                               --term uses the headless terminal loop instead
    dump  <level.bm>            ASCII-art every plane + trigger map
    check <level.bm>            validate the file, exit nonzero if invalid
    asm   <in.asm> <out.bin>    assemble a script to raw bytecode
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
        "play" => cmd_play(rest),
        "dump" => cmd_dump(rest),
        "check" => return cmd_check(rest),
        "asm" => cmd_asm(rest),
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

/// `asm` — assemble a `.asm` script into raw BitVM bytecode (Phase 5). Writes
/// the raw script bytes (not a `.bm` level); a later tool embeds them.
fn cmd_asm(args: &[String]) -> Result<(), String> {
    let [input, output] = args else {
        return Err("usage: bitmaze asm <in.asm> <out.bin>".to_string());
    };
    let src = std::fs::read_to_string(input).map_err(|e| format!("cannot read {input}: {e}"))?;
    let bytes = asm::assemble(&src).map_err(|e| format!("{input}:{e}"))?;
    std::fs::write(output, &bytes).map_err(|e| format!("cannot write {output}: {e}"))?;
    println!("assembled {input} -> {output} ({} bytes)", bytes.len());
    Ok(())
}

/// `play` — run the interactive game over the shared Phase 2 [`World`] core.
/// Defaults to the Phase 3 minifb window; `--term` selects the headless terminal
/// loop (the same `World`/`step`, a different I/O shell). Both reuse `world`
/// unchanged.
fn cmd_play(args: &[String]) -> Result<(), String> {
    let usage = "usage: bitmaze play [--term] <level.bm>";
    let mut term = false;
    let mut path: Option<&String> = None;
    for a in args {
        match a.as_str() {
            "--term" => term = true,
            _ if a.starts_with('-') => return Err(format!("unknown play option '{a}'\n{usage}")),
            _ if path.is_none() => path = Some(a),
            _ => return Err(usage.to_string()),
        }
    }
    let Some(path) = path else {
        return Err(usage.to_string());
    };

    let level = load(path)?;
    let mut world = World::new(level).map_err(|e| format!("{path}: {e}"))?;

    if term {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        play::run(&mut world, stdin.lock(), stdout.lock())
            .map_err(|e| format!("play loop I/O error: {e}"))
    } else {
        window::run(&mut world, window::TILE_PX)
    }
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
