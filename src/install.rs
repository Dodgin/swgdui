//! `render` turns the settings into the set of files to write (path relative
//! to the game dir -> bytes).  `install` writes them, keeping whatever loose
//! file was there before as `<file>.pre-unitframes` and recording SHA-256
//! hashes in a manifest; `remove` puts the backups back (or deletes our files
//! when there was none) and refuses to touch a file changed since install.

use crate::settings::{Rgb, Settings, PROFESSIONS};
use crate::templates;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const BACKUP_EXT: &str = ".pre-unitframes";
pub const STYLES_FILE: &str = r"ui\ui_styles.inc";
pub const SWATCH_NAME: &str = "ui_class_swatch";
pub const SWATCH_FILE: &str = r"texture\ui_class_swatch.dds";
const SWATCH_CELL: u32 = 16;

/// Class swatch layout in ui_class_swatch.dds (4 columns of 16px cells).
pub const SWATCH_ORDER: [&str; 11] = [
    "mando", "officer", "jedi", "spy", "bh", "medic", "smuggler", "ent", "trader", "green", "default",
];

/// Role icon style name -> class swatch key.
fn role_map(style: &str) -> Option<&'static str> {
    Some(match style {
        "commando" | "advancedCommando" | "tank" | "advancedTank" => "mando",
        "officer" | "advancedOfficer" | "utility" | "advancedUtilities" => "officer",
        "force_sensitive" | "jediDefender" | "jediEnhancer" | "jediHealer" | "jediPowers"
        | "jediSaber" | "advancedJediSaber" => "jedi",
        "spy" | "crowdControl" | "advancedCrowdControl" => "spy",
        "bounty_hunter" | "advancedBountyHunter" | "ranged" | "advancedRanged" => "bh",
        "medic" | "advancedMedic" => "medic",
        "smuggler" | "advancedSmuggler" | "melee" | "advancedMelee" => "smuggler",
        "entertainer" | "advancedEntertainer" => "ent",
        "trader" | "artisan" | "advancedArtisan" => "trader",
        _ => return None,
    })
}

fn swatch_rect(key: &str) -> String {
    let i = SWATCH_ORDER.iter().position(|k| *k == key).unwrap_or(SWATCH_ORDER.len() - 1) as u32;
    let x = (i % 4) * SWATCH_CELL + 6;
    let y = (i / 4) * SWATCH_CELL + 6;
    format!("{x},{y},{},{}", x + 4, y + 4)
}

/// 64x64 uncompressed BGRA DDS, one 16px cell per swatch entry.
fn swatch_dds(colours: &BTreeMap<&str, Rgb>) -> Vec<u8> {
    let (w, h) = (64u32, 64u32);
    let mut pix = vec![0u8; (w * h * 4) as usize];
    for (i, key) in SWATCH_ORDER.iter().enumerate() {
        let c = colours[key];
        let cx = (i as u32 % 4) * SWATCH_CELL;
        let cy = (i as u32 / 4) * SWATCH_CELL;
        for y in cy..cy + SWATCH_CELL {
            for x in cx..cx + SWATCH_CELL {
                let o = ((y * w + x) * 4) as usize;
                pix[o] = c.2;
                pix[o + 1] = c.1;
                pix[o + 2] = c.0;
                pix[o + 3] = 255;
            }
        }
    }
    let mut out = Vec::with_capacity(128 + pix.len());
    out.extend_from_slice(b"DDS ");
    let hdr: [u32; 31] = [
        124, 0xA1007, h, w, w * h * 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        32, 0x41, 0, 32, 0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000, 0x401008, 0, 0, 0, 0,
    ];
    for v in hdr {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&pix);
    out
}

/// Everything the installer would write, keyed by path relative to the game dir.
pub struct Rendered {
    pub files: BTreeMap<String, Vec<u8>>,
    #[allow(dead_code)]
    pub health: Rgb,
}

