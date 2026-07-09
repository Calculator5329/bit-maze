//! Pure software framebuffer rendering — the headless-testable seam.
//!
//! [`draw`] fills a flat `u32` pixel buffer (`0x00RR_GGBB`, the layout minifb
//! expects) from a [`World`], a [`Sprites`] set, and a [`Palette`]. It does
//! **no** windowing and touches no OS state, so it is fully unit-testable
//! without a display: build a small [`World`], call [`draw`] into a `Vec<u32>`,
//! and assert individual pixels are the ink or paper color for the role that
//! covers that tile. The minifb window shell (`crate::window`) is a thin layer
//! that only allocates the buffer, calls this, and blits.
//!
//! Each tile is rendered by **blitting a 1-bit sprite through the palette**
//! (Phase 6), replacing the old solid-color fill:
//! - a wall tile blits the wall sprite (ink → [`Palette::wall_ink`], paper →
//!   [`Palette::wall_paper`]);
//! - a floor tile blits the floor sprite (ink → `floor_ink`, paper →
//!   `floor_paper`);
//! - the player's tile blits the floor sprite first, then composites the player
//!   sprite over it (ink → `player_ink`, paper → **transparent**, so the floor
//!   shows through).
//!
//! Sprites are scaled to `tile_px` with nearest-neighbor sampling, so any
//! sprite size works at any tile size (an 8×8 sprite fills a 24-px tile, a 4-px
//! tile samples every other sprite pixel).

use crate::sprite::{Palette, Sprite, Sprites};
use crate::world::World;

/// Framebuffer width in pixels for a level of `width` tiles at `tile_px` each.
pub fn fb_width(world: &World, tile_px: usize) -> usize {
    world.level.width as usize * tile_px
}

/// Framebuffer height in pixels for a level of `height` tiles at `tile_px` each.
pub fn fb_height(world: &World, tile_px: usize) -> usize {
    world.level.height as usize * tile_px
}

/// Fill `fb` with the current world by blitting a 1-bit sprite per tile through
/// `palette`. `fb` must be at least `fb_w * fb_h` long; `fb_w`/`fb_h` are the
/// buffer's pixel dimensions and should be `tile_px` times the level's tile
/// dimensions (see [`fb_width`] / [`fb_height`]). Pure and deterministic — no
/// I/O, no windowing.
pub fn draw(
    world: &World,
    fb: &mut [u32],
    fb_w: usize,
    fb_h: usize,
    tile_px: usize,
    sprites: &Sprites,
    palette: &Palette,
) {
    let mut canvas = Canvas { fb, w: fb_w, h: fb_h, tile_px };
    for ty in 0..world.level.height {
        for tx in 0..world.level.width {
            let is_player = tx == world.px && ty == world.py;
            let is_wall = world.level.get_bit(0, tx, ty);
            if is_wall {
                canvas.blit(tx, ty, &sprites.wall, palette.wall_ink, Some(palette.wall_paper));
            } else {
                // Floor base under everything that isn't a wall.
                canvas.blit(tx, ty, &sprites.floor, palette.floor_ink, Some(palette.floor_paper));
                if is_player {
                    // Player composited over the floor: paper is transparent.
                    canvas.blit(tx, ty, &sprites.player, palette.player_ink, None);
                }
            }
        }
    }
}

/// Encode a rendered `0x00RR_GGBB` pixel buffer as a binary **P6 PPM** image
/// (Phase 7) — pure std, no image crate. `fb` must be at least `w * h` long. The
/// output is the exact bytes of a `.ppm` file: an ASCII header
/// (`P6\n<w> <h>\n255\n`) followed by `w * h` raw RGB triples, one byte each for
/// red, green, and blue. This is the headless, viewable proof that the *real*
/// renderer ([`draw`]) works: `bitmaze shot` renders a level and writes it here.
pub fn to_ppm(fb: &[u32], w: usize, h: usize) -> Vec<u8> {
    let header = format!("P6\n{w} {h}\n255\n");
    let mut out = Vec::with_capacity(header.len() + w * h * 3);
    out.extend_from_slice(header.as_bytes());
    for &px in fb.iter().take(w * h) {
        out.push(((px >> 16) & 0xFF) as u8); // R
        out.push(((px >> 8) & 0xFF) as u8); // G
        out.push((px & 0xFF) as u8); // B
    }
    out
}

/// A mutable view of the pixel buffer plus its geometry, so blitting is one
/// method call per tile (and clippy stays happy about argument counts).
struct Canvas<'a> {
    fb: &'a mut [u32],
    w: usize,
    h: usize,
    tile_px: usize,
}

