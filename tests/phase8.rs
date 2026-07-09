//! Phase 8 integration tests: the gameplay loop — hazards + lose, win by
//! collecting all items, the one-shot trigger latch, `GET_HAZARD` through the
//! `World` host, and deterministic replay of the final `GameState`. All pure and
//! deterministic (no I/O, no time, no host RNG).

use bitmaze::format::{plane_len, Level, Script};
use bitmaze::replay::Replay;
use bitmaze::world::{GameState, Move, StepResult, HAZARDS_PLANE, ITEMS_PLANE};
use bitmaze::{samples, VmHost, World};

/// A 2×1 level with one floor spawn tile and a second tile; `paint` decorates it
/// (e.g. drops an item or hazard on the right tile) before it becomes a `World`.
fn two_tile_world(paint: impl FnOnce(&mut Level)) -> World {
    let mut level = Level::blank(2, 1); // walls plane only
    level.planes.push(vec![0u8; plane_len(2, 1)]); // items plane
    level.planes.push(vec![0u8; plane_len(2, 1)]); // hazards plane
    paint(&mut level);
    World::new(level).expect("has a floor spawn")
}

#[test]
fn stepping_onto_a_hazard_loses_the_game() {
    let mut world = two_tile_world(|lvl| lvl.set_bit(HAZARDS_PLANE, 1, 0, true));
    assert_eq!(world.state, GameState::Playing);
    assert_eq!((world.px, world.py), (0, 0));

    // Move right onto the hazard: the move happens, but the game is now Lost.
    let r = world.step(Move::Right);
    assert_eq!(r, StepResult::Moved);
    assert_eq!((world.px, world.py), (1, 0), "player did move onto the hazard");
    assert_eq!(world.state, GameState::Lost);

    // Once Lost, further moves are ignored (documented no-op).
    let after = world.step(Move::Left);
    assert_eq!(after, StepResult::Idle);
    assert_eq!((world.px, world.py), (1, 0), "no move after game over");
}

#[test]
fn collecting_the_last_item_wins() {
    let mut world = two_tile_world(|lvl| lvl.set_bit(ITEMS_PLANE, 1, 0, true));
    assert_eq!(world.total_items, 1);
    assert_eq!(world.state, GameState::Playing);

    let r = world.step(Move::Right); // collect the only item
    assert_eq!(r, StepResult::Moved);
    assert_eq!(world.score, 1);
    assert_eq!(world.state, GameState::Won, "all items collected -> Won");

    // Terminal: further moves ignored.
    assert_eq!(world.step(Move::Left), StepResult::Idle);
    assert_eq!((world.px, world.py), (1, 0));
}

#[test]
fn a_level_with_no_items_never_wins() {
    // Endless/sandbox: total_items == 0, so no win-by-collection.
    let mut world = two_tile_world(|_| {});
    assert_eq!(world.total_items, 0);
    world.step(Move::Right);
    assert_eq!(world.state, GameState::Playing, "itemless level is endless");
}

/// Build a 3×1 walls-free level with a trigger at tile (1,0) of the given id and
/// a matching one-byte `halt` script, so we can watch it fire (or not) on
/// repeated entry.
fn plate_world(id: u8) -> World {
    let mut level = Level::blank(3, 1);
    let mut triggers = vec![0u8; 3];
    triggers[1] = id; // tile (1,0)
    level.triggers = Some(triggers);
    level.scripts = Some(vec![Script { id, bytes: vec![0x01] }]); // halt
    World::new(level).expect("spawns at (0,0)")
}

#[test]
fn one_shot_trigger_fires_once_then_never_again() {
    // A high-bit id (0x80) is one-shot: fires only the first entry of the tile.
    let mut world = plate_world(0x80);
    let first = world.step_triggered(Move::Right); // (0,0)->(1,0) plate
    assert!(first.trigger.is_some(), "one-shot fires on first entry");

    world.step_triggered(Move::Left); // back to (0,0)
    let second = world.step_triggered(Move::Right); // re-enter the plate
    assert!(second.trigger.is_none(), "one-shot does NOT fire again");
}

