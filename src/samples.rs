//! Sample-level builders (Phase 7) — the "add a plane → add a mechanic" showcase.
//!
//! Each builder constructs a full `.bm` [`Level`] in memory: a **walls** plane
//! (plane 0), an **items** plane (plane 1), a byte-per-tile trigger plane, and a
//! script table whose bytecode is produced by the **Phase 5 assembler** from the
//! matching `scripts/*.asm` source (embedded here with `include_str!`, so the
//! same assembler the CLI exposes stuffs the script bytes into the level). This
//! is the documented "embed assembled bytecode into a `.bm` script table"
//! helper the Phase 5 notes flagged as missing.
//!
//! The `bitmaze gen-levels <dir>` command writes these to disk; a test asserts
//! the committed `levels/*.bm` byte-match these builders and pass `check`.

use crate::asm;
use crate::format::{plane_len, Level, Script};
use crate::world::{HAZARDS_PLANE, ITEMS_PLANE, WALLS_PLANE};

/// The garden gate script (`scripts/gate.asm`), assembled at build time.
const GATE_ASM: &str = include_str!("../scripts/gate.asm");
/// The vault script (`scripts/vault.asm`), assembled at build time.
const VAULT_ASM: &str = include_str!("../scripts/vault.asm");
/// The trial gate script (`scripts/trial.asm`), assembled at build time.
const TRIAL_ASM: &str = include_str!("../scripts/trial.asm");

/// Start an items-carrying level: a blank walls plane plus an empty items plane
/// and a zeroed trigger plane, ready for the caller to paint.
fn base(width: u8, height: u8) -> (Level, Vec<u8>) {
    let mut level = Level::blank(width, height);
    level.planes.push(vec![0u8; plane_len(width, height)]); // items plane
    let triggers = vec![0u8; width as usize * height as usize];
    (level, triggers)
}

/// Draw a solid wall border around the level.
fn border(level: &mut Level) {
    let (w, h) = (level.width, level.height);
    for x in 0..w {
        level.set_bit(WALLS_PLANE, x, 0, true);
        level.set_bit(WALLS_PLANE, x, h - 1, true);
    }
    for y in 0..h {
        level.set_bit(WALLS_PLANE, 0, y, true);
        level.set_bit(WALLS_PLANE, w - 1, y, true);
    }
}

/// The **garden**: a 10×7 room split by a vertical gate. A pressure plate at
/// (2,2) runs `gate.asm` to clear the gate at (5,3), joining the halves so the
/// items on the far side become reachable. Five items in all.
pub fn garden() -> Level {
    let (mut level, mut triggers) = base(10, 7);
    border(&mut level);

    // Vertical divider at x=5, rows 1..=5. (5,3) is the gate the plate opens.
    for y in 1..6 {
        level.set_bit(WALLS_PLANE, 5, y, true);
    }

    // Items: two on the near (left) side, three behind the gate.
    for &(x, y) in &[(3u8, 1u8), (2, 4), (7, 1), (7, 3), (7, 5)] {
        level.set_bit(ITEMS_PLANE, x, y, true);
    }

    // Pressure plate at (2,2) -> script id 1.
    triggers[2 * 10 + 2] = 1;
    level.triggers = Some(triggers);

    let bytes = asm::assemble(GATE_ASM).expect("gate.asm assembles");
    level.scripts = Some(vec![Script { id: 1, bytes }]);
    level
}

/// The **vault**: an 8×6 room with a walled-off vault column. A plate at (2,2)
/// runs `vault.asm` — which bumps a scratch-RAM visit counter (LOAD/STORE) and
/// then opens the vault at (5,3) — so the item behind it can be collected.
pub fn vault() -> Level {
    let (mut level, mut triggers) = base(8, 6);
    border(&mut level);

    // Vault wall column at x=5, rows 2..=4. (5,3) is the door the plate opens.
    for y in 2..5 {
        level.set_bit(WALLS_PLANE, 5, y, true);
    }

    // Items: two in the open room, one locked behind the vault.
    for &(x, y) in &[(3u8, 1u8), (2, 4), (6, 3)] {
        level.set_bit(ITEMS_PLANE, x, y, true);
    }

    triggers[2 * 8 + 2] = 1; // plate at (2,2)
    level.triggers = Some(triggers);

    let bytes = asm::assemble(VAULT_ASM).expect("vault.asm assembles");
    level.scripts = Some(vec![Script { id: 1, bytes }]);
    level
}

