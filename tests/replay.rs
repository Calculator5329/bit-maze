//! Phase 7 replay determinism: record a move sequence, serialize it to the
//! `.rec` format, decode it, replay it against a fresh world, and assert the
//! reproduced final state is byte-identical — player position, score, and every
//! wall the triggers mutated. This is the whole payoff of "determinism is law".

use bitmaze::replay::Replay;
use bitmaze::world::Move;
use bitmaze::{samples, World};

/// A run through the garden that collects items and trips the gate plate, so the
/// final state exercises all three things replay must reproduce: position,
/// score, and a wall mutation.
const RUN: &[Move] = &[
    Move::Right, // (1,1)->(2,1)
    Move::Right, // ->(3,1) collect item
    Move::Down,  // ->(3,2)
    Move::Left,  // ->(2,2) PLATE: gate at (5,3) opens
    Move::Down,  // ->(2,3)
    Move::Down,  // ->(2,4) collect item
    Move::Up,    // ->(2,3)
    Move::Right, // ->(3,3)
    Move::Right, // ->(4,3)
    Move::Right, // ->(5,3) through the opened gate
    Move::Right, // ->(6,3)
    Move::Right, // ->(7,3) collect item
];

/// Snapshot of everything a deterministic replay must reproduce.
#[derive(Debug, PartialEq, Eq)]
struct FinalState {
    px: u8,
    py: u8,
    score: u32,
    planes: Vec<Vec<u8>>,
}

fn play(moves: &[Move]) -> FinalState {
    let mut world = World::new(samples::garden()).expect("garden spawns");
    for &mv in moves {
        world.step_triggered(mv);
    }
    FinalState {
        px: world.px,
        py: world.py,
        score: world.score,
        planes: world.level.planes.clone(),
    }
}

#[test]
fn record_then_replay_reproduces_exact_final_state() {
    // The live run.
    let live = play(RUN);

    // The run must actually be interesting, or the test proves nothing.
    assert!(live.score >= 3, "the run collects at least three items");
    let gate_open = !World::new(samples::garden()).unwrap().level.get_bit(0, 5, 3);
    assert!(!gate_open, "sanity: the gate starts closed");

    // Round-trip the input log through the on-disk `.rec` format.
    let rec_bytes = Replay::new("levels/garden.bm", RUN).to_bytes();
    let decoded = Replay::from_bytes(&rec_bytes).expect("valid .rec");
    assert_eq!(decoded.moves, RUN, "moves survive the 2-bit packing");

    // Replay the decoded moves against a fresh world.
    let replayed = play(&decoded.moves);

    // Determinism: identical position, score, and wall/item plane mutations.
    assert_eq!(replayed, live, "replay reproduces the exact final world state");

    // And concretely: the gate the trigger opened is cleared in both.
    assert!(
        !replayed.planes[0]
            .iter()
            .eq(World::new(samples::garden()).unwrap().level.planes[0].iter()),
        "the walls plane was mutated by the trigger (gate opened), and replay reproduced it"
    );
}

#[test]
fn replay_is_stable_across_runs() {
    // Same input, same output, every time (no host randomness leaks in).
    assert_eq!(play(RUN), play(RUN));
}
