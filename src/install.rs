//! `render` turns the settings into the set of files to write (path relative
//! to the game dir -> bytes).  `install` writes them, keeping whatever loose
//! file was there before as `<file>.pre-unitframes` and recording SHA-256
//! hashes in a manifest; `remove` puts the backups back (or deletes our files
//! when there was none) and refuses to touch a file changed since install.

use crate::settings::{Rgb, Settings, PROFESSIONS};
use crate::{keymap, templates};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const BACKUP_EXT: &str = ".pre-unitframes";
pub const STOCK_ICON_MARGIN: &str = "1,2,2,2";
pub const STYLES_FILE: &str = r"ui\ui_styles.inc";
pub const HUD_FILE: &str = r"ui\ui_ground_hud.inc";
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
    /// the keymap the action bar labels came from, if one was found
    pub keymap: Option<PathBuf>,
    /// why there is none (labels left blank)
    pub keymap_err: Option<String>,
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
    let ab_back = s.action_bar_backdrop()?;
    let ab_key = s.action_bar_key()?;
    let km = if s.action_bars { Some(keymap::for_settings(Path::new(&s.game_dir), &s.keymap_file)) } else { None };
    let (keymap_path, km, keymap_err) = match km {
        Some(Ok((p, k))) => (Some(p), k, None),
        Some(Err(e)) => (None, keymap::Keymap::default(), Some(e)),
        None => (None, keymap::Keymap::default(), None),
    };

    let mut tokens: Vec<(&str, String)> = vec![
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
        ("@@AB_BACK@@", ab_back.hex()),
        ("@@AB_BACK_OPACITY@@", format!("{:.2}", if s.action_bar_backdrop { s.action_bar_backdrop_opacity.clamp(0.0, 1.0) } else { 0.0 })),
        ("@@AB_KEY_COLOR@@", ab_key.hex()),
        ("@@AB_QUEUE_VISIBLE@@", if s.action_bar_queue_bar { "true".into() } else { "false".into() }),
        // the stock value: the client applies this to every icon it lays out (a
        // negative margin moved icons everywhere without scaling them), so it stays
        ("@@AB_ICON_MARGIN@@", STOCK_ICON_MARGIN.into()),
    ];
    let key_tokens: Vec<(String, String)> = (12..24)
        .map(|i| (format!("@@AB_KEY_{i}@@"), km.slot(i)))
        .chain((0..9).map(|i| (format!("@@AB_PET_KEY_{i}@@"), km.pet_slot(i))))
        .collect();
    for (k, v) in &key_tokens {
        tokens.push((k.as_str(), v.clone()));
    }

    let unreplaced = Regex::new("@@[A-Z_0-9]+@@").unwrap();
    let mut files = BTreeMap::new();
    let mut pages: Vec<(&str, &str)> = Vec::new();
    for f in templates::PAGES {
        pages.push((f, templates::get(f, s.inline_buffs, s.role_colors).ok_or_else(|| format!("template missing: {f}"))?));
    }
    if s.action_bars {
        pages.push((templates::TOOLBAR, templates::toolbar(s.action_bar_keybinds_inside)));
        pages.push((templates::SIDE_TOOLBAR, templates::side_toolbar(s.action_bar_keybinds_inside)));
    }
    for (f, tpl) in pages {
        let mut text = tpl.to_string();
        for (k, v) in &tokens {
            text = text.replace(k, v);
        }
        if let Some(m) = unreplaced.find(&text) {
            return Err(format!("unreplaced token in {f}: {}", m.as_str()));
        }
        files.insert(format!("ui\\{f}"), text.replace("\r\n", "\n").into_bytes());
    }

    // The pet page is only loaded if ui_ground_hud.inc includes it; stock
    // defines the pet window inline instead.  Patch whatever hud the client is
    // using (a loose one from another mod, else the bundled stock copy).  A hud
    // that already includes it (Clean UI, or ours from a previous install) is
    // written back unchanged so it stays in the manifest.
    let loose_hud = std::fs::read_to_string(Path::new(&s.game_dir).join(HUD_FILE)).ok();
    let hud = patch_hud_pet(loose_hud.as_deref().unwrap_or(templates::STOCK_HUD))?;
    files.insert(HUD_FILE.to_string(), hud.into_bytes());

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
    Ok(Rendered { files, health, keymap: keymap_path, keymap_err })
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