/// The **trial**: the Phase 8 showcase — a genuinely winnable/losable level with
/// walls, items, a **hazards** plane (plane 2), and a **one-shot** trigger. An
/// 8×6 room whose right column (items at (6,1) and (6,4)) is walled off behind a
/// gate at (5,3); a one-shot plate at (3,1) (trigger id `0x80`) opens it. A spike
/// hazard sits at (2,3): stepping onto it loses. Collect all three items —
/// (6,1), (6,4), (1,4) — while avoiding the spikes to win.
pub fn trial() -> Level {
    let (mut level, mut triggers) = base(8, 6);
    level.planes.push(vec![0u8; plane_len(8, 6)]); // hazards plane (plane 2)
    border(&mut level);

    // Divider wall at x=5, rows 1..=4 — a full wall that seals off the right
    // column. (5,3) is the gate; it starts closed and the plate clears it.
    for y in 1..5 {
        level.set_bit(WALLS_PLANE, 5, y, true);
    }

    // Three items: two walled off in the right column, one in the open room.
    for &(x, y) in &[(6u8, 1u8), (6, 4), (1, 4)] {
        level.set_bit(ITEMS_PLANE, x, y, true);
    }

    // One spike hazard in the open room — a lose tile to route around.
    level.set_bit(HAZARDS_PLANE, 2, 3, true);

    // One-shot pressure plate at (3,1): trigger id 128 (0x80) → the high bit
    // marks it one-shot (fires only the first entry). See docs/VM.md.
    triggers[8 + 3] = 0x80; // tile (3,1) in an 8-wide grid: y*w + x = 1*8 + 3
    level.triggers = Some(triggers);

    let bytes = asm::assemble(TRIAL_ASM).expect("trial.asm assembles");
    level.scripts = Some(vec![Script { id: 0x80, bytes }]);
    level
}

/// Every sample, as `(filename, level)` pairs. `bitmaze gen-levels` writes each.
pub fn all() -> Vec<(&'static str, Level)> {
    vec![("garden.bm", garden()), ("vault.bm", vault()), ("trial.bm", trial())]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_valid_and_reparse() {
        for (name, level) in all() {
            let bytes = level.to_bytes();
            let back = Level::from_bytes(&bytes).expect(name);
            assert_eq!(back, level, "{name} round-trips");
            // At least walls + items planes (trial adds a third HAZARDS plane),
            // with triggers and scripts present.
            assert!(level.planes.len() >= 2, "{name} has walls + items planes");
            assert!(level.triggers.is_some(), "{name} has a trigger plane");
            assert!(level.scripts.is_some(), "{name} has a script table");
        }
    }

    #[test]
    fn trial_has_a_hazards_plane_and_a_one_shot_trigger() {
        let level = trial();
        assert_eq!(level.planes.len(), 3, "trial has walls + items + hazards planes");
        // The plate at (3,1) is a one-shot trigger (high-bit id 0x80).
        let triggers = level.triggers.as_ref().unwrap();
        assert_eq!(triggers[8 + 3], 0x80, "one-shot plate id has the high bit set");
        // The gate script is the assembled clr_wall(5,3).
        assert_eq!(level.scripts.unwrap()[0].bytes, vec![0x10, 0x05, 0x10, 0x03, 0x32, 0x01]);
    }

    #[test]
    fn garden_gate_script_is_the_assembled_bytecode() {
        let level = garden();
        let scripts = level.scripts.unwrap();
        assert_eq!(scripts[0].bytes, vec![0x10, 0x05, 0x10, 0x03, 0x32, 0x01]);
    }
}