/// Fill the templates.  `styles_src` is the stock ui/ui_styles.inc (needed only
/// when role colours are on); pass `None` to use the bundled stock copy.
pub fn render(s: &Settings, styles_src: Option<&str>) -> Result<Rendered, String> {
    let health = s.health()?;
    let target = s.target_health()?;
    let power = s.power()?;
    let (bx, by) = Settings::parse_loc(&s.buff_location)
        .ok_or_else(|| format!("location must be 'x,y', got '{}'", s.buff_location))?;
    let (dx, dy) = s.debuff_loc()?;
    let cell = s.buff_icon_size;

    let tokens: Vec<(&str, String)> = vec![
        ("@@HEALTH@@", health.hex()),
        ("@@HEALTH_BG@@", health.scale(s.backdrop_mul).hex()),
        ("@@TARGET_HEALTH@@", target.hex()),
        ("@@TARGET_HEALTH_BG@@", target.scale(s.backdrop_mul).hex()),
        ("@@POWER@@", power.hex()),
        ("@@POWER_BG@@", power.scale(s.backdrop_mul).hex()),
        ("@@CELL@@", cell.to_string()),
        ("@@BUFFS_LOC@@", format!("{bx},{by}")),
        ("@@DEBUFFS_LOC@@", format!("{dx},{dy}")),
        ("@@BUFFS_COLS@@", s.buff_columns.to_string()),
        ("@@BUFFS_ROWS@@", s.buff_rows.to_string()),
        ("@@BUFFS_GW@@", (s.buff_columns * cell).to_string()),
        ("@@BUFFS_GH@@", (s.buff_rows * cell).to_string()),
        ("@@DEBUFFS_COLS@@", s.debuff_columns.to_string()),
        ("@@DEBUFFS_ROWS@@", s.debuff_rows.to_string()),
        ("@@DEBUFFS_GW@@", (s.debuff_columns * cell).to_string()),
        ("@@DEBUFFS_GH@@", (s.debuff_rows * cell).to_string()),
    ];

    let unreplaced = Regex::new("@@[A-Z_]+@@").unwrap();
    let mut files = BTreeMap::new();
    for f in templates::PAGES {
        let mut text = templates::get(f, s.inline_buffs, s.role_colors)
            .ok_or_else(|| format!("template missing: {f}"))?
            .to_string();
        for (k, v) in &tokens {
            text = text.replace(k, v);
        }
        if let Some(m) = unreplaced.find(&text) {
            return Err(format!("unreplaced token in {f}: {}", m.as_str()));
        }
        files.insert(format!("ui\\{f}"), text.replace("\r\n", "\n").into_bytes());
    }

    if s.role_colors {
        let styles: &str = styles_src.unwrap_or(templates::STOCK_STYLES);
        files.insert(STYLES_FILE.to_string(), patch_role_styles(styles)?.into_bytes());

        let mut colours: BTreeMap<&str, Rgb> = BTreeMap::new();
        for key in SWATCH_ORDER {
            let c = if PROFESSIONS.iter().any(|(k, _, _)| *k == key) {
                s.class_color(key)
            } else {
                health
            };
            colours.insert(key, c);
        }
        files.insert(SWATCH_FILE.to_string(), swatch_dds(&colours));
    }
    Ok(Rendered { files, health })
}