/// Make ui_ground_hud.inc load the pet page from ui_ground_hud_pet.inc: the
/// inline `<Page Name='pet'>` block (a direct child of GroundHUD in stock) is
/// replaced by `<include>ui_ground_hud_pet.inc</include>` at the same
/// indentation.  A hud that already has the include comes back unchanged.
pub fn patch_hud_pet(hud: &str) -> Result<String, String> {
    let include_re = Regex::new(r"(?i)<include>\s*ui_ground_hud_pet\.inc\s*</include>").unwrap();
    if include_re.is_match(hud) {
        return Ok(hud.to_string());
    }
    // every tag; Page opens/closes drive the depth count
    let tag_re = Regex::new(r"(?s)<(/?)([A-Za-z]+)(\s[^<>]*?)?(/?)>").unwrap();
    let name_re = Regex::new(r"(?i)\sName='pet'").unwrap();
    let mut start = None;
    let mut depth = 0usize;
    for m in tag_re.captures_iter(hud) {
        let closing = &m[1] == "/";
        let self_closing = &m[4] == "/";
        let attrs = m.get(3).map_or("", |a| a.as_str());
        if &m[2] != "Page" {
            continue;
        }
        let whole = m.get(0).unwrap();
        match start {
            None => {
                if !closing && !self_closing && name_re.is_match(attrs) {
                    start = Some(whole.start());
                    depth = 1;
                }
            }
            Some(begin) => {
                if closing {
                    depth -= 1;
                    if depth == 0 {
                        let line_start = hud[..begin].rfind('\n').map_or(0, |i| i + 1);
                        if !hud[line_start..begin].trim().is_empty() {
                            return Err("pet page in ui_ground_hud.inc does not start its own line".into());
                        }
                        return Ok(format!(
                            "{}<include>ui_ground_hud_pet.inc</include>{}",
                            &hud[..begin],
                            &hud[whole.end()..]
                        ));
                    }
                } else if !self_closing {
                    depth += 1;
                }
            }
        }
    }
    Err("no <Page Name='pet'> block in ui_ground_hud.inc; the game's hud may have changed".into())
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

    // The hud references ui_ground_hud_pet.inc, so it goes last: if a write
    // fails partway through, the client is never left with a hud including a
    // page that is not on disk (that pair does not boot).
    let write_order = r
        .files
        .iter()
        .filter(|(rel, _)| rel.as_str() != HUD_FILE)
        .chain(r.files.iter().filter(|(rel, _)| rel.as_str() == HUD_FILE));
    for (rel, bytes) in write_order {
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

/// Every path the installer can write, whatever the settings.  The hud comes
/// first: it must stop referencing the pet page before that page is removed.
pub fn all_installable() -> Vec<String> {
    let mut v = vec![HUD_FILE.to_string()];
    v.extend(
        templates::PAGES
            .iter()
            .chain([&templates::TOOLBAR, &templates::SIDE_TOOLBAR])
            .map(|f| format!("ui\\{f}")),
    );
    v.push(STYLES_FILE.to_string());
    v.push(SWATCH_FILE.to_string());
    v
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
    // The manifest records the last install this app dir made.  It is not the
    // whole story: an install from another copy of the exe, or one that failed
    // before rewriting the manifest, leaves files it does not list, and those
    // used to survive Remove and keep the UI patched.  Force sweeps everything
    // the installer can write; otherwise take the manifest plus the hud, which
    // must never outlive the pet page it includes.
    let mut files: Vec<String> = match &manifest {
        Some(m) if !m.files.is_empty() => m.files.keys().cloned().collect(),
        _ => all_installable(),
    };
    for extra in if force { all_installable() } else { vec![HUD_FILE.to_string()] } {
        if !files.iter().any(|f| *f == extra) {
            files.insert(0, extra);
        }
    }
    files.sort_by_key(|f| (f != HUD_FILE, f.clone()));
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
            match manifest.as_ref().and_then(|m| m.files.get(&rel)) {
                Some(expected) => {
                    let actual = sha256_file(&target).map_err(|e| e.to_string())?;
                    if &actual != expected {
                        log.push(format!("{rel} differs from what we wrote - leaving it (use Force)"));
                        continue;
                    }
                }
                // no manifest: only touch a hud that is exactly our patched stock one,
                // never another mod's (which we would have backed up, had we installed over it)
                None if rel == HUD_FILE && !backup.exists() => {
                    let ours = patch_hud_pet(templates::STOCK_HUD).map(String::into_bytes).unwrap_or_default();
                    if std::fs::read(&target).map(|b| b != ours).unwrap_or(true) {
                        log.push(format!("{rel} is not ours - leaving it (use Force)"));
                        continue;
                    }
                }
                None => {}
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
    // Last line of defence.  Whatever happened above, the client will not start
    // if ui_ground_hud.inc includes a pet page that is not there (stock ships no
    // such file, it defines the pet window inline), so never leave that pair.
    let hud = root.join(HUD_FILE);
    let pet = root.join(format!(r"ui\{}", templates::PET));
    if hud.is_file()
        && !pet.is_file()
        && std::fs::read_to_string(&hud).map(|t| t.contains(templates::PET)).unwrap_or(false)
    {
        let backup = PathBuf::from(format!("{}{BACKUP_EXT}", hud.display()));
        if backup.exists() {
            std::fs::rename(&backup, &hud).map_err(|e| format!("restore {HUD_FILE}: {e}"))?;
            log.push(format!("restored previous {HUD_FILE} (it referenced the removed pet page)"));
        } else {
            std::fs::remove_file(&hud).map_err(|e| format!("remove {HUD_FILE}: {e}"))?;
            log.push(format!("removed {HUD_FILE}: it referenced the removed pet page and would not have loaded"));
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
        assert_eq!(r.files.len(), 9);
        for (_, b) in &r.files {
            assert!(!String::from_utf8_lossy(b).contains("@@"));
        }
        assert!(String::from_utf8_lossy(&r.files["ui\\ui_ground_hud_targets_skinned.inc"]).contains("#C74040"));
        let tb = String::from_utf8_lossy(&r.files["ui\\ui_ground_hud_toolbar_skinned.inc"]);
        assert!(tb.contains("BackgroundTint='#0F0F0F'"));
        assert!(tb.contains("BackgroundOpacity='0.00'"), "backdrop off by default");
        assert!(tb.contains("TextAlignmentVertical='Top'"));
        assert!(tb.contains("IconMargin='1,2,2,2'"));
        assert!(tb.contains("Visible='false'"));
        assert!(tb.contains("Name='volumeBorders'"));
        assert!(tb.contains(">Q</Text>") || tb.contains("></Text>"), "static labels are element bodies");
        let side = String::from_utf8_lossy(&r.files["ui\\ui_ground_hud_side_toolbar_skinned.inc"]);
        assert_eq!(side.matches("Name='volumeBorders'").count(), 4);
        assert!(side.contains("sideIconMargin='1,2,2,2'"));
    }

    #[test]
    fn backdrop_and_queue_bar_tokens() {
        let s = Settings { role_colors: false, action_bar_backdrop: true, action_bar_queue_bar: true, ..Settings::default() };
        let r = render(&s, None).unwrap();
        let tb = String::from_utf8_lossy(&r.files["ui\\ui_ground_hud_toolbar_skinned.inc"]);
        assert!(tb.contains("BackgroundOpacity='0.80'"));
        assert_eq!(tb.matches("Visible='true'").count(), 2, "throttlePage in both pages");
    }

    /// a throwaway game dir (legends.cfg + ui/) plus an app dir for the manifest
    fn temp_dirs(tag: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("dodgins-ui-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (game, app) = (base.join("game"), base.join("app"));
        std::fs::create_dir_all(game.join("ui")).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(game.join("legends.cfg"), b"").unwrap();
        (game, app)
    }

    fn installed(tag: &str) -> (PathBuf, PathBuf, Settings) {
        let (game, app) = temp_dirs(tag);
        let s = Settings { game_dir: game.display().to_string(), ..Settings::default() };
        let r = render(&s, None).unwrap();
        install(&app, &s, &r).unwrap();
        (game, app, s)
    }

    /// A manifest from before the installer patched the hud must not leave the
    /// hud behind once the pet page it includes is removed: that will not boot.
    #[test]
    fn remove_never_strands_the_hud() {
        let (game, app, _) = installed("strand");
        let hud = game.join(HUD_FILE);
        let pet = game.join(format!(r"ui\{}", templates::PET));
        assert!(hud.is_file() && pet.is_file());
        assert!(std::fs::read_to_string(&hud).unwrap().contains(templates::PET));

        // rewrite the manifest the way an older build left it: no hud entry
        let mut m = Manifest::load(&app).unwrap();
        m.files.remove(HUD_FILE);
        std::fs::write(Manifest::path(&app), serde_json::to_string(&m).unwrap()).unwrap();

        remove(&app, None, false).unwrap();
        assert!(!pet.is_file(), "pet page removed");
        assert!(!hud.is_file(), "hud must not survive the pet page it includes");
        let _ = std::fs::remove_dir_all(game.parent().unwrap());
    }

    /// Force removes what this app dir never recorded: files from an install
    /// made by another copy of the exe used to survive and stay patched.
    #[test]
    fn force_remove_sweeps_unrecorded_files() {
        let (game, app, _) = installed("sweep");
        std::fs::remove_file(Manifest::path(&app)).unwrap();
        remove(&app, Some(&game.display().to_string()), true).unwrap();
        for rel in all_installable() {
            assert!(!game.join(&rel).exists(), "{rel} left behind");
        }
        let _ = std::fs::remove_dir_all(game.parent().unwrap());
    }

    /// A mod that already ships the hud include and its own pet page (Clean
    /// UI's, say): both are backed up on install and both put back on remove,
    /// so the include still resolves afterwards.
    #[test]
    fn remove_restores_a_third_party_hud() {
        let (game, app) = temp_dirs("restore");
        let theirs = patch_hud_pet(templates::STOCK_HUD).unwrap() + "\n<!-- theirs -->";
        let their_pet = "<!-- their pet page -->";
        std::fs::write(game.join(HUD_FILE), &theirs).unwrap();
        std::fs::write(game.join(format!(r"ui\{}", templates::PET)), their_pet).unwrap();
        let s = Settings { game_dir: game.display().to_string(), ..Settings::default() };
        let r = render(&s, None).unwrap();
        install(&app, &s, &r).unwrap();
        remove(&app, None, true).unwrap();
        assert_eq!(std::fs::read_to_string(game.join(HUD_FILE)).unwrap(), theirs, "their hud is back");
        assert_eq!(
            std::fs::read_to_string(game.join(format!(r"ui\{}", templates::PET))).unwrap(),
            their_pet,
            "their pet page is back, so the include still resolves"
        );
        let _ = std::fs::remove_dir_all(game.parent().unwrap());
    }

    #[test]
    fn hud_pet_include() {
        let stock = templates::STOCK_HUD;
        assert!(!stock.contains("ui_ground_hud_pet.inc"));
        assert_eq!(stock.matches("Name='pet'").count(), 1);
        let patched = patch_hud_pet(stock).unwrap();
        assert!(!patched.contains("Name='pet'"), "inline pet page removed");
        assert_eq!(patched.matches("<include>ui_ground_hud_pet.inc</include>").count(), 1);
        // the include takes the block's place: GroundHUD's first child, same indentation
        let idx = patched.find("<include>ui_ground_hud_pet.inc</include>").unwrap();
        assert!(patched[..idx].ends_with("\t\t"), "keeps the block's indentation");
        assert!(patched[..idx].contains("Name='GroundHUD'"));
        assert!(patched[idx..].contains("<include>ui_ground_hud_incap.inc</include>"));
        let before = &stock[..stock.find("\t\t<Page").unwrap()];
        assert!(patched.starts_with(before), "nothing before the block changed");
        // everything from the next sibling on is untouched
        let tail = &stock[stock.find("\t\t<Page\r\n\t\t\tAbsorbsInput='false'").unwrap()..];
        assert!(patched.ends_with(tail), "nothing after the block changed");
        // idempotent, and a hud that already includes the page is left alone
        assert_eq!(patch_hud_pet(&patched).unwrap(), patched);
        assert!(patch_hud_pet("<Page Name='GroundHUD'>\n</Page>").is_err());
        // rendered output carries the patched hud (no loose hud in a game dir that does not exist)
        let s = Settings { role_colors: false, game_dir: r"C:
o-such-dir".into(), ..Settings::default() };
        let r = render(&s, None).unwrap();
        assert_eq!(r.files[HUD_FILE], patched.into_bytes());
    }

    #[test]
    fn render_without_action_bars_or_with_key_row() {
        let base = Settings { role_colors: false, ..Settings::default() };
        let r = render(&Settings { action_bars: false, ..base.clone() }, None).unwrap();
        assert_eq!(r.files.len(), 7);
        let r = render(&Settings { action_bar_keybinds_inside: false, ..base }, None).unwrap();
        let tb = String::from_utf8_lossy(&r.files["ui\\ui_ground_hud_toolbar_skinned.inc"]);
        assert!(!tb.contains("TextAlignmentVertical='Top'"));
        assert!(tb.contains("CellSize='36,12'"));
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
