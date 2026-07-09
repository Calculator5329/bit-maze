//! `bitmaze check` — validate a level and produce a one-line-per-fact summary.

use crate::format::{plane_len, Level};

/// Produce a human-readable "OK" summary for a level that already parsed and
/// therefore satisfies every invariant.
pub fn summary(level: &Level) -> String {
    let plen = plane_len(level.width, level.height);
    let mut lines = vec![
        "OK: valid bit-maze v1 level".to_string(),
        format!("  dimensions : {}x{} ({} tiles)", level.width, level.height, level.tile_count()),
        format!("  flags      : {:#04x}", level.flags()),
        format!("  planes     : {} x {} bytes each", level.planes.len(), plen),
    ];
    match &level.triggers {
        Some(t) => lines.push(format!("  triggers   : present ({} bytes)", t.len())),
        None => lines.push("  triggers   : none".to_string()),
    }
    match &level.scripts {
        Some(s) => {
            let total: usize = s.iter().map(|sc| sc.bytes.len()).sum();
            lines.push(format!("  scripts    : {} script(s), {} bytecode byte(s)", s.len(), total));
        }
        None => lines.push("  scripts    : none".to_string()),
    }
    lines.push(format!("  total size : {} bytes", level.to_bytes().len()));
    lines.join("\n")
}