/// Rewrite every ImageStyle inside `<Namespace Name='role'>` into a solid
/// swatch from ui_class_swatch.dds.
pub fn patch_role_styles(styles: &str) -> Result<String, String> {
    let ns = Regex::new(r"(?s)<Namespace\s+Name='role'\s*>.*?</Namespace>").unwrap();
    let blocks: Vec<_> = ns.find_iter(styles).collect();
    if blocks.len() != 1 {
        return Err(format!(
            "expected one <Namespace Name='role'> in ui_styles.inc, found {}",
            blocks.len()
        ));
    }
    let block = blocks[0].as_str();
    let nl = if styles.contains("\r\n") { "\r\n" } else { "\n" };
    let style_re = Regex::new(r"(?s)([ \t]*)<ImageStyle\s+(.*?)/>").unwrap();
    let name_re = Regex::new(r"Name='([^']*)'").unwrap();
    let mut n = 0;
    let patched = style_re.replace_all(block, |caps: &regex::Captures| {
        let ind = &caps[1];
        let nm = name_re.captures(&caps[2]).map(|c| c[1].to_string()).unwrap_or_default();
        let rect = swatch_rect(role_map(&nm).unwrap_or("default"));
        n += 1;
        format!(
            "{ind}<ImageStyle{nl}{ind}\tName='{nm}'{nl}{ind}\tSource='{SWATCH_NAME}'{nl}{ind}\tSourceRect='{rect}'{nl}{ind}/>"
        )
    });
    if n < 30 {
        return Err(format!("only {n} role styles found in ui_styles.inc; refusing"));
    }
    Ok(styles.replacen(block, &patched, 1))
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub game_dir: String,
    pub installed: String,
    pub files: BTreeMap<String, String>,
}

impl Manifest {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join("installed.json")
    }
    pub fn load(dir: &Path) -> Option<Manifest> {
        let s = std::fs::read_to_string(Self::path(dir)).ok()?;
        serde_json::from_str(&s).ok()
    }
}

pub fn sha256_file(p: &Path) -> std::io::Result<String> {
    let b = std::fs::read(p)?;
    Ok(format!("{:x}", Sha256::digest(b)))
}

