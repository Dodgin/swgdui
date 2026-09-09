//! Recover the settings from the pages currently installed in the game folder
//! (whichever installer wrote them), so a fresh copy of the app previews what
//! the player actually sees instead of the built-in defaults.

use crate::settings::{Rgb, Settings, PROFESSIONS};
use crate::templates;
use std::path::Path;

/// Minimal reader for the UIBuilder XML subset our pages use: one element =
/// tag + attributes + children (or leaf text).
struct El {
    attrs: Vec<(String, String)>,
    children: Vec<El>,
}

impl El {
    fn get(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
    }
    fn name(&self) -> &str {
        self.get("Name").unwrap_or("")
    }
    fn child(&self, name: &str) -> Option<&El> {
        self.children.iter().find(|c| c.name() == name)
    }
    fn path(&self, p: &str) -> Option<&El> {
        p.split('.').try_fold(self, |cur, part| cur.child(part))
    }
    fn find(&self, name: &str) -> Option<&El> {
        for c in &self.children {
            if c.name() == name {
                return Some(c);
            }
            if let Some(r) = c.find(name) {
                return Some(r);
            }
        }
        None
    }
}

fn parse(src: &str) -> Vec<El> {
    let b = src.as_bytes();
    let mut pos = 0;
    fn skip_ws(b: &[u8], mut p: usize) -> usize {
        while p < b.len() && b[p].is_ascii_whitespace() {
            p += 1;
        }
        p
    }
    fn parse_children(b: &[u8], pos: &mut usize) -> Vec<El> {
        let mut out = Vec::new();
        loop {
            // next '<' that starts a tag
            let Some(lt) = b[*pos..].iter().position(|c| *c == b'<') else { return out };
            let lt = *pos + lt;
            if lt + 1 < b.len() && b[lt + 1] == b'/' {
                // closing tag of our parent
                let gt = b[lt..].iter().position(|c| *c == b'>').map(|g| lt + g + 1).unwrap_or(b.len());
                *pos = gt;
                return out;
            }
            let mut p = lt + 1;
            while p < b.len() && b[p].is_ascii_alphanumeric() {
                p += 1;
            }
            let mut attrs = Vec::new();
            loop {
                p = skip_ws(b, p);
                if p >= b.len() || b[p] == b'/' || b[p] == b'>' {
                    break;
                }
                let ks = p;
                while p < b.len() && (b[p].is_ascii_alphanumeric() || b[p] == b'_') {
                    p += 1;
                }
                let key = String::from_utf8_lossy(&b[ks..p]).into_owned();
                p = skip_ws(b, p);
                if p < b.len() && b[p] == b'=' {
                    p += 1;
                }
                p = skip_ws(b, p);
                if p < b.len() && b[p] == b'\'' {
                    p += 1;
                    let vs = p;
                    while p < b.len() && b[p] != b'\'' {
                        p += 1;
                    }
                    attrs.push((key, String::from_utf8_lossy(&b[vs..p]).into_owned()));
                    p += 1;
                } else {
                    break;
                }
            }
            if p < b.len() && b[p] == b'/' {
                *pos = p + 2;
                out.push(El { attrs, children: Vec::new() });
                continue;
            }
            *pos = p + 1; // past '>'
            // leaf text (<Text ...>abc</Text>) or children
            let children = parse_children(b, pos);
            out.push(El { attrs, children });
        }
    }
    let els = parse_children(b, &mut pos);
    els
}

/// The bar fill colour of pool page `h`/`a` under `status_path` (juice tint),
/// and the backdrop tint.
fn bar_colours(root: &El, status_path: &str, pool: &str) -> Option<(Rgb, Rgb)> {
    let bar = root.path(status_path)?.child(pool)?.child("Bar")?;
    let juice = Rgb::parse(bar.child("juice")?.get("BackgroundTint")?)?;
    let back = Rgb::parse(bar.child("back")?.get("BackgroundTint")?)?;
    Some((juice, back))
}

/// A buff window: position, and its grid as (columns, rows, cell).  The client
/// ignores the page's cell size and packs icons at the Options slider size, so
/// when `client_cell` is known the grid is reported as the cells that actually
/// fit in the window - what the player sees - rather than what the page says.
fn grid(root: &El, window: &str, client_cell: Option<u32>) -> Option<((i32, i32), u32, u32, u32)> {
    let w = if root.name() == window { root } else { root.find(window)? };
    let loc = Settings::parse_loc(w.get("Location")?)?;
    let (sw, sh) = Settings::parse_loc(w.get("Size")?)?;
    let vol = w.children.iter().find(|c| c.get("CellCount").is_some())?;
    let (cols, rows) = Settings::parse_loc(vol.get("CellCount")?)?;
    if cols <= 0 || rows <= 0 || sw <= 0 || sh <= 0 {
        return None;
    }
    match client_cell {
        Some(cell) if cell > 0 => {
            let c = ((sw as u32) / cell).max(1);
            let r = ((sh as u32) / cell).max(1);
            Some((loc, c, r, cell))
        }
        _ => Some((loc, cols as u32, rows as u32, (sw / cols) as u32)),
    }
}

pub struct Imported {
    pub applied: Vec<String>,
}

