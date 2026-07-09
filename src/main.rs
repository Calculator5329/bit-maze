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

use bitmaze::{
    asm, check, dump, format::Level, framebuffer, newlevel, play, replay, samples, sprite,
    sprite::Sprites, window, world::World,
};

const USAGE: &str = "\
bit-maze — a game whose world is binary

USAGE:
    bitmaze <command> [args]

COMMANDS:
    play  [--term] <level.bm>   play in a window (w/a/s/d or arrows, esc/q quit);
                               --term uses the headless terminal loop instead
    play --term --record <r.rec> <level.bm>   record the input sequence to a .rec
    play --replay <r.rec> <level.bm>          replay a .rec, print the final state
    dump  <level.bm>            ASCII-art every plane + trigger map
    check <level.bm>            validate the file, exit nonzero if invalid
    asm   <in.asm> <out.bin>    assemble a script to raw bytecode
    new   <w> <h> <out.bm>      generate a sample walls-only level
    gen-levels <dir>            write the built-in sample levels (walls + items)
    shot  <level.bm> <out.ppm> [tile_px]      render a level to a P6 PPM image
    sprite <file.spr>           dump a 1-bit sprite as ASCII (# ink / . paper)
    sprite gen <dir>            write the default sprites into <dir>
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
        "gen-levels" => cmd_gen_levels(rest),
        "shot" => cmd_shot(rest),
        "sprite" => cmd_sprite(rest),
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
///
/// Phase 7 replay flags:
/// - `--record <file.rec>` (with `--term`): log the input sequence to a `.rec`.
/// - `--replay <file.rec>`: replay the log deterministically and print the exact
///   final state (position, score, wall mutations) — no window, no input.
fn cmd_play(args: &[String]) -> Result<(), String> {
    let usage = "usage: bitmaze play [--term] [--record <file.rec>] [--replay <file.rec>] <level.bm>";
    let mut term = false;
    let mut record: Option<String> = None;
    let mut replay: Option<String> = None;
    let mut path: Option<&String> = None;

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--term" => term = true,
            "--record" => {
                i += 1;
                record =
                    Some(args.get(i).ok_or_else(|| format!("--record needs a path\n{usage}"))?.clone());
            }
            "--replay" => {
                i += 1;
                replay =
                    Some(args.get(i).ok_or_else(|| format!("--replay needs a path\n{usage}"))?.clone());
            }
            _ if a.starts_with('-') => return Err(format!("unknown play option '{a}'\n{usage}")),
            _ if path.is_none() => path = Some(a),
            _ => return Err(usage.to_string()),
        }
        i += 1;
    }
    let Some(path) = path else {
        return Err(usage.to_string());
    };

    let level = load(path)?;
    let mut world = World::new(level).map_err(|e| format!("{path}: {e}"))?;

    // Replay path: deterministic re-run, then print the final state.
    if let Some(rec_path) = replay {
        return cmd_replay(&mut world, &rec_path, path);
    }

    if record.is_some() && !term {
        return Err(format!("--record requires --term (headless recording)\n{usage}"));
    }

    if term {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let moves = play::run(&mut world, stdin.lock(), stdout.lock())
            .map_err(|e| format!("play loop I/O error: {e}"))?;
        if let Some(rec_path) = record {
            let bytes = replay::Replay::new(path, &moves).to_bytes();
            std::fs::write(&rec_path, &bytes)
                .map_err(|e| format!("cannot write {rec_path}: {e}"))?;
            eprintln!("recorded {} move(s) -> {rec_path} ({} bytes)", moves.len(), bytes.len());
        }
        Ok(())
    } else {
        // Load role sprites from `sprites/` (per-sprite fallback to built-ins),
        // then blit them through the default palette in the window.
        let (sprites, notes) = Sprites::load_from_dir(sprite::SPRITE_DIR);
        for note in &notes {
            eprintln!("sprite: {note}");
        }
        window::run(&mut world, window::TILE_PX, &sprites, &sprite::Palette::DEFAULT)
    }
}

