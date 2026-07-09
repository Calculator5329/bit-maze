//! `bitmaze play` — the terminal front-end.
//!
//! This is the **I/O shell** only. All game logic lives in [`crate::world`];
//! this module reads keys and prints frames. Phase 3 swaps this shell for a
//! minifb window and reuses [`World`]/[`World::step`] verbatim.
//!
//! ## Input mode
//! Line-buffered, std-only — no raw-mode dependency. Type a movement key and
//! press Enter (`w`/`a`/`s`/`d` to move, `q` to quit). Multiple keys on one
//! line are processed left-to-right, so `dd<Enter>` steps right twice and piped
//! input like `printf 'd\nd\nq\n'` walks the player two tiles right then quits.
//! The loop also ends at end-of-input (EOF).

use crate::world::{Move, World};
use std::io::{BufRead, Write};

/// Controls banner printed once at startup.
pub const CONTROLS: &str = "controls: w/a/s/d = move, q = quit  (press Enter after each key)";

/// Render one frame: the map with the player, a status line, and the controls
/// hint. Kept out of the loop so it's easy to reason about and test.
fn frame(world: &World, status: &str) -> String {
    format!(
        "{}player @ ({},{}) — {}\n{}\n",
        world.render(),
        world.px,
        world.py,
        status,
        CONTROLS,
    )
}

/// Run the interactive terminal loop over `world`, reading keys from `input`
/// and drawing frames to `output`. Returns on `q` or EOF.
///
/// Generic over the reader/writer so tests can drive it with in-memory buffers;
/// `main` passes real stdin/stdout.
pub fn run<R: BufRead, W: Write>(
    world: &mut World,
    mut input: R,
    mut output: W,
) -> std::io::Result<()> {
    write!(output, "{}", frame(world, "start"))?;
    output.flush()?;

    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            break; // EOF
        }
        for c in line.chars() {
            if c == 'q' || c == 'Q' {
                writeln!(output, "bye.")?;
                output.flush()?;
                return Ok(());
            }
            let Some(mv) = Move::from_key(c) else {
                continue; // ignore whitespace, unknown keys
            };
            let status = match world.step(mv) {
                crate::world::StepResult::Moved => "moved",
                crate::world::StepResult::Blocked => "blocked",
                crate::world::StepResult::Idle => "idle",
            };
            write!(output, "{}", frame(world, status))?;
            output.flush()?;
        }
    }
    Ok(())
}
