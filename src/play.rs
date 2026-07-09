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

use crate::world::{GameState, Move, World};
use std::io::{BufRead, Write};

/// Controls banner printed once at startup.
pub const CONTROLS: &str = "controls: w/a/s/d = move, q = quit  (press Enter after each key)";

/// Render one frame: the map with the player, a status line (with the live
/// item score), and the controls hint. Kept out of the loop so it's easy to
/// reason about and test.
fn frame(world: &World, status: &str) -> String {
    format!(
        "{}player @ ({},{})  score {} — {}\n{}\n",
        world.render(),
        world.px,
        world.py,
        world.score,
        status,
        CONTROLS,
    )
}

/// Run the interactive terminal loop over `world`, reading keys from `input`
/// and drawing frames to `output`. Returns on `q` or EOF the **sequence of
/// movement inputs applied**, in order — the caller turns that into a `.rec`
/// replay when `--record` is set. Every directional key that reaches
/// `step_triggered` is recorded (including ones that end up `Blocked`), so a
/// replay reproduces the run tile-for-tile.
///
/// Generic over the reader/writer so tests can drive it with in-memory buffers;
/// `main` passes real stdin/stdout.
pub fn run<R: BufRead, W: Write>(
    world: &mut World,
    mut input: R,
    mut output: W,
) -> std::io::Result<Vec<Move>> {
    let mut recorded: Vec<Move> = Vec::new();
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
                return Ok(recorded);
            }
            let Some(mv) = Move::from_key(c) else {
                continue; // ignore whitespace, unknown keys
            };
            recorded.push(mv);
            let outcome = world.step_triggered(mv);
            let mut status = match outcome.result {
                crate::world::StepResult::Moved => "moved".to_string(),
                crate::world::StepResult::Blocked => "blocked".to_string(),
                crate::world::StepResult::Idle => "idle".to_string(),
            };
            if let Some(t) = outcome.trigger {
                // A trigger fired and (may have) mutated the world — the next
                // frame already reflects it since render reads live plane data.
                status = format!(
                    "{status} — trigger #{} at ({},{}) ran [{}]",
                    t.script_id, t.x, t.y, t.halt
                );
            }
            write!(output, "{}", frame(world, &status))?;
            output.flush()?;

            // Win/lose ends the run (Phase 8). The world is now terminal, so any
            // further input would be ignored anyway; print the outcome and stop.
            match world.state {
                GameState::Won => {
                    writeln!(output, "YOU WIN — collected all {} item(s).", world.total_items)?;
                    output.flush()?;
                    return Ok(recorded);
                }
                GameState::Lost => {
                    writeln!(output, "GAME OVER — you stepped on a hazard.")?;
                    output.flush()?;
                    return Ok(recorded);
                }
                GameState::Playing => {}
            }
        }
    }
    Ok(recorded)
}
