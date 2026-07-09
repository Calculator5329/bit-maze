//! Integration tests for the Phase 2 world: spawn rule, movement/collision,
//! and a full-render snapshot. All deterministic — no I/O, no randomness.

use bitmaze::world::{Move, SpawnError, StepResult, World};
use bitmaze::Level;

/// The ROADMAP `xxd` contract: an 8x8 walls-only maze.
///
/// ```text
/// ########  y0
/// #......#  y1
/// #.####.#  y2
/// #.#..#.#  y3
/// #.#.####  y4
/// #......#  y5
/// #####.##  y6
/// ########  y7
/// ```
const FIRST_BM: [u8; 16] = [
    0x42, 0x4d, 0x01, 0x00, 0x08, 0x08, 0x01, 0x00, // header
    0xff, 0x81, 0xbd, 0xa5, 0xaf, 0x81, 0xfb, 0xff, // rows
];

fn first_world() -> World {
    World::new(Level::from_bytes(&FIRST_BM).unwrap()).unwrap()
}

/// Feed a key string (`w`/`a`/`s`/`d`) through the pure step function.
fn drive(world: &mut World, keys: &str) {
    for c in keys.chars() {
        if let Some(mv) = Move::from_key(c) {
            world.step(mv);
        }
    }
}

#[test]
fn spawns_on_first_floor_tile_row_major() {
    // Row 0 of first.bm is all wall; the first floor tile scanning row-major is
    // (1,1).
    let world = first_world();
    assert_eq!((world.px, world.py), (1, 1));
    assert!(!world.level.get_bit(0, world.px, world.py), "spawn must be a floor tile");
}

#[test]
fn all_walls_level_fails_to_spawn() {
    let mut level = Level::blank(2, 2);
    for y in 0..2 {
        for x in 0..2 {
            level.set_bit(0, x, y, true);
        }
    }
    assert_eq!(World::new(level).unwrap_err(), SpawnError::AllWalls);
}

#[test]
fn scripted_sequence_final_position_and_render_snapshot() {
    let mut world = first_world();
    // From (1,1): d->(2,1) moved, d->(3,1) moved, s into wall (3,2) blocked,
    // s blocked again. Final = (3,1).
    drive(&mut world, "ddss");
    assert_eq!((world.px, world.py), (3, 1));

    let expected = "\
########
#..@...#
#.####.#
#.#..#.#
#.#.####
#......#
#####.##
########
";
    assert_eq!(world.render(), expected);
}

#[test]
fn moving_into_a_wall_is_a_noop() {
    // 3x1 level: floor, wall, floor. Spawn on (0,0), step right into the wall.
    let mut level = Level::blank(3, 1);
    level.set_bit(0, 1, 0, true);
    let mut world = World::new(level).unwrap();
    assert_eq!((world.px, world.py), (0, 0));

    assert_eq!(world.step(Move::Right), StepResult::Blocked);
    assert_eq!((world.px, world.py), (0, 0), "wall must block, player stays");
}

#[test]
fn moving_off_the_edge_is_a_noop() {
    // 1x1 all-floor level: spawn at (0,0); every direction is off the edge.
    let mut world = World::new(Level::blank(1, 1)).unwrap();
    assert_eq!((world.px, world.py), (0, 0));

    for mv in [Move::Up, Move::Down, Move::Left, Move::Right] {
        assert_eq!(world.step(mv), StepResult::Blocked);
        assert_eq!((world.px, world.py), (0, 0), "edge must block, player stays");
    }
}

#[test]
fn move_none_is_idle() {
    let mut world = first_world();
    let before = (world.px, world.py);
    assert_eq!(world.step(Move::None), StepResult::Idle);
    assert_eq!((world.px, world.py), before);
}

#[test]
fn a_successful_move_reports_moved() {
    let mut world = first_world();
    assert_eq!(world.step(Move::Right), StepResult::Moved);
    assert_eq!((world.px, world.py), (2, 1));
}
