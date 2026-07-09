//! The game world — pure, deterministic, headless-testable game state.
//!
//! This module holds *only* world state and the step function. It performs no
//! I/O, no windowing, no time, and no randomness: given a [`Level`] and a
//! sequence of [`Move`]s, the final state is fixed. This is the layer Phase 3
//! (the minifb window) reuses unchanged — the terminal front-end in
//! `crate::play` is a thin, replaceable shell around this.

use crate::format::Level;
use crate::vm::{Halt, Vm, VmHost};

/// A single movement input for one [`World::step`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    Left,
    Right,
    /// No input this step; the world is unchanged.
    None,
}

impl Move {
    /// Map a WASD key to a [`Move`] (case-insensitive). Returns `None` for any
    /// key that is not a movement key (e.g. `q`), so front-ends can distinguish
    /// "no move" from "quit" themselves.
    pub fn from_key(c: char) -> Option<Move> {
        match c.to_ascii_lowercase() {
            'w' => Some(Move::Up),
            'a' => Some(Move::Left),
            's' => Some(Move::Down),
            'd' => Some(Move::Right),
            _ => None,
        }
    }
}

/// Outcome of a single [`World::step`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    /// The player moved to a new tile.
    Moved,
    /// A move was attempted but the target was a wall or off the map edge; the
    /// player stayed put (a no-op).
    Blocked,
    /// The input was [`Move::None`]; nothing happened.
    Idle,
}

/// Why a level could not be turned into a playable [`World`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnError {
    /// Every tile in the walls plane is a wall — nowhere to place the player.
    AllWalls,
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnError::AllWalls => {
                write!(f, "level is entirely walls: no floor tile to spawn the player on")
            }
        }
    }
}

impl std::error::Error for SpawnError {}

/// The result of a trigger script running after the player entered a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriggerRun {
    /// The script id from the trigger plane (1..=255).
    pub script_id: u8,
    /// The tile whose trigger fired (where the player landed).
    pub x: u8,
    pub y: u8,
    /// Why the script's VM run stopped. `Halt::Halt`/`EndOfScript` are clean;
    /// anything else means a cap tripped or the bytecode was malformed. Either
    /// way the game continues — a bad script simply does less than intended.
    pub halt: Halt,
}

/// The full outcome of one [`World::step_triggered`]: the movement result plus
/// any trigger that fired as a consequence of a successful move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepOutcome {
    /// The movement result (identical to what pure [`World::step`] returns).
    pub result: StepResult,
    /// `Some` iff this step **moved** onto a tile with a nonzero trigger byte
    /// whose script was found and run. `None` on a blocked/idle step, a
    /// zero-trigger tile, or a trigger id with no matching script.
    pub trigger: Option<TriggerRun>,
}

/// A loaded level plus the player position. Plane 0 is the walls plane
/// (`1` = wall, `0` = floor).
#[derive(Debug, Clone)]
pub struct World {
    pub level: Level,
    /// Player column, `0..width`.
    pub px: u8,
    /// Player row, `0..height`.
    pub py: u8,
    /// Run seed for the trigger VM's PRNG. Derived deterministically from the
    /// level bytes at construction so replays are reproducible; a front-end can
    /// overwrite it (it's a plain field) to vary a run without touching the VM.
    pub seed: u32,
}