/// Replay a `.rec` file against `world` and print the reproduced final state.
/// Because the world is deterministic, this is the exact state the recorded run
/// ended in.
fn cmd_replay(world: &mut World, rec_path: &str, level_path: &str) -> Result<(), String> {
    let data = std::fs::read(rec_path).map_err(|e| format!("cannot read {rec_path}: {e}"))?;
    let rep = replay::Replay::from_bytes(&data).map_err(|e| format!("{rec_path}: {e}"))?;
    if rep.level_ref != level_path {
        eprintln!(
            "note: replay was recorded against '{}', replaying against '{}'",
            rep.level_ref, level_path
        );
    }
    for &mv in &rep.moves {
        world.step_triggered(mv);
    }
    print!("{}", world.render());
    println!("final: player @ ({},{})  score {}", world.px, world.py, world.score);
    println!("replayed {} move(s) from {rec_path}", rep.moves.len());
    Ok(())
}

/// `shot` — render a level with the *real* sprite/palette renderer into a
/// headless framebuffer and write it as a binary P6 PPM (Phase 7). No display
/// needed; the output is a viewable image proving the renderer works.
fn cmd_shot(args: &[String]) -> Result<(), String> {
    let usage = "usage: bitmaze shot <level.bm> <out.ppm> [tile_px]";
    let (in_path, out_path, tile_px) = match args {
        [i, o] => (i, o, window::TILE_PX),
        [i, o, t] => {
            let px: usize = t.parse().map_err(|_| format!("tile_px '{t}' must be a number\n{usage}"))?;
            if px == 0 {
                return Err("tile_px must be >= 1".to_string());
            }
            (i, o, px)
        }
        _ => return Err(usage.to_string()),
    };
    let level = load(in_path)?;
    let world = World::new(level).map_err(|e| format!("{in_path}: {e}"))?;
    let (sprites, _notes) = Sprites::load_from_dir(sprite::SPRITE_DIR);

    let fb_w = framebuffer::fb_width(&world, tile_px);
    let fb_h = framebuffer::fb_height(&world, tile_px);
    let mut fb = vec![0u32; fb_w * fb_h];
    framebuffer::draw(&world, &mut fb, fb_w, fb_h, tile_px, &sprites, &sprite::Palette::DEFAULT);

    let ppm = framebuffer::to_ppm(&fb, fb_w, fb_h);
    std::fs::write(out_path, &ppm).map_err(|e| format!("cannot write {out_path}: {e}"))?;
    println!("wrote {out_path}: P6 PPM {fb_w}x{fb_h} ({} bytes)", ppm.len());
    Ok(())
}

/// `gen-levels` — write the built-in sample levels (walls + items + an
/// assembler-built trigger) into a directory. The documented helper that embeds
/// assembled bytecode into a `.bm` script table (see `src/samples.rs`).
fn cmd_gen_levels(args: &[String]) -> Result<(), String> {
    let [dir] = args else {
        return Err("usage: bitmaze gen-levels <dir>".to_string());
    };
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {dir}: {e}"))?;
    for (name, level) in samples::all() {
        let path = format!("{dir}/{name}");
        let bytes = level.to_bytes();
        std::fs::write(&path, &bytes).map_err(|e| format!("cannot write {path}: {e}"))?;
        println!("wrote {path}: {}x{} ({} bytes)", level.width, level.height, bytes.len());
    }
    Ok(())
}

/// `sprite` — the sprite counterpart of `dump`/`new`. With one `.spr` path it
/// prints the sprite as ASCII art (the headless verification tool). With
/// `gen <dir>` it writes the compiled-in default sprites into `<dir>`.
fn cmd_sprite(args: &[String]) -> Result<(), String> {
    match args {
        [sub, dir] if sub == "gen" => {
            std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {dir}: {e}"))?;
            let defaults = [
                ("wall.spr", sprite::Sprite::default_wall()),
                ("floor.spr", sprite::Sprite::default_floor()),
                ("player.spr", sprite::Sprite::default_player()),
                ("item.spr", sprite::Sprite::default_item()),
            ];
            for (name, spr) in defaults {
                let path = format!("{dir}/{name}");
                std::fs::write(&path, spr.to_bytes())
                    .map_err(|e| format!("cannot write {path}: {e}"))?;
                println!("wrote {path} ({}x{})", spr.width, spr.height);
            }
            Ok(())
        }
        [path] => {
            let data = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
            let spr = sprite::Sprite::from_bytes(&data).map_err(|e| format!("{path}: {e}"))?;
            println!("sprite: {}x{}  '#' = ink, '.' = paper", spr.width, spr.height);
            print!("{}", spr.to_ascii());
            Ok(())
        }
        _ => Err("usage: bitmaze sprite <file.spr> | bitmaze sprite gen <dir>".to_string()),
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
