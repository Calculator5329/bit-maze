//! The game world — pure, deterministic, headless-testable game state.
//!
//! This module holds *only* world state and the step function. It performs no
//! I/O, no windowing, no time, and no randomness: given a [`Level`] and a
//! sequence of [`Move`]s, the final state is fixed. This is the layer Phase 3
//! (the minifb window) reuses unchanged — the terminal front-end in
//! `crate::play` is a thin, replaceable shell around this.

use crate::format::Level;

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

/// A loaded level plus the player position. Plane 0 is the walls plane
/// (`1` = wall, `0` = floor).
#[derive(Debug, Clone)]
pub struct World {
    pub level: Level,
    /// Player column, `0..width`.
    pub px: u8,
    /// Player row, `0..height`.
    pub py: u8,
}

impl World {
    /// Build a world from a level, spawning the player on the **first floor
    /// tile in row-major order** (scanning left-to-right, top-to-bottom, for
    /// the first walls-plane bit == 0). Fails with [`SpawnError::AllWalls`] if
    /// the level has no floor tile.
    pub fn new(level: Level) -> Result<World, SpawnError> {
        for y in 0..level.height {
            for x in 0..level.width {
                if !level.get_bit(0, x, y) {
                    return Ok(World { level, px: x, py: y });
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
