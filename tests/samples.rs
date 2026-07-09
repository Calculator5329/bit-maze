//! Phase 7 sample content: the committed `levels/garden.bm` and
//! `levels/vault.bm` must byte-match their `src/samples.rs` builders (whose
//! trigger scripts come from the assembler) and pass the loader's invariants —
//! the same guarantee `bitmaze check` gives. Regenerate with
//! `bitmaze gen-levels levels` if a builder changes.

use bitmaze::{samples, Level, World};

#[test]
fn committed_sample_files_match_builders_and_validate() {
    for (name, level) in samples::all() {
        let path = format!("levels/{name}");
        let disk = std::fs::read(&path).unwrap_or_else(|_| panic!("{path} exists"));
        assert_eq!(disk, level.to_bytes(), "{path} is out of date (run gen-levels)");
        assert!(Level::from_bytes(&disk).is_ok(), "{path} must pass check");
    }
}

#[test]
fn garden_playthrough_collects_items_and_fires_the_gate() {
    use bitmaze::world::Move::*;
    let mut world = World::new(samples::garden()).unwrap();
    assert_eq!((world.px, world.py), (1, 1));
    assert!(world.level.get_bit(0, 5, 3), "gate starts closed");

    // Collect the first item and step on the plate.
    world.step_triggered(Right); // (2,1)
    world.step_triggered(Right); // (3,1) item
    assert_eq!(world.score, 1);
    world.step_triggered(Down); // (3,2)
    let plate = world.step_triggered(Left); // (2,2) plate
    assert!(plate.trigger.is_some(), "the plate fires the gate script");
    assert!(!world.level.get_bit(0, 5, 3), "gate is now open");
}

#[test]
fn vault_plate_uses_ram_and_opens_the_vault() {
    use bitmaze::world::Move::*;
    let mut world = World::new(samples::vault()).unwrap();
    assert!(world.level.get_bit(0, 5, 3), "vault starts closed");
    world.step_triggered(Right); // (2,1)... actually (2,1) then down to plate
    world.step_triggered(Down); // (2,2) plate at (2,2)
    // The plate at (2,2): we reached it via (1,1)->(2,1)->(2,2).
    assert!(!world.level.get_bit(0, 5, 3), "vault opened via LOAD/STORE + CLR_WALL script");
}