impl Canvas<'_> {
    /// Blit `sprite` into the tile block at `(tx, ty)`, scaled to `tile_px` with
    /// nearest-neighbor sampling. Ink pixels are painted `ink`; paper pixels are
    /// painted `paper` when `Some`, or **skipped** (transparent) when `None`.
    /// Clipped to the buffer so a short `fb` can never panic.
    fn blit(&mut self, tx: u8, ty: u8, sprite: &Sprite, ink: u32, paper: Option<u32>) {
        let x0 = tx as usize * self.tile_px;
        let y0 = ty as usize * self.tile_px;
        for dy in 0..self.tile_px {
            let py = y0 + dy;
            if py >= self.h {
                break;
            }
            // Nearest-neighbor sample of the sprite row.
            let sy = (dy * sprite.height as usize / self.tile_px) as u8;
            let row = py * self.w;
            for dx in 0..self.tile_px {
                let px = x0 + dx;
                if px >= self.w {
                    break;
                }
                let sx = (dx * sprite.width as usize / self.tile_px) as u8;
                let color = if sprite.get(sx, sy) { Some(ink) } else { paper };
                if let Some(color) = color {
                    if let Some(slot) = self.fb.get_mut(row + px) {
                        *slot = color;
                    }
                }
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
    fn draw_blits_sprites_through_the_palette() {
        let world = sample_world();
        assert_eq!((world.px, world.py), (0, 1)); // sanity: spawn on first floor

        let sprites = Sprites::default();
        let pal = Palette::DEFAULT;
        // tile_px == sprite dimension so sampling is 1:1 (dx -> sx directly).
        let tile_px = 8;
        let fb_w = fb_width(&world, tile_px);
        let fb_h = fb_height(&world, tile_px);
        let mut fb = vec![0u32; fb_w * fb_h];

        draw(&world, &mut fb, fb_w, fb_h, tile_px, &sprites, &pal);

        // Sample the pixel at sprite-local (sx, sy) inside tile (tx, ty).
        let px_at = |tx: usize, ty: usize, sx: usize, sy: usize| {
            let px = tx * tile_px + sx;
            let py = ty * tile_px + sy;
            fb[py * fb_w + px]
        };

        // Wall tile (0,0): the wall sprite's top-left pixel is ink (row0=0xFF),
        // and its row-3 pixel is paper (mortar line, all 0).
        assert_eq!(px_at(0, 0, 0, 0), pal.wall_ink, "wall ink pixel");
        assert_eq!(px_at(0, 0, 0, 3), pal.wall_paper, "wall paper (mortar) pixel");

        // Plain floor tile (1,1): center dot is ink, corner is paper.
        // Floor sprite has ink at (3,3)/(4,3)/(3,4)/(4,4) only.
        assert_eq!(px_at(1, 1, 3, 3), pal.floor_ink, "floor dot ink pixel");
        assert_eq!(px_at(1, 1, 0, 0), pal.floor_paper, "floor paper pixel");

        // Player tile (0,1): the player sprite's ink shows player_ink; where the
        // player sprite is paper (transparent), the floor beneath shows through.
        // Player sprite (0,0) is paper -> floor paper shows. Player (2,0) is ink.
        assert!(Sprite::default_player().get(2, 0), "sanity: player (2,0) is ink");
        assert_eq!(px_at(0, 1, 2, 0), pal.player_ink, "player ink pixel");
        assert!(!Sprite::default_player().get(0, 0), "sanity: player (0,0) is paper");
        assert_eq!(px_at(0, 1, 0, 0), pal.floor_paper, "floor shows through player paper");
    }

    #[test]
    fn ppm_header_and_length_are_correct() {
        let world = sample_world();
        let sprites = Sprites::default();
        let pal = Palette::DEFAULT;
        let tile_px = 8;
        let fb_w = fb_width(&world, tile_px); // 3*8 = 24
        let fb_h = fb_height(&world, tile_px); // 2*8 = 16
        let mut fb = vec![0u32; fb_w * fb_h];
        draw(&world, &mut fb, fb_w, fb_h, tile_px, &sprites, &pal);

        let ppm = to_ppm(&fb, fb_w, fb_h);
        let header = format!("P6\n{fb_w} {fb_h}\n255\n");
        assert!(ppm.starts_with(header.as_bytes()), "P6 header present with dims");
        // Exactly header + w*h*3 body bytes.
        assert_eq!(ppm.len(), header.len() + fb_w * fb_h * 3);

        // The first body pixel is tile (0,0)'s top-left = wall ink; check its RGB
        // triple matches the palette color, proving real render bytes are written.
        let body = &ppm[header.len()..];
        let ink = pal.wall_ink;
        assert_eq!(body[0], ((ink >> 16) & 0xFF) as u8);
        assert_eq!(body[1], ((ink >> 8) & 0xFF) as u8);
        assert_eq!(body[2], (ink & 0xFF) as u8);
    }

    #[test]
    fn every_pixel_is_painted_and_scaling_covers_the_tile() {
        // A non-8 tile_px exercises nearest-neighbor scaling and full coverage.
        let world = sample_world();
        let sprites = Sprites::default();
        let pal = Palette::DEFAULT;
        let tile_px = 5;
        let fb_w = fb_width(&world, tile_px);
        let fb_h = fb_height(&world, tile_px);
        let mut fb = vec![0xDEAD_BEEFu32; fb_w * fb_h]; // poison to catch gaps

        draw(&world, &mut fb, fb_w, fb_h, tile_px, &sprites, &pal);

        // No poisoned pixel survives: every tile fully paints its block (the
        // player's transparent paper still lands on the floor drawn first).
        assert!(fb.iter().all(|&p| p != 0xDEAD_BEEF), "draw covers the whole buffer");
    }
}
