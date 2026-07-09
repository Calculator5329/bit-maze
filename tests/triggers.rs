//! Integration tests for Phase 4 trigger wiring: loading a real `.bm` level
//! with a trigger plane + script table, stepping onto the pressure plate, and
//! asserting the door script actually mutated the walls plane.

use bitmaze::world::{Move, StepResult, World};
use bitmaze::{Halt, Level};

/// Build the door level programmatically — the exact same bytes as the
/// committed `levels/door.bm` (an 8x8 room split by a wall at column 4, a plate
/// at (2,2) bound to script #1 = `push 4; push 3; clr_wall; halt`).
fn door_level() -> Level {
    // Walls: border + a full vertical divider at x=4 (rows 1..6).
    let walls = [0xFF, 0x89, 0x89, 0x89, 0x89, 0x89, 0x89, 0xFF];
    let mut triggers = vec![0u8; 64];
    triggers[2 * 8 + 2] = 1; // plate at (2,2) -> script id 1

    let mut bytes = vec![0x42, 0x4D, 0x01, 0x03, 0x08, 0x08, 0x01, 0x00];
    bytes.extend_from_slice(&walls);
    bytes.extend_from_slice(&triggers);
    // script table: count=1; id=1, len=6, bytes = 10 04 10 03 32 01
    bytes.extend_from_slice(&[0x01, 0x01, 0x06, 0x00, 0x10, 0x04, 0x10, 0x03, 0x32, 0x01]);

    Level::from_bytes(&bytes).expect("hand-authored door level is valid")
}

#[test]
fn committed_door_file_matches_and_is_valid() {
    // The repo's levels/door.bm must byte-match our programmatic build and pass
    // the loader's invariants (this is what `bitmaze check` runs).
    let disk = std::fs::read("levels/door.bm").expect("levels/door.bm exists");
    assert_eq!(disk, door_level().to_bytes(), "committed door.bm is out of date");
    assert!(Level::from_bytes(&disk).is_ok(), "door.bm must pass check");
}

#[test]
fn stepping_on_the_plate_opens_the_door() {
    let mut world = World::new(door_level()).unwrap();
    // Spawn is the first floor tile row-major: (1,1) in the left room.
    assert_eq!((world.px, world.py), (1, 1));
    // The door tile (4,3) starts as a wall.
    assert!(world.level.get_bit(0, 4, 3), "door starts closed (wall present)");

    // Walk onto the plate: right to (2,1), down to (2,2).
    let a = world.step_triggered(Move::Right);
    assert_eq!(a.result, StepResult::Moved);
    assert!(a.trigger.is_none(), "no trigger at (2,1)");

    let b = world.step_triggered(Move::Down);
    assert_eq!(b.result, StepResult::Moved);
    let t = b.trigger.expect("the plate at (2,2) fires a trigger");
    assert_eq!(t.script_id, 1);
    assert_eq!((t.x, t.y), (2, 2));
    assert_eq!(t.halt, Halt::Halt, "the door script ends on HALT");

    // The door bit at (4,3) is now cleared — the world mutated.
    assert!(!world.level.get_bit(0, 4, 3), "door is open (wall cleared)");
}

#[test]
fn the_opened_door_is_walkable_end_to_end() {
    let mut world = World::new(door_level()).unwrap();
    // Before opening, the divider blocks passage into the right room.
    // Trip the plate first.
    world.step_triggered(Move::Right); // (2,1)
    world.step_triggered(Move::Down); //  (2,2) -> door opens at (4,3)

    // Now route through the opened door: (2,2)->(2,3)->(3,3)->(4,3)->(5,3).
    for mv in [Move::Down, Move::Right, Move::Right, Move::Right] {
        assert_eq!(world.step_triggered(mv).result, StepResult::Moved);
    }
    assert_eq!((world.px, world.py), (5, 3), "player crossed into the right room");
}

#[test]
fn blocked_and_idle_steps_fire_no_trigger() {
    let mut world = World::new(door_level()).unwrap();
    // Up from (1,1) hits the top border wall -> Blocked, no trigger.
    let up = world.step_triggered(Move::Up);
    assert_eq!(up.result, StepResult::Blocked);
    assert!(up.trigger.is_none());

    let idle = world.step_triggered(Move::None);
    assert_eq!(idle.result, StepResult::Idle);
    assert!(idle.trigger.is_none());
}

#[test]
fn trigger_firing_is_stateless_and_idempotent() {
    let mut world = World::new(door_level()).unwrap();
    world.step_triggered(Move::Right); // (2,1)
    world.step_triggered(Move::Down); //  (2,2) fires, door opens
    assert!(!world.level.get_bit(0, 4, 3));

    // Leave and re-enter the plate: it fires again (v0 is stateless), and
    // clearing an already-clear wall is a harmless no-op.
    world.step_triggered(Move::Up); // back to (2,1)
    let again = world.step_triggered(Move::Down); // (2,2) again
    let t = again.trigger.expect("re-entering the plate fires again");
    assert_eq!(t.script_id, 1);
    assert!(!world.level.get_bit(0, 4, 3), "door stays open (idempotent)");
}
