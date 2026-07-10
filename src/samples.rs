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
/// The circuit's first and second gate scripts, assembled independently.
const CIRCUIT_A_ASM: &str = include_str!("../scripts/circuit-a.asm");
const CIRCUIT_B_ASM: &str = include_str!("../scripts/circuit-b.asm");

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

/// The **circuit**: a 24×16 three-sector level with 12 items, 9 hazards, and
/// two one-shot plates. Plate A at (5,2) opens divider gate (8,4), then plate B
/// at (12,13) opens divider gate (16,11). It has eight times as many tiles as
/// `trial`, making it the first sample intended to exercise larger-level
/// storage and rendering while remaining compact and hand-inspectable.
pub fn circuit() -> Level {
    let (mut level, mut triggers) = base(24, 16);
    level.planes.push(vec![0u8; plane_len(24, 16)]); // hazards plane
    border(&mut level);

    // Three sectors divided by full-height walls. Each has one scripted gate.
    for y in 1..15 {
        level.set_bit(WALLS_PLANE, 8, y, true);
        level.set_bit(WALLS_PLANE, 16, y, true);
    }

    // Short walls turn each sector into a small maze without creating isolated
    // pockets. The explicit gaps are the intended routes through each band.
    for x in 2..=6 {
        if x != 4 { level.set_bit(WALLS_PLANE, x, 6, true); }
    }
    for x in 2..=7 {
        if x != 6 { level.set_bit(WALLS_PLANE, x, 10, true); }
    }
    for x in 10..=15 {
        if x != 13 { level.set_bit(WALLS_PLANE, x, 5, true); }
    }
    for x in 9..=14 {
        if x != 11 { level.set_bit(WALLS_PLANE, x, 9, true); }
    }
    for x in 18..=22 {
        if x != 20 { level.set_bit(WALLS_PLANE, x, 6, true); }
    }
    for x in 17..=21 {
        if x != 19 { level.set_bit(WALLS_PLANE, x, 10, true); }
    }

    for &(x, y) in &[
        (3u8, 3u8), (6, 11), (2, 13),
        (10, 2), (14, 4), (11, 8), (14, 13),
        (18, 2), (21, 4), (19, 8), (21, 12), (18, 14),
    ] {
        level.set_bit(ITEMS_PLANE, x, y, true);
    }

    for &(x, y) in &[
        (4u8, 4u8), (3, 8), (7, 13),
        (11, 3), (14, 7), (10, 12),
        (19, 3), (22, 8), (18, 12),
    ] {
        level.set_bit(HAZARDS_PLANE, x, y, true);
    }

    triggers[2 * 24 + 5] = 0x80; // plate A at (5,2)
    triggers[13 * 24 + 12] = 0x81; // plate B at (12,13)
    level.triggers = Some(triggers);
    level.scripts = Some(vec![
        Script { id: 0x80, bytes: asm::assemble(CIRCUIT_A_ASM).expect("circuit-a.asm assembles") },
        Script { id: 0x81, bytes: asm::assemble(CIRCUIT_B_ASM).expect("circuit-b.asm assembles") },
    ]);
    level
}

/// Every sample, as `(filename, level)` pairs. `bitmaze gen-levels` writes each.
pub fn all() -> Vec<(&'static str, Level)> {
    vec![
        ("garden.bm", garden()),
        ("vault.bm", vault()),
        ("trial.bm", trial()),
        ("circuit.bm", circuit()),
    ]
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
        let scripts = level.scripts.as_ref().unwrap();
        assert_eq!(scripts[0].bytes, vec![0x10, 0x05, 0x10, 0x03, 0x32, 0x01]);
    }

    #[test]
    fn circuit_has_expected_scale_and_gate_programs() {
        let mut level = circuit();
        assert_eq!((level.width, level.height), (24, 16));
        assert_eq!(level.planes.len(), 3);
        assert_eq!(level.planes[ITEMS_PLANE].iter().map(|b| b.count_ones()).sum::<u32>(), 12);
        assert_eq!(level.planes[HAZARDS_PLANE].iter().map(|b| b.count_ones()).sum::<u32>(), 9);
        let triggers = level.triggers.as_ref().unwrap();
        assert_eq!(triggers[2 * 24 + 5], 0x80);
        assert_eq!(triggers[13 * 24 + 12], 0x81);
        let scripts = level.scripts.as_ref().unwrap();
        assert_eq!(scripts[0].bytes, vec![0x10, 8, 0x10, 4, 0x32, 0x01]);
        assert_eq!(scripts[1].bytes, vec![0x10, 16, 0x10, 11, 0x32, 0x01]);

        // Treat hazards as blocked and prove the intended progression has safe
        // routes: plate A, then plate B after gate A, then every collectible.
        let reachable = |level: &Level, start: (u8, u8)| {
            use std::collections::VecDeque;
            let mut seen = vec![false; level.tile_count()];
            let mut queue = VecDeque::from([start]);
            while let Some((x, y)) = queue.pop_front() {
                let idx = y as usize * level.width as usize + x as usize;
                if seen[idx]
                    || level.get_bit(WALLS_PLANE, x, y)
                    || level.get_bit(HAZARDS_PLANE, x, y)
                {
                    continue;
                }
                seen[idx] = true;
                for (nx, ny) in [
                    (x.wrapping_sub(1), y),
                    (x.saturating_add(1), y),
                    (x, y.wrapping_sub(1)),
                    (x, y.saturating_add(1)),
                ] {
                    if nx < level.width && ny < level.height {
                        queue.push_back((nx, ny));
                    }
                }
            }
            seen
        };
        let can_reach = |seen: &[bool], level: &Level, x: u8, y: u8| {
            seen[y as usize * level.width as usize + x as usize]
        };
        let first = reachable(&level, (1, 1));
        assert!(can_reach(&first, &level, 5, 2), "plate A has a safe route");
        level.set_bit(WALLS_PLANE, 8, 4, false);
        let second = reachable(&level, (1, 1));
        assert!(can_reach(&second, &level, 12, 13), "plate B is reachable after gate A");
        level.set_bit(WALLS_PLANE, 16, 11, false);
        let final_map = reachable(&level, (1, 1));
        for y in 0..level.height {
            for x in 0..level.width {
                if level.get_bit(ITEMS_PLANE, x, y) {
                    assert!(can_reach(&final_map, &level, x, y), "item ({x},{y}) has a safe route");
                }
            }
        }
    }
}
