//! `bitmaze play` — the minifb window front-end (Phase 3).
//!
//! This is the graphical I/O shell, the counterpart to the terminal shell in
//! `crate::play`. It is deliberately thin: it opens a window, translates minifb
//! key events into [`Move`]s, calls [`World::step`] (the unchanged Phase 2 game
//! core), fills a pixel buffer with the pure [`framebuffer::draw`], and blits it
//! each frame. All game logic stays in [`crate::world`]; the only thing here
//! that another platform would change is the windowing.
//!
//! Because opening a window needs a display, [`run`] is never called from tests
//! (window creation can fail or hang headless). It surfaces a clear error and
//! returns `Err` (mapped to a nonzero exit) when no display is available, rather
//! than hanging — and `bitmaze play --term` stays available as a headless path.

use crate::framebuffer;
use crate::world::{Move, World};
use minifb::{Key, KeyRepeat, Window, WindowOptions};

/// Pixel size of one tile in the graphical window.
pub const TILE_PX: usize = 24;

/// Frame pacing target, in frames per second.
const TARGET_FPS: usize = 60;

/// Map a minifb key to a [`Move`]. Both WASD and the arrow keys move; every
/// other key (including quit keys, handled separately) maps to `None`. Pure —
/// no window needed, so it is unit-testable.
fn key_to_move(key: Key) -> Option<Move> {
    match key {
        Key::W | Key::Up => Some(Move::Up),
        Key::S | Key::Down => Some(Move::Down),
        Key::A | Key::Left => Some(Move::Left),
        Key::D | Key::Right => Some(Move::Right),
        _ => None,
    }
}

/// Open a window and run the interactive graphical loop over `world`, rendering
/// each tile at `tile_px` pixels. Movement: `W/A/S/D` or the arrow keys.
/// Quit: `Esc` or `Q`, or closing the window.
///
/// Returns `Err(String)` (never hangs) if a window cannot be created — e.g. no
/// display in a headless environment — so the caller can print it and exit
/// nonzero. Never call this from tests.
pub fn run(world: &mut World, tile_px: usize) -> Result<(), String> {
    let fb_w = framebuffer::fb_width(world, tile_px);
    let fb_h = framebuffer::fb_height(world, tile_px);

    let mut window = Window::new("bit-maze", fb_w, fb_h, WindowOptions::default()).map_err(|e| {
        format!(
            "cannot open a window ({e}). No display available? \
             Run headless with `bitmaze play --term <level.bm>` instead."
        )
    })?;

    // Cap the frame rate so we don't busy-spin the CPU.
    window.set_target_fps(TARGET_FPS);

    let mut fb = vec![framebuffer::COLOR_FLOOR; fb_w * fb_h];

    while window.is_open() && !window.is_key_down(Key::Escape) && !window.is_key_down(Key::Q) {
        // Edge-triggered: one step per physical key press (no auto-repeat), so
        // discrete tile movement matches the terminal loop's feel.
        for key in window.get_keys_pressed(KeyRepeat::No) {
            if let Some(mv) = key_to_move(key) {
                // Fire triggers too, so stepping on a plate mutates the walls
                // plane; the next `draw` already reflects it (it reads live).
                world.step_triggered(mv);
            }
        }

        framebuffer::draw(world, &mut fb, fb_w, fb_h, tile_px);
        window
            .update_with_buffer(&fb, fb_w, fb_h)
            .map_err(|e| format!("window update failed: {e}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasd_and_arrows_both_map_to_moves() {
        assert_eq!(key_to_move(Key::W), Some(Move::Up));
        assert_eq!(key_to_move(Key::Up), Some(Move::Up));
        assert_eq!(key_to_move(Key::S), Some(Move::Down));
        assert_eq!(key_to_move(Key::Down), Some(Move::Down));
        assert_eq!(key_to_move(Key::A), Some(Move::Left));
        assert_eq!(key_to_move(Key::Left), Some(Move::Left));
        assert_eq!(key_to_move(Key::D), Some(Move::Right));
        assert_eq!(key_to_move(Key::Right), Some(Move::Right));
    }

    #[test]
    fn non_movement_keys_map_to_none() {
        assert_eq!(key_to_move(Key::Q), None);
        assert_eq!(key_to_move(Key::Escape), None);
        assert_eq!(key_to_move(Key::Space), None);
    }
}
