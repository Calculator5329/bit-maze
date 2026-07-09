//! Pure software framebuffer rendering — the headless-testable seam.
//!
//! [`draw`] fills a flat `u32` pixel buffer (`0x00RR_GGBB`, the layout minifb
//! expects) from a [`World`]. It does **no** windowing and touches no OS state,
//! so it is fully unit-testable without a display: build a small [`World`], call
//! [`draw`] into a `Vec<u32>`, and assert individual pixels are the wall, floor,
//! or player color. The minifb window shell (`crate::window`) is a thin layer
//! that only allocates the buffer, calls this, and blits.
//!
//! Each tile is rendered as a solid `tile_px`×`tile_px` block. In Phase 6 the
//! per-tile solid-color fill here is what gets replaced by 1-bit sprite blits;
//! the window shell and this function's signature stay put.

use crate::world::World;

/// Wall tile color (`0x00RR_GGBB`): a dim slate blue.
pub const COLOR_WALL: u32 = 0x0025_2B48;
/// Floor tile color: near-black.
pub const COLOR_FLOOR: u32 = 0x000A_0A0F;
/// Player color: bright amber.
pub const COLOR_PLAYER: u32 = 0x00FF_B300;

/// Framebuffer width in pixels for a level of `width` tiles at `tile_px` each.
pub fn fb_width(world: &World, tile_px: usize) -> usize {
    world.level.width as usize * tile_px
}

/// Framebuffer height in pixels for a level of `height` tiles at `tile_px` each.
pub fn fb_height(world: &World, tile_px: usize) -> usize {
    world.level.height as usize * tile_px
}

/// Fill `fb` with the current world: walls in [`COLOR_WALL`], floor in
/// [`COLOR_FLOOR`], and the player's tile in [`COLOR_PLAYER`]. `fb` must be at
/// least `fb_w * fb_h` long; `fb_w`/`fb_h` are the buffer's pixel dimensions and
/// should be `tile_px` times the level's tile dimensions (see [`fb_width`] /
/// [`fb_height`]). Pure and deterministic — no I/O, no windowing.
pub fn draw(world: &World, fb: &mut [u32], fb_w: usize, fb_h: usize, tile_px: usize) {
    for ty in 0..world.level.height {
        for tx in 0..world.level.width {
            let color = if tx == world.px && ty == world.py {
                COLOR_PLAYER
            } else if world.level.get_bit(0, tx, ty) {
                COLOR_WALL
            } else {
                COLOR_FLOOR
            };
            fill_tile(fb, fb_w, fb_h, tx as usize, ty as usize, tile_px, color);
        }
    }
}

/// Paint one `tile_px`×`tile_px` block at tile `(tx, ty)`, clipped to the
/// buffer so a short `fb` can never panic.
fn fill_tile(
    fb: &mut [u32],
    fb_w: usize,
    fb_h: usize,
    tx: usize,
    ty: usize,
    tile_px: usize,
    color: u32,
) {
    let x0 = tx * tile_px;
    let y0 = ty * tile_px;
    for dy in 0..tile_px {
        let py = y0 + dy;
        if py >= fb_h {
            break;
        }
        let row = py * fb_w;
        for dx in 0..tile_px {
            let px = x0 + dx;
            if px >= fb_w {
                break;
            }
            if let Some(slot) = fb.get_mut(row + px) {
                *slot = color;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Level;

    /// A 3x2 level: top row all walls, bottom row all floor. Player spawns on
    /// the first floor tile in row-major order — that is (0,1).
    fn sample_world() -> World {
        let mut level = Level::blank(3, 2);
        for x in 0..3 {
            level.set_bit(0, x, 0, true); // top row = walls
        }
        World::new(level).expect("has floor tiles")
    }

    #[test]
    fn buffer_dimensions_match_tiles_times_tile_px() {
        let world = sample_world();
        let tile_px = 8;
        assert_eq!(fb_width(&world, tile_px), 3 * 8);
        assert_eq!(fb_height(&world, tile_px), 2 * 8);
    }

    #[test]
    fn draw_paints_walls_floor_and_player() {
        let world = sample_world();
        assert_eq!((world.px, world.py), (0, 1)); // sanity: spawn on first floor

        let tile_px = 4;
        let fb_w = fb_width(&world, tile_px);
        let fb_h = fb_height(&world, tile_px);
        let mut fb = vec![0u32; fb_w * fb_h];

        draw(&world, &mut fb, fb_w, fb_h, tile_px);

        // Helper: sample the center pixel of tile (tx, ty).
        let center = |tx: usize, ty: usize| {
            let px = tx * tile_px + tile_px / 2;
            let py = ty * tile_px + tile_px / 2;
            fb[py * fb_w + px]
        };

        // Top row is walls.
        assert_eq!(center(0, 0), COLOR_WALL);
        assert_eq!(center(1, 0), COLOR_WALL);
        assert_eq!(center(2, 0), COLOR_WALL);
        // Bottom-row floor tiles (not the player's).
        assert_eq!(center(1, 1), COLOR_FLOOR);
        assert_eq!(center(2, 1), COLOR_FLOOR);
        // Player tile at (0,1) wins over the floor underneath.
        assert_eq!(center(0, 1), COLOR_PLAYER);
    }

    #[test]
    fn every_pixel_of_a_tile_is_filled() {
        let world = sample_world();
        let tile_px = 5;
        let fb_w = fb_width(&world, tile_px);
        let fb_h = fb_height(&world, tile_px);
        let mut fb = vec![0xDEAD_BEEFu32; fb_w * fb_h]; // poison to catch gaps

        draw(&world, &mut fb, fb_w, fb_h, tile_px);

        // No poisoned pixel survives: draw covers the whole buffer.
        assert!(fb.iter().all(|&p| p != 0xDEAD_BEEF));

        // Every pixel inside the player's tile block (tile (0,1)) is the player
        // color.
        let (tile_x, tile_y) = (0usize, 1usize);
        for dy in 0..tile_px {
            for dx in 0..tile_px {
                let px = tile_x * tile_px + dx;
                let py = tile_y * tile_px + dy;
                assert_eq!(fb[py * fb_w + px], COLOR_PLAYER);
            }
        }
    }
}
