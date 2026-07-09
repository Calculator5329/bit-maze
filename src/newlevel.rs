//! `bitmaze new` — generate a walls-only sample level.

use crate::format::Level;

/// Generate a bordered maze: a solid wall border, a floor interior, and a
/// couple of interior wall stubs so the result is visually interesting.
///
/// Deterministic (no randomness) so regenerating is reproducible. Produces a
/// walls-only level: `flags = 0`, a single bitplane.
pub fn generate(width: u8, height: u8) -> Level {
    let mut level = Level::blank(width, height);

    for y in 0..height {
        for x in 0..width {
            let border = x == 0 || y == 0 || x == width - 1 || y == height - 1;
            if border {
                level.set_bit(0, x, y, true);
            }
        }
    }

    // A vertical interior wall a third of the way across, with a gap so the
    // interior stays connected. Only draw it when there's room.
    if width >= 5 && height >= 5 {
        let wx = width / 3;
        let gap = height / 2;
        for y in 1..height - 1 {
            if y != gap {
                level.set_bit(0, wx, y, true);
            }
        }
        // A short horizontal stub reaching in from the right wall.
        let hy = height / 2;
        let stub_end = width.saturating_sub(width / 4).max(wx + 2);
        for x in stub_end..width - 1 {
            level.set_bit(0, x, hy, true);
        }
    }

    level
}
