//! Read the client's effective config: `client.cfg` and everything it
//! `.include`s, in order, later files overriding earlier ones (that is how the
//! client resolves them: options.cfg is the launcher's, custom.cfg the user's).
//! Only used to pre-fill the preview's resolution and UI scale.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Default, Debug)]
pub struct ClientCfg {
    /// (section, key) -> value, both lower-cased
    values: HashMap<(String, String), String>,
}

impl ClientCfg {
    pub fn load(game_dir: &Path) -> ClientCfg {
        let mut cfg = ClientCfg::default();
        let mut seen = Vec::new();
        cfg.read_file(game_dir, "client.cfg", &mut seen);
        cfg
    }

    fn read_file(&mut self, game_dir: &Path, name: &str, seen: &mut Vec<String>) {
        let key = name.to_ascii_lowercase();
        if seen.contains(&key) || seen.len() > 32 {
            return;
        }
        seen.push(key);
        let Ok(text) = std::fs::read_to_string(game_dir.join(name)) else { return };
        let mut section = String::new();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            if let Some(rest) = line.strip_prefix(".include") {
                let inc = rest.trim().trim_matches('"').trim();
                if !inc.is_empty() {
                    self.read_file(game_dir, inc, seen);
                }
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].trim().to_ascii_lowercase();
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let v = v.trim().trim_matches('"').to_string();
                self.values.insert((section.clone(), k.trim().to_ascii_lowercase()), v);
            }
        }
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.values
            .get(&(section.to_ascii_lowercase(), key.to_ascii_lowercase()))
            .map(String::as_str)
    }

    fn num<T: std::str::FromStr>(&self, section: &str, key: &str) -> Option<T> {
        self.get(section, key)?.trim().parse().ok()
    }

    /// Options > Interface scale (uiScalingFactor), if set.
    pub fn ui_scale(&self) -> Option<f64> {
        self.num::<f64>("ClientUserInterface", "uiScalingFactor").filter(|v| *v > 0.1)
    }

    /// screenWidth x screenHeight, if both set.
    pub fn resolution(&self) -> Option<(u32, u32)> {
        let w = self.num::<u32>("ClientGraphics", "screenWidth")?;
        let h = self.num::<u32>("ClientGraphics", "screenHeight")?;
        (w >= 320 && h >= 240).then_some((w, h))
    }

    /// borderlessWindow=1: the client ignores screenWidth/Height and uses the desktop.
    pub fn borderless(&self) -> bool {
        matches!(self.get("ClientGraphics", "borderlessWindow"), Some("1") | Some("true"))
    }
}

/// The buff icon size the client compiles in when a character has never
/// touched Options > Interface > Buff Icon (CuiPreferences default).
pub const DEFAULT_BUFF_ICON_SIZE: u32 = 22;

/// One value out of a per-character options file (`profiles/<user>/<cluster>/<id>.opt`).
/// It is an IFF: FORM chunks (big-endian sizes) holding `INT `/`FLT `/`BOOL`/...
/// leaves whose body is the value followed by `section\0key\0`.  Only
/// preferences the player changed are stored, so a missing key means "default".
pub fn read_opt_int(path: &Path, section: &str, key: &str) -> Option<i32> {
    let d = std::fs::read(path).ok()?;
    fn walk(d: &[u8], mut off: usize, end: usize, want: &(String, String), out: &mut Option<i32>) {
        while off + 8 <= end && out.is_none() {
            let tag = &d[off..off + 4];
            let size = u32::from_be_bytes([d[off + 4], d[off + 5], d[off + 6], d[off + 7]]) as usize;
            let body_end = (off + 8 + size).min(end);
            if tag == b"FORM" {
                walk(d, off + 12, body_end, want, out);
            } else if tag == b"INT " && size >= 4 {
                let body = &d[off + 8..body_end];
                let mut tail = &body[4..];
                while let Some(t) = tail.strip_suffix(b"\0") {
                    tail = t; // records are NUL-padded after the key
                }
                let parts: Vec<&[u8]> = tail.split(|b| *b == 0).collect();
                if parts.len() >= 2 {
                    let (sec, k) = (parts[parts.len() - 2], parts[parts.len() - 1]);
                    if sec.eq_ignore_ascii_case(want.0.as_bytes()) && k.eq_ignore_ascii_case(want.1.as_bytes()) {
                        *out = Some(i32::from_le_bytes([body[0], body[1], body[2], body[3]]));
                    }
                }
            }
            off = off + 8 + size;
        }
    }
    let mut out = None;
    walk(&d, 0, d.len(), &(section.to_string(), key.to_string()), &mut out);
    out
}