#[test]
fn repeating_trigger_fires_every_entry() {
    // A low id (1) is repeating (the pre-Phase-8 behavior): fires every entry.
    let mut world = plate_world(1);
    assert!(world.step_triggered(Move::Right).trigger.is_some(), "fires first time");
    world.step_triggered(Move::Left);
    assert!(world.step_triggered(Move::Right).trigger.is_some(), "fires again (repeating)");
}

#[test]
fn get_hazard_through_the_world_host_is_bounds_safe() {
    let world = two_tile_world(|lvl| lvl.set_bit(HAZARDS_PLANE, 1, 0, true));
    assert!(world.get_hazard(1, 0), "hazard bit reads true");
    assert!(!world.get_hazard(0, 0), "floor reads false");
    assert!(!world.get_hazard(1000, 1000), "out of bounds is a safe false");
}

/// The trial level's winning route: collect all three items while avoiding the
/// spike at (2,3), tripping the one-shot plate to open the gate. Matches the
/// piped `--term` playthrough in the report.
const WIN_ROUTE: &[Move] = &[
    Move::Right, Move::Right, // (1,1)->(3,1) plate: gate (5,3) opens
    Move::Down, Move::Down, //   ->(3,3)
    Move::Right, Move::Right, Move::Right, // ->(6,3) through the gate
    Move::Up, Move::Up, //       ->(6,1) item 1
    Move::Down, Move::Down, Move::Down, // ->(6,4) item 2
    Move::Up, //                 ->(6,3)
    Move::Left, Move::Left, //   ->(4,3)
    Move::Down, //               ->(4,4) (steps around the spike at (2,3))
    Move::Left, Move::Left, Move::Left, // ->(1,4) item 3 -> WON
];

/// The trial level's losing route: walk straight into the spike at (2,3).
const LOSE_ROUTE: &[Move] = &[Move::Down, Move::Down, Move::Right];

#[test]
fn trial_winning_route_reaches_won() {
    let mut world = World::new(samples::trial()).unwrap();
    assert!(world.level.get_bit(0, 5, 3), "gate starts closed");
    for &mv in WIN_ROUTE {
        world.step_triggered(mv);
    }
    assert_eq!(world.score, 3);
    assert_eq!(world.state, GameState::Won);
    assert!(!world.level.get_bit(0, 5, 3), "the one-shot plate opened the gate");
}

#[test]
fn trial_losing_route_reaches_lost() {
    let mut world = World::new(samples::trial()).unwrap();
    for &mv in LOSE_ROUTE {
        world.step_triggered(mv);
    }
    assert_eq!((world.px, world.py), (2, 3), "stepped onto the spike tile");
    assert_eq!(world.state, GameState::Lost);
}

#[test]
fn replay_reproduces_the_final_gamestate() {
    // Record the winning route, round-trip it through the `.rec` format, replay
    // it against a fresh world, and confirm the reproduced GameState is Won —
    // determinism covers the win/lose outcome too.
    let rec = Replay::new("levels/trial.bm", WIN_ROUTE).to_bytes();
    let decoded = Replay::from_bytes(&rec).expect("valid .rec");
    assert_eq!(decoded.moves, WIN_ROUTE, "moves survive 2-bit packing");

    let mut world = World::new(samples::trial()).unwrap();
    for &mv in &decoded.moves {
        world.step_triggered(mv);
    }
    assert_eq!(world.state, GameState::Won, "replay reproduces the Won outcome");
    assert_eq!(world.score, 3);

    // And the losing route replays to Lost, stably.
    let mut w2 = World::new(samples::trial()).unwrap();
    for &mv in LOSE_ROUTE {
        w2.step_triggered(mv);
    }
    assert_eq!(w2.state, GameState::Lost);
}
