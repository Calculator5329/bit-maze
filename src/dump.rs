//! `bitmaze dump` — the project's debugger. Renders every plane as ASCII art
//! and the trigger plane as a hex grid.

use crate::format::Level;
use std::fmt::Write as _;

/// Render a full human-readable dump of a level to a `String`.
pub fn render(level: &Level) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "level: {}x{} tiles, {} plane(s), flags={:#04x}",
        level.width,
        level.height,
        level.planes.len(),
        level.flags()
    );

    for (i, _plane) in level.planes.iter().enumerate() {
        let label = match i {
            0 => "WALLS",
            1 => "ITEMS",
            _ => "aux",
        };
        let _ = writeln!(out, "\nplane {i} ({label})  '#' = set, '.' = clear:");
        for y in 0..level.height {
            for x in 0..level.width {
                out.push(if level.get_bit(i, x, y) { '#' } else { '.' });
            }
            out.push('\n');
        }
    }

    if let Some(triggers) = &level.triggers {
        let _ = writeln!(out, "\ntrigger plane (hex script id per tile, 00 = none):");
        for y in 0..level.height {
            for x in 0..level.width {
                let idx = y as usize * level.width as usize + x as usize;
                if x > 0 {
                    out.push(' ');
                }
                let _ = write!(out, "{:02x}", triggers[idx]);
            }
            out.push('\n');
        }
    }

    if let Some(scripts) = &level.scripts {
        let _ = writeln!(out, "\nscript table ({} script(s)):", scripts.len());
        for s in scripts {
            let hex: Vec<String> = s.bytes.iter().map(|b| format!("{b:02x}")).collect();
            let _ = writeln!(out, "  id {:3}: {} byte(s): {}", s.id, s.bytes.len(), hex.join(" "));
        }
    }

    out
}
