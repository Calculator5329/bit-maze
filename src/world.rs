//! The game world — pure, deterministic, headless-testable game state.
//!
//! This module holds *only* world state and the step function. It performs no
//! I/O, no windowing, no time, and no randomness: given a [`Level`] and a
//! sequence of [`Move`]s, the final state is fixed. This is the layer Phase 3
//! (the minifb window) reuses unchanged — the terminal front-end in
//! `crate::play` is a thin, replaceable shell around this.

use crate::format::Level;
use crate::vm::{Halt, Vm, VmHost};

/// Bitplane index of the walls plane (`1` = wall). Always present.
pub const WALLS_PLANE: usize = 0;
/// Bitplane index of the items plane (`1` = item present). Optional (Phase 7):
/// a level with only the walls plane simply has no items to collect.
pub const ITEMS_PLANE: usize = 1;
/// Bitplane index of the hazards plane (`1` = spike/hazard present). Optional
/// (Phase 8): a level with only walls (+ items) simply has no hazards. Stepping
/// onto a set hazard bit loses the game. A level that opts in has `planes = 3`;
/// `.bm` stays v1 (the multi-plane mechanism was in the format from day one).
pub const HAZARDS_PLANE: usize = 2;

/// Whether the game is still in progress, won, or lost (Phase 8). The single
/// source of truth for the win/lose outcome, held on [`World`]. It is pure state
/// — set only by [`World::step`] from deterministic world logic (no time, no
/// RNG), so replays reproduce it exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    /// The game is in progress; moves are accepted.
    Playing,
    /// Every item in the level has been collected. Terminal.
    Won,
    /// The player stepped onto a hazard tile. Terminal.
    Lost,
}

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
    /// Items collected so far (Phase 7). Incremented when the player steps onto
    /// a tile whose items-plane bit is set; the bit is then cleared. This is the
    /// single source of truth for the score — front-ends only read it.
    pub score: u32,
    /// Total items in the level at construction (Phase 8). When [`World::score`]
    /// reaches this (and it is nonzero), the game is [`GameState::Won`]. A level
    /// with zero items has **no win-by-collection condition** — it is endless
    /// sandbox play (so existing itemless samples never insta-win).
    pub total_items: u32,
    /// Win/lose state (Phase 8). The single source of truth for the outcome; see
    /// [`GameState`]. Once it is not [`GameState::Playing`], [`World::step`]
    /// ignores further moves (documented no-op).
    pub state: GameState,
    /// One-shot trigger latch (Phase 8), one flag per tile. A trigger whose
    /// script id has the **high bit set** (`0x80..=0xFF`, i.e. 128..=255) is
    /// *one-shot*: it fires only the first time the player enters that tile this
    /// run, and its tile flag is then set so re-entering does nothing. Ids
    /// `1..=127` are *repeating* (fire every entry, the pre-Phase-8 behavior).
    /// This is pure **runtime** state — not stored in `.bm` — so replays rebuild
    /// it deterministically from the same move sequence.
    fired: Vec<bool>,
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
        let total_items = count_items(&level);
        let tiles = level.width as usize * level.height as usize;
        for y in 0..level.height {
            for x in 0..level.width {
                if !level.get_bit(WALLS_PLANE, x, y) {
                    let mut world = World {
                        level,
                        px: x,
                        py: y,
                        seed,
                        score: 0,
                        total_items,
                        state: GameState::Playing,
                        fired: vec![false; tiles],
                    };
                    // The player may spawn on top of an item; collect it too so
                    // the score is consistent with "standing on a picked tile"
                    // (this can win a 1-item level whose only item is the spawn).
                    world.collect_item();
                    return Ok(world);
                }
            }
        }
        Err(SpawnError::AllWalls)
    }

    /// Advance the world by one input. Moving into a wall or off the map edge
    /// is a no-op (the player stays). Pure: no I/O, no randomness, no time.
    ///
    /// Phase 8: once the game is over ([`GameState::Won`]/[`GameState::Lost`]),
    /// every move is ignored — a documented no-op that reports [`StepResult::Idle`].
    /// Moving onto a **hazard** tile loses (`state` becomes [`GameState::Lost`]);
    /// collecting the last item wins ([`GameState::Won`]). Both are set here so
    /// the outcome is pure, deterministic world logic.
    pub fn step(&mut self, input: Move) -> StepResult {
        // Game over: further moves do nothing (single source of truth is `state`).
        if self.state != GameState::Playing {
            return StepResult::Idle;
        }

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
        if self.level.get_bit(WALLS_PLANE, nx, ny) {
            return StepResult::Blocked;
        }

        self.px = nx;
        self.py = ny;

        // Stepping onto a hazard loses immediately (Phase 8). The move still
        // happened (the player is on the hazard tile), so this is a `Moved`; the
        // hazard takes precedence over any item, which is not collected.
        if self.level.get_bit(HAZARDS_PLANE, nx, ny) {
            self.state = GameState::Lost;
            return StepResult::Moved;
        }

        // Pick up an item if the tile just entered carries one (Phase 7); this
        // may complete the level and set `state` to `Won` (Phase 8).
        self.collect_item();
        StepResult::Moved
    }

    /// If the player's current tile has an items-plane bit set, collect it:
    /// clear the bit and increment [`World::score`]. Pure and deterministic. A
    /// level with no items plane never collects anything (the read is a safe
    /// `false`). This is the single spot that mutates the score.
    fn collect_item(&mut self) {
        if self.level.get_bit(ITEMS_PLANE, self.px, self.py) {
            self.level.set_bit(ITEMS_PLANE, self.px, self.py, false);
            self.score += 1;
            // Win when every item has been collected (Phase 8). A level with no
            // items (`total_items == 0`) has no win condition: endless sandbox.
            if self.total_items > 0 && self.score >= self.total_items {
                self.state = GameState::Won;
            }
        }
    }

    /// Advance the world by one input **and fire any trigger** the player lands
    /// on. This is the game-facing step: it calls the pure [`World::step`] for
    /// movement, then — only on a successful `Moved` — checks the trigger plane
    /// at the new tile and, if the byte is nonzero, runs the matching script on
    /// the BitVM against `self`, which may mutate the walls plane (e.g. open a
    /// door). Both front-ends call this so trigger-driven wall changes show up.
    ///
    /// Firing semantics (documented):
    /// - Fires **after** the move resolves, on the tile just entered — but only
    ///   while the game is still [`GameState::Playing`] (a move that lost by
    ///   stepping onto a hazard fires nothing).
    /// - **One-shot vs repeating by script-id convention (Phase 8).** A trigger
    ///   id with the high bit set (`0x80..=0xFF`) is *one-shot*: it fires only
    ///   the first time the player enters that tile this run. Ids `1..=127` are
    ///   *repeating* — re-entering fires again every time (the pre-Phase-8
    ///   behavior; idempotent scripts like the door are safe to re-fire). The
    ///   latch is per-tile runtime state, not stored in `.bm`, so replays rebuild
    ///   it deterministically.
    /// - A trigger id with no matching script, or a zero byte, fires nothing.
    /// - The VM is hard-capped, so a malformed/looping script halts cleanly and
    ///   the game continues; it can never hang or crash here.
    pub fn step_triggered(&mut self, input: Move) -> StepOutcome {
        let result = self.step(input);
        // Only fire on a successful move that did not end the game (a hazard-loss
        // move is a `Moved` but leaves `state == Lost`, so it fires nothing).
        let trigger = if result == StepResult::Moved && self.state == GameState::Playing {
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
        let idx = y as usize * self.level.width as usize + x as usize;

        // Look up the trigger id and clone the script bytes, ending all borrows
        // of `self.level` before we hand `self` to the VM as a mutable host.
        let (id, bytes) = {
            let triggers = self.level.triggers.as_ref()?;
            let id = *triggers.get(idx)?;
            if id == 0 {
                return None;
            }
            // One-shot latch (Phase 8): a high-bit id fires once per tile per run.
            if id & 0x80 != 0 && *self.fired.get(idx).unwrap_or(&false) {
                return None;
            }
            let script = self.level.scripts.as_ref()?.iter().find(|s| s.id == id)?;
            (id, script.bytes.clone())
        };

        // Latch a one-shot trigger now that we know it will fire.
        if id & 0x80 != 0 {
            if let Some(flag) = self.fired.get_mut(idx) {
                *flag = true;
            }
        }

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

    /// Draw the world as ASCII: `@` = player, `#` = wall, `^` = hazard/spike,
    /// `*` = uncollected item, `.` = floor. Player takes precedence over
    /// everything, then walls, then hazards, then items. Each row ends in a
    /// newline. Snapshot-friendly and deterministic.
    pub fn render(&self) -> String {
        let mut out = String::with_capacity(
            (self.level.width as usize + 1) * self.level.height as usize,
        );
        for y in 0..self.level.height {
            for x in 0..self.level.width {
                let ch = if x == self.px && y == self.py {
                    '@'
                } else if self.level.get_bit(WALLS_PLANE, x, y) {
                    '#'
                } else if self.level.get_bit(HAZARDS_PLANE, x, y) {
                    '^'
                } else if self.level.get_bit(ITEMS_PLANE, x, y) {
                    '*'
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

/// Count the set bits in the items plane — the level's total collectible count
/// (Phase 8). Pure. A level with no items plane counts `0` (endless play).
fn count_items(level: &Level) -> u32 {
    let mut n = 0;
    for y in 0..level.height {
        for x in 0..level.width {
            if level.get_bit(ITEMS_PLANE, x, y) {
                n += 1;
            }
        }
    }
    n
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

    fn get_item(&self, x: u16, y: u16) -> bool {
        if x >= self.level.width as u16 || y >= self.level.height as u16 {
            return false; // out of bounds reads as "no item"
        }
        self.level.get_bit(ITEMS_PLANE, x as u8, y as u8)
    }

    fn score(&self) -> u16 {
        self.score.min(u16::MAX as u32) as u16
    }

    fn get_hazard(&self, x: u16, y: u16) -> bool {
        if x >= self.level.width as u16 || y >= self.level.height as u16 {
            return false; // out of bounds reads as "no hazard"
        }
        self.level.get_bit(HAZARDS_PLANE, x as u8, y as u8)
    }
}