fn now_iso() -> String {
    // seconds since the epoch is enough for a "when"; no chrono dependency
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

pub fn is_game_dir(dir: &str) -> bool {
    Path::new(dir).join("legends.cfg").is_file()
}

/// Write the rendered files into the game dir.  Returns log lines.
pub fn install(app_dir: &Path, s: &Settings, r: &Rendered) -> Result<Vec<String>, String> {
    if !is_game_dir(&s.game_dir) {
        return Err(format!("Not a SWG Legends install (no legends.cfg): {}", s.game_dir));
    }
    let root = Path::new(&s.game_dir);
    let prev = Manifest::load(app_dir);
    let mut log = Vec::new();
    let mut manifest = Manifest { game_dir: s.game_dir.clone(), installed: now_iso(), files: BTreeMap::new() };

    for (rel, bytes) in &r.files {
        let target = root.join(rel);
        let backup = PathBuf::from(format!("{}{BACKUP_EXT}", target.display()));
        let ours_before = prev.as_ref().map(|m| m.files.contains_key(rel)).unwrap_or(false);
        if target.exists() && !backup.exists() && !ours_before {
            std::fs::copy(&target, &backup).map_err(|e| format!("backup {rel}: {e}"))?;
            log.push(format!("kept previous {rel} as {rel}{BACKUP_EXT}"));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        std::fs::write(&target, bytes).map_err(|e| format!("write {rel}: {e}"))?;
        manifest.files.insert(rel.clone(), sha256_file(&target).map_err(|e| e.to_string())?);
        log.push(format!("wrote {}", target.display()));
    }

    // files from the previous install that this configuration no longer writes
    if let Some(p) = &prev {
        for rel in p.files.keys() {
            if manifest.files.contains_key(rel) {
                continue;
            }
            let target = root.join(rel);
            let backup = PathBuf::from(format!("{}{BACKUP_EXT}", target.display()));
            if backup.exists() {
                std::fs::rename(&backup, &target).map_err(|e| format!("restore {rel}: {e}"))?;
                log.push(format!("restored previous {rel}"));
            } else if target.exists() {
                std::fs::remove_file(&target).map_err(|e| format!("remove {rel}: {e}"))?;
                log.push(format!("removed {rel}"));
            }
        }
    }

    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(Manifest::path(app_dir), json).map_err(|e| format!("manifest: {e}"))?;
    s.save(app_dir).map_err(|e| format!("settings: {e}"))?;
    log.push("installed; relog (character select and back) to load".into());
    Ok(log)
}

/// Put the previous files back.  `force` overrides the changed-since-install check.
pub fn remove(app_dir: &Path, game_dir_override: Option<&str>, force: bool) -> Result<Vec<String>, String> {
    let manifest = Manifest::load(app_dir);
    let game_dir = game_dir_override
        .map(str::to_string)
        .or_else(|| manifest.as_ref().map(|m| m.game_dir.clone()))
        .unwrap_or_else(|| r"C:\SWG Legends".into());
    if !is_game_dir(&game_dir) {
        return Err(format!("Not a SWG Legends install (no legends.cfg): {game_dir}"));
    }
    let root = Path::new(&game_dir);
    let files: Vec<String> = match &manifest {
        Some(m) if !m.files.is_empty() => m.files.keys().cloned().collect(),
        _ => templates::PAGES.iter().map(|f| format!("ui\\{f}")).collect(),
    };
    let mut log = Vec::new();
    for rel in files {
        let target = root.join(&rel);
        let backup = PathBuf::from(format!("{}{BACKUP_EXT}", target.display()));
        if !target.exists() {
            if backup.exists() {
                std::fs::rename(&backup, &target).map_err(|e| format!("restore {rel}: {e}"))?;
                log.push(format!("restored {rel}"));
            }
            continue;
        }
        if !force {
            if let Some(expected) = manifest.as_ref().and_then(|m| m.files.get(&rel)) {
                let actual = sha256_file(&target).map_err(|e| e.to_string())?;
                if &actual != expected {
                    log.push(format!("{rel} differs from what we wrote - leaving it (use Force)"));
                    continue;
                }
            }
        }
        if backup.exists() {
            std::fs::rename(&backup, &target).map_err(|e| format!("restore {rel}: {e}"))?;
            log.push(format!("restored previous {rel}"));
        } else {
            std::fs::remove_file(&target).map_err(|e| format!("remove {rel}: {e}"))?;
            log.push(format!("removed {rel}"));
        }
    }
    let _ = std::fs::remove_file(Manifest::path(app_dir));
    log.push("done; relog to load the previous UI".into());
    Ok(log)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swatch_rects() {
        assert_eq!(swatch_rect("mando"), "6,6,10,10");
        assert_eq!(swatch_rect("bh"), "6,22,10,26");
        assert_eq!(swatch_rect("default"), "38,38,42,42");
        assert_eq!(swatch_rect("nope"), "38,38,42,42");
    }

    #[test]
    fn render_default_has_no_tokens() {
        let r = render(&Settings { role_colors: false, ..Settings::default() }, None).unwrap();
        assert_eq!(r.files.len(), 6);
        for (_, b) in &r.files {
            assert!(!String::from_utf8_lossy(b).contains("@@"));
        }
        assert!(String::from_utf8_lossy(&r.files["ui\\ui_ground_hud_targets_skinned.inc"]).contains("#C74040"));
    }

    #[test]
    fn role_style_patch() {
        let mut src = String::from("<Namespace Name='role'>\n");
        for i in 0..31 {
            src.push_str(&format!("\t<ImageStyle\n\t\tName='style{i}'\n\t\tSource='x'\n\t/>\n"));
        }
        src.push_str("\t<ImageStyle\n\t\tName='medic'\n\t\tSource='x'\n\t/>\n</Namespace>\n");
        let out = patch_role_styles(&src).unwrap();
        assert!(out.contains("Name='medic'\n\t\tSource='ui_class_swatch'\n\t\tSourceRect='22,22,26,26'"));
        assert!(out.contains("Name='style0'\n\t\tSource='ui_class_swatch'\n\t\tSourceRect='38,38,42,42'"));
    }
}