/// Read the installed player / target pages (and swatch) and update `s` in place.
pub fn import_installed(game_dir: &Path, s: &mut Settings, client_cell: Option<u32>) -> Result<Imported, String> {
    let ui = game_dir.join("ui");
    let player_src = std::fs::read_to_string(ui.join(templates::PLAYER))
        .map_err(|_| format!("no installed {} in {}", templates::PLAYER, ui.display()))?;
    let pages = parse(&player_src);
    let player = pages.iter().find(|p| p.name() == "MFDStatus").ok_or("installed player page has no MFDStatus")?;
    let mut applied = Vec::new();

    // buff windows
    if let Some((loc, cols, rows, cell)) = pages.iter().find_map(|p| grid(p, "PlayerBuffs", client_cell)) {
        s.inline_buffs = false;
        s.buff_location = format!("{},{}", loc.0, loc.1);
        s.buff_columns = cols;
        s.buff_rows = rows;
        s.buff_icon_size = cell;
        applied.push(format!("buffs {cols}x{rows} at {},{} (icon {cell})", loc.0, loc.1));
        if let Some((dloc, dcols, drows, _)) = pages.iter().find_map(|p| grid(p, "PlayerDebuffs", client_cell)) {
            s.debuff_location = format!("{},{}", dloc.0, dloc.1);
            s.debuff_columns = dcols;
            s.debuff_rows = drows;
            applied.push(format!("debuffs {dcols}x{drows} at {},{}", dloc.0, dloc.1));
        }
    } else {
        s.inline_buffs = true;
        applied.push("inline buff rows".into());
    }

    // role colours: the health juice carries a 'fill' image
    let role = player.path("status.h.Bar.juice").map(|j| j.child("fill").is_some()).unwrap_or(false);
    s.role_colors = role;
    applied.push(if role { "role colours on".into() } else { "role colours off".into() });

    // colours
    if let Some((power, back)) = bar_colours(player, "status", "a") {
        s.power_color = power.hex();
        let m = power.0.max(power.1).max(power.2);
        let mb = back.0.max(back.1).max(back.2);
        if m > 0 {
            s.backdrop_mul = ((mb as f64) / (m as f64) * 100.0).round() / 100.0;
        }
        applied.push(format!("action {} backdrop x{:.2}", power.hex(), s.backdrop_mul));
    }
    if let Some((health, _)) = bar_colours(player, "status", "h") {
        if !role {
            // a profession preset at the current brightness, else a custom colour
            let hit = PROFESSIONS.iter().find(|(k, _, _)| s.class_color(k) == health);
            match hit {
                Some((k, _, _)) => {
                    s.profession = if *k == "green" { String::new() } else { k.to_string() };
                    s.health_color.clear();
                }
                None => s.health_color = health.hex(),
            }
            applied.push(format!("health {}", health.hex()));
        }
    }
    if let Ok(t) = std::fs::read_to_string(ui.join(templates::TARGET)) {
        if let Some(root) = parse(&t).into_iter().find(|p| p.name() == "Target") {
            if let Some((c, _)) = bar_colours(&root, "status_npc", "h") {
                s.target_health_color = c.hex();
                applied.push(format!("target {}", c.hex()));
            }
        }
    }
    // class palette brightness from the jedi swatch (#00B3FF: blue channel is 255 * mul)
    if let Ok(dds) = std::fs::read(game_dir.join(crate::install::SWATCH_FILE)) {
        let (x, y) = (2 * 16 + 6, 6); // jedi is swatch index 2
        let o = 128 + (y * 64 + x) * 4;
        if let Some(px) = dds.get(o..o + 4) {
            let blue = px[0] as f64; // BGRA
            let mul = (blue / 255.0 * 100.0).round() / 100.0;
            if mul > 0.0 {
                s.class_color_mul = mul;
                applied.push(format!("class palette x{mul:.2}"));
            }
        }
    }
    Ok(Imported { applied })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_cells_at_client_size() {
        // page built for 24x6 at 22px (528x132); the client packs 36px cells -> 14x3 visible
        let mut want = Settings::default();
        want.role_colors = false;
        want.buff_columns = 24;
        want.buff_rows = 6;
        want.buff_icon_size = 22;
        let r = crate::install::render(&want, None).unwrap();
        let d = std::env::temp_dir().join(format!("dodgins-import-cells-{}", std::process::id()));
        for (rel, bytes) in &r.files {
            let p = d.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        }
        let mut got = Settings::default();
        import_installed(&d, &mut got, Some(36)).unwrap();
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!((got.buff_columns, got.buff_rows, got.buff_icon_size), (14, 3, 36));
    }

    #[test]
    fn roundtrip_from_rendered_pages() {
        let mut want = Settings::default();
        want.role_colors = false;
        want.profession = "medic".into();
        want.target_health_color = "#953030".into();
        want.power_color = "#10E0A0".into();
        want.buff_location = "1202,0".into();
        want.buff_columns = 24;
        want.buff_rows = 6;
        want.debuff_location = "10,10".into();
        want.debuff_columns = 12;
        want.debuff_rows = 2;
        want.buff_icon_size = 26;
        let r = crate::install::render(&want, None).unwrap();
        let d = std::env::temp_dir().join(format!("dodgins-import-test-{}", std::process::id()));
        for (rel, bytes) in &r.files {
            let p = d.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        }
        let mut got = Settings::default();
        got.class_color_mul = want.class_color_mul;
        import_installed(&d, &mut got, None).unwrap();
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(got.profession, "medic");
        assert_eq!(got.health_color, "");
        assert_eq!(got.target_health_color, "#953030");
        assert_eq!(got.power_color, "#10E0A0");
        assert_eq!(got.buff_location, "1202,0");
        assert_eq!((got.buff_columns, got.buff_rows, got.buff_icon_size), (24, 6, 26));
        assert_eq!((got.debuff_columns, got.debuff_rows), (12, 2));
        assert_eq!(got.debuff_location, "10,10");
        assert!(!got.role_colors && !got.inline_buffs);
        assert!((got.backdrop_mul - 0.25).abs() < 0.011);
    }
}