/// A BOOL out of an options IFF (`BOOL` leaf: one byte then `section\0key\0`).
pub fn read_opt_bool(path: &Path, section: &str, key: &str) -> Option<bool> {
    let d = std::fs::read(path).ok()?;
    fn walk(d: &[u8], mut off: usize, end: usize, want: &(String, String), out: &mut Option<bool>) {
        while off + 8 <= end && out.is_none() {
            let tag = &d[off..off + 4];
            let size = u32::from_be_bytes([d[off + 4], d[off + 5], d[off + 6], d[off + 7]]) as usize;
            let body_end = (off + 8 + size).min(end);
            if tag == b"FORM" {
                walk(d, off + 12, body_end, want, out);
            } else if tag == b"BOOL" && size >= 1 {
                let body = &d[off + 8..body_end];
                let mut tail = &body[1..];
                while let Some(t) = tail.strip_suffix(b"\0") {
                    tail = t;
                }
                let parts: Vec<&[u8]> = tail.split(|b| *b == 0).collect();
                if parts.len() >= 2 {
                    let (sec, k) = (parts[parts.len() - 2], parts[parts.len() - 1]);
                    if sec.eq_ignore_ascii_case(want.0.as_bytes()) && k.eq_ignore_ascii_case(want.1.as_bytes()) {
                        *out = Some(body[0] != 0);
                    }
                }
            }
            off = off + 8 + size;
        }
    }
    let mut out = None;
    walk(&d, 0, d.len(), &(section.to_string(), key.to_string()), &mut out);
    out
}

/// Options > Interface > double toolbar (`useDoubleToolbar` in local_machine_options.iff).
pub fn use_double_toolbar(game_dir: &Path) -> Option<bool> {
    read_opt_bool(&game_dir.join("local_machine_options.iff"), "ClientUserInterface", "useDoubleToolbar")
}

/// Every per-character options file under `profiles/<user>/<cluster>/*.opt`.
pub fn all_opts(game_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(profiles) = std::fs::read_dir(game_dir.join("profiles")) else { return out };
    for user in profiles.flatten() {
        let Ok(clusters) = std::fs::read_dir(user.path()) else { continue };
        for cluster in clusters.flatten() {
            let Ok(files) = std::fs::read_dir(cluster.path()) else { continue };
            for f in files.flatten() {
                let p = f.path();
                if p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("opt")) == Some(true) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// Buff icon sizes the client actually uses.  The Options > Interface sliders
/// are machine-wide: `local_machine_options.iff` in the game root holds
/// `buffIconSizeStatus` / `Pet` / `Group` (same IFF layout as the character
/// files).  Falls back to the largest per-character value, then the defaults.
/// Returns (status, pet, group, where it came from).
pub fn buff_icon_sizes(game_dir: &Path) -> Option<(u32, u32, u32, String)> {
    let lmo = game_dir.join("local_machine_options.iff");
    let ok = |v: Option<i32>| v.filter(|v| (5..=120).contains(v)).map(|v| v as u32);
    if lmo.is_file() {
        let status = ok(read_opt_int(&lmo, "ClientUserInterface", "buffIconSizeStatus"));
        let pet = ok(read_opt_int(&lmo, "ClientUserInterface", "buffIconSizePet"));
        let group = ok(read_opt_int(&lmo, "ClientUserInterface", "buffIconSizeGroup"));
        if status.is_some() || pet.is_some() || group.is_some() {
            return Some((
                status.unwrap_or(DEFAULT_BUFF_ICON_SIZE),
                pet.unwrap_or(16),
                group.unwrap_or(16),
                "local_machine_options.iff".to_string(),
            ));
        }
    }
    let opts = all_opts(game_dir);
    if opts.is_empty() {
        return None;
    }
    let mut best: Option<(u32, String)> = None;
    for opt in &opts {
        if let Some(v) = ok(read_opt_int(opt, "ClientUserInterface", "buffIconSizeStatus")) {
            let who = opt.file_stem().and_then(|s| s.to_str()).unwrap_or("?").to_string();
            if best.as_ref().map(|(b, _)| v > *b).unwrap_or(true) {
                best = Some((v, who));
            }
        }
    }
    Some(match best {
        Some((v, who)) => (v, 16, 16, format!("largest of {} characters, set by {who}", opts.len())),
        None => (DEFAULT_BUFF_ICON_SIZE, 16, 16, format!("client defaults; nothing saved for {} characters", opts.len())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn include_chain_last_wins() {
        let d = std::env::temp_dir().join(format!("dodgins-cfg-test-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("client.cfg"), ".include \"options.cfg\"\n.include \"custom.cfg\"\n").unwrap();
        std::fs::write(
            d.join("options.cfg"),
            "[ClientGraphics]\n\tscreenWidth=1440\n\tscreenHeight=1080\n[ClientUserInterface]\n\tuiScalingFactor=2.250000\n",
        )
        .unwrap();
        std::fs::write(d.join("custom.cfg"), "# mine\n[ClientGraphics]\n\tscreenWidth=3840\n\tscreenHeight=2160\n").unwrap();
        let c = ClientCfg::load(&d);
        assert_eq!(c.resolution(), Some((3840, 2160)));
        assert_eq!(c.ui_scale(), Some(2.25));
        assert!(!c.borderless());
        let _ = std::fs::remove_dir_all(&d);
    }
}