/// Derive a non-zero run seed deterministically from bytes (FNV-1a/32). Pure —
/// same level always yields the same seed, so trigger RNG is reproducible.
fn derive_seed(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for &b in bytes {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    if h == 0 {
        0xDEAD_BEEF
    } else {
        h
    }
}

impl World {
    /// Build a world from a level, spawning the player on the **first floor
    /// tile in row-major order** (scanning left-to-right, top-to-bottom, for
    /// the first walls-plane bit == 0). Fails with [`SpawnError::AllWalls`] if
    /// the level has no floor tile.
    pub fn new(level: Level) -> Result<World, SpawnError> {
        let seed = derive_seed(&level.to_bytes());
        for y in 0..level.height {
            for x in 0..level.width {
                if !level.get_bit(0, x, y) {
                    return Ok(World { level, px: x, py: y, seed });
                }
            }
        }
        Err(SpawnError::AllWalls)
    }

    /// Advance the world by one input. Moving into a wall or off the map edge
    /// is a no-op (the player stays). Pure: no I/O, no randomness, no time.
    pub fn step(&mut self, input: Move) -> StepResult {
        let (dx, dy) = match input {
            Move::Up => (0i32, -1i32),
            Move::Down => (0, 1),
            Move::Left => (-1, 0),
            Move::Right => (1, 0),
            Move::None => return StepResult::Idle,
        };

        let nx = self.px as i32 + dx;
        let ny = self.py as i32 + dy;

        // Off the map edge -> no-op.
        if nx < 0 || ny < 0 || nx >= self.level.width as i32 || ny >= self.level.height as i32 {
            return StepResult::Blocked;
        }

        let (nx, ny) = (nx as u8, ny as u8);

        // Into a wall -> no-op.
        if self.level.get_bit(0, nx, ny) {
            return StepResult::Blocked;
        }

        self.px = nx;
        self.py = ny;
        StepResult::Moved
    }

    /// Advance the world by one input **and fire any trigger** the player lands
    /// on. This is the game-facing step: it calls the pure [`World::step`] for
    /// movement, then — only on a successful `Moved` — checks the trigger plane
    /// at the new tile and, if the byte is nonzero, runs the matching script on
    /// the BitVM against `self`, which may mutate the walls plane (e.g. open a
    /// door). Both front-ends call this so trigger-driven wall changes show up.
    ///
    /// Firing semantics (v0, deliberately simple and documented):
    /// - Fires **after** the move resolves, on the tile just entered.
    /// - **Stateless**: re-entering the same plate fires again every time. There
    ///   is no one-shot latch in v0 (clearing an already-clear wall is a no-op,
    ///   so idempotent scripts like the door are naturally safe to re-fire).
    /// - A trigger id with no matching script, or a zero byte, fires nothing.
    /// - The VM is hard-capped, so a malformed/looping script halts cleanly and
    ///   the game continues; it can never hang or crash here.
    pub fn step_triggered(&mut self, input: Move) -> StepOutcome {
        let result = self.step(input);
        let trigger = if result == StepResult::Moved {
            self.fire_trigger()
        } else {
            None
        };
        StepOutcome { result, trigger }
    }

    /// If the player's current tile has a nonzero trigger byte with a matching
    /// script, run it on a fresh VM against this world and report the outcome.
    fn fire_trigger(&mut self) -> Option<TriggerRun> {
        let (x, y) = (self.px, self.py);

        // Look up the trigger id and clone the script bytes, ending all borrows
        // of `self.level` before we hand `self` to the VM as a mutable host.
        let (id, bytes) = {
            let triggers = self.level.triggers.as_ref()?;
            let idx = y as usize * self.level.width as usize + x as usize;
            let id = *triggers.get(idx)?;
            if id == 0 {
                return None;
            }
            let script = self.level.scripts.as_ref()?.iter().find(|s| s.id == id)?;
            (id, script.bytes.clone())
        };

        // Seed the run deterministically from the world seed mixed with the
        // plate coordinates, so different plates get different RAND streams and
        // the same plate replays identically.
        let seed = self
            .seed
            .wrapping_add((x as u32).wrapping_mul(0x9E37_79B1))
            .wrapping_add((y as u32).wrapping_mul(0x85EB_CA77));

        let halt = Vm::new(seed).run(&bytes, self);
        Some(TriggerRun { script_id: id, x, y, halt })
    }

    /// Draw the walls plane as ASCII (`#` = wall, `.` = floor, reusing the
    /// `dump` convention) with the player drawn as `@` on top. Each row ends in
    /// a newline. Snapshot-friendly and deterministic.
    pub fn render(&self) -> String {
        let mut out = String::with_capacity(
            (self.level.width as usize + 1) * self.level.height as usize,
        );
        for y in 0..self.level.height {
            for x in 0..self.level.width {
                let ch = if x == self.px && y == self.py {
                    '@'
                } else if self.level.get_bit(0, x, y) {
                    '#'
                } else {
                    '.'
                };
                out.push(ch);
            }
            out.push('\n');
        }
        out
    }
}

/// The world's implementation of the VM's world seam. All coordinate handling
/// is bounds-checked here — the single chokepoint that makes out-of-range
/// `GET_WALL`/`SET_WALL`/`CLR_WALL` safe no-ops instead of panics.
impl VmHost for World {
    fn get_wall(&self, x: u16, y: u16) -> bool {
        if x >= self.level.width as u16 || y >= self.level.height as u16 {
            return false; // out of bounds reads as "no wall"
        }
        self.level.get_bit(0, x as u8, y as u8)
    }

    fn set_wall(&mut self, x: u16, y: u16, value: bool) {
        if x >= self.level.width as u16 || y >= self.level.height as u16 {
            return; // out of bounds write is a no-op
        }
        self.level.set_bit(0, x as u8, y as u8, value);
    }

    fn player_x(&self) -> u16 {
        self.px as u16
    }

    fn player_y(&self) -> u16 {
        self.py as u16
    }
}
