//! Read a character's input map (`profiles/<user>/<cluster>/<id>.inp`, an
//! IFF `IMAP`/`0006` form) for the keys bound to toolbar slots, so the pages
//! can carry labels for the slots the client does not label itself (the
//! double toolbar's top row, slots 12-23, and the pet bar).
//!
//! Layout (recovered from the files themselves): `SHFT`/`KEY ` records map modifier scancodes to bits
//! (Alt 1, Ctrl 2, Shift 4); each `MAP ` form's `INFO` chunk starts with the
//! modifier state its bindings need; `KEYS`/`DATA` records are
//! `scancode, 0, 0, 0, "CMD_...\0"`, `MOSB`/`DATA` records `button u32 LE,
//! "CMD_...\0"`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct Keymap {
    /// command -> labels, best first (no modifier before modified, keys before mouse)
    bindings: HashMap<String, Vec<(u8, String)>>,
}

impl Keymap {
    pub fn load(path: &Path) -> Result<Keymap, String> {
        let d = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&d)
    }

    pub fn parse(d: &[u8]) -> Result<Keymap, String> {
        if d.len() < 20 || &d[0..4] != b"FORM" || &d[8..12] != b"IMAP" {
            return Err("not an IMAP input map".into());
        }
        let mut km = Keymap::default();
        walk(d, 0, d.len(), 0, &mut km);
        Ok(km)
    }

    /// The label for a command ("" when unbound): `4`, `S4` (Shift), `C4`
    /// (Ctrl), `A4` (Alt), `M4` (mouse button 4).
    pub fn label(&self, cmd: &str) -> String {
        self.bindings.get(cmd).and_then(|v| v.first()).map(|(_, l)| l.clone()).unwrap_or_default()
    }

    /// Labels for `CMD_uiToolbarSlot<nn>`.
    pub fn slot(&self, n: u32) -> String {
        self.label(&format!("CMD_uiToolbarSlot{n:02}"))
    }

    /// Labels for `CMD_petToolbarSlot<n>`.
    pub fn pet_slot(&self, n: u32) -> String {
        self.label(&format!("CMD_petToolbarSlot{n}"))
    }

    fn add(&mut self, cmd: &str, rank: u8, label: String) {
        let v = self.bindings.entry(cmd.to_string()).or_default();
        if v.iter().any(|(_, l)| *l == label) {
            return;
        }
        v.push((rank, label));
        v.sort_by_key(|(r, _)| *r);
    }
}

fn walk(d: &[u8], mut off: usize, end: usize, state: u8, km: &mut Keymap) {
    while off + 8 <= end {
        let tag = &d[off..off + 4];
        let size = u32::from_be_bytes([d[off + 4], d[off + 5], d[off + 6], d[off + 7]]) as usize;
        let body = off + 8;
        let body_end = (body + size).min(end);
        if tag == b"FORM" && body + 4 <= body_end {
            let sub = &d[body..body + 4];
            match sub {
                b"MAP " => {
                    // INFO chunk first: its first byte is the modifier state
                    let st = read_map_state(d, body + 4, body_end);
                    walk_map(d, body + 4, body_end, st, km);
                }
                b"CMDS" | b"SHFT" => {}
                _ => walk(d, body + 4, body_end, state, km),
            }
        }
        off = body + size;
    }
}

fn read_map_state(d: &[u8], mut off: usize, end: usize) -> u8 {
    while off + 8 <= end {
        let tag = &d[off..off + 4];
        let size = u32::from_be_bytes([d[off + 4], d[off + 5], d[off + 6], d[off + 7]]) as usize;
        if tag == b"INFO" && size >= 1 {
            return d[off + 8];
        }
        off += 8 + size;
    }
    0
}

fn walk_map(d: &[u8], mut off: usize, end: usize, state: u8, km: &mut Keymap) {
    while off + 8 <= end {
        let tag = &d[off..off + 4];
        let size = u32::from_be_bytes([d[off + 4], d[off + 5], d[off + 6], d[off + 7]]) as usize;
        let body = off + 8;
        let body_end = (body + size).min(end);
        if tag == b"FORM" && body + 4 <= body_end {
            let sub = &d[body..body + 4];
            let mouse = sub == b"MOSB";
            if sub == b"KEYS" || mouse {
                let mut o = body + 4;
                while o + 8 <= body_end {
                    let t = &d[o..o + 4];
                    let sz = u32::from_be_bytes([d[o + 4], d[o + 5], d[o + 6], d[o + 7]]) as usize;
                    let rec = &d[o + 8..(o + 8 + sz).min(body_end)];
                    if t == b"DATA" && rec.len() > 4 {
                        let cmd_bytes = rec[4..].split(|b| *b == 0).next().unwrap_or(&[]);
                        let cmd = String::from_utf8_lossy(cmd_bytes);
                        let key = if mouse {
                            let b = u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]);
                            Some(format!("M{}", b + 1))
                        } else {
                            key_name(rec[0])
                        };
                        if let Some(k) = key {
                            let rank = (if mouse { 8 } else { 0 }) + state.count_ones() as u8;
                            km.add(&cmd, rank, format!("{}{k}", mod_prefix(state)));
                        }
                    }
                    o += 8 + sz;
                }
            }
        }
        off = body + size;
    }
}

fn mod_prefix(state: u8) -> String {
    let mut s = String::new();
    if state & 2 != 0 {
        s.push('C');
    }
    if state & 4 != 0 {
        s.push('S');
    }
    if state & 1 != 0 {
        s.push('A');
    }
    s
}

/// Short label for a DirectInput scancode.
pub fn key_name(sc: u8) -> Option<String> {
    let s: &str = match sc {
        0x02..=0x0A => return Some(((b'1' + (sc - 0x02)) as char).to_string()),
        0x0B => "0",
        0x0C => "-",
        0x0D => "=",
        0x0E => "BS",
        0x0F => "Tab",
        0x10 => "Q", 0x11 => "W", 0x12 => "E", 0x13 => "R", 0x14 => "T", 0x15 => "Y", 0x16 => "U",
        0x17 => "I", 0x18 => "O", 0x19 => "P", 0x1A => "[", 0x1B => "]", 0x1C => "Ent",
        0x1E => "A", 0x1F => "S", 0x20 => "D", 0x21 => "F", 0x22 => "G", 0x23 => "H", 0x24 => "J",
        0x25 => "K", 0x26 => "L", 0x27 => ";", 0x28 => "Ap", 0x29 => "`", 0x2B => "\\",
        0x2C => "Z", 0x2D => "X", 0x2E => "C", 0x2F => "V", 0x30 => "B", 0x31 => "N", 0x32 => "M",
        0x33 => ",", 0x34 => ".", 0x35 => "/", 0x37 => "N*", 0x39 => "Spc", 0x3A => "Caps",
        0x3B => "F1", 0x3C => "F2", 0x3D => "F3", 0x3E => "F4", 0x3F => "F5", 0x40 => "F6",
        0x41 => "F7", 0x42 => "F8", 0x43 => "F9", 0x44 => "F10", 0x57 => "F11", 0x58 => "F12",
        0x47 => "N7", 0x48 => "N8", 0x49 => "N9", 0x4A => "N-", 0x4B => "N4", 0x4C => "N5", 0x4D => "N6",
        0x4E => "N+", 0x4F => "N1", 0x50 => "N2", 0x51 => "N3", 0x52 => "N0", 0x53 => "N.",
        0x9C => "NEnt", 0xB5 => "N/", 0xC7 => "Home", 0xC8 => "Up", 0xC9 => "PgUp", 0xCB => "Left",
        0xCD => "Right", 0xCF => "End", 0xD0 => "Down", 0xD1 => "PgDn", 0xD2 => "Ins", 0xD3 => "Del",
        _ => return None,
    };
    Some(s.to_string())
}

/// One character under `profiles/<user>/<cluster>/`: the client writes its
/// `.uis` / `.opt` on every logout but the `.inp` only when a binding
/// changes, so "last played" comes from the former and the keymap may be
/// missing altogether (the character still uses a stock preset).
#[derive(Clone, Debug)]
pub struct Character {
    pub id: String,
    pub cluster: String,
    pub last_played: std::time::SystemTime,
    pub inp: Option<PathBuf>,
}

impl Character {
    /// "404079662065 (Omega) - played 2026-09-08 17:49"
    pub fn label(&self) -> String {
        let secs = self.last_played.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let mut s = format!("{} ({}) - played {}", self.id, self.cluster, fmt_local(secs));
        if self.inp.is_none() {
            s.push_str(", no keymap saved");
        }
        s
    }
}

/// "YYYY-MM-DD HH:MM" in local time (civil-from-days, no chrono).
fn fmt_local(unix: i64) -> String {
    let t = unix + local_offset_secs();
    let days = t.div_euclid(86400);
    let rem = t.rem_euclid(86400);
    let (h, m) = (rem / 3600, (rem % 3600) / 60);
    // Howard Hinnant's civil_from_days
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}")
}

#[cfg(windows)]
fn local_offset_secs() -> i64 {
    #[repr(C)]
    struct Tzi {
        bias: i32,
        rest: [u8; 168],
    }
    extern "system" {
        fn GetTimeZoneInformation(t: *mut Tzi) -> u32;
    }
    let mut t = Tzi { bias: 0, rest: [0; 168] };
    // SAFETY: Tzi is 172 bytes, the size of TIME_ZONE_INFORMATION, and the OS only writes into it
    let r = unsafe { GetTimeZoneInformation(&mut t) };
    let dst = if r == 2 { -60 } else { 0 }; // TIME_ZONE_ID_DAYLIGHT; DaylightBias is -60 everywhere it applies
    -((t.bias + dst) as i64) * 60
}

#[cfg(not(windows))]
fn local_offset_secs() -> i64 {
    0
}

/// Every character under `profiles/`, most recently played first.
pub fn find_characters(game_dir: &Path) -> Vec<Character> {
    let mut by_id: std::collections::BTreeMap<(String, String), Character> = std::collections::BTreeMap::new();
    let Ok(profiles) = std::fs::read_dir(game_dir.join("profiles")) else { return Vec::new() };
    for user in profiles.flatten() {
        let Ok(clusters) = std::fs::read_dir(user.path()) else { continue };
        for cluster in clusters.flatten() {
            let cluster_name = cluster.file_name().to_string_lossy().into_owned();
            let Ok(files) = std::fs::read_dir(cluster.path()) else { continue };
            for f in files.flatten() {
                let p = f.path();
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
                let Some((id, ext)) = name.split_once('.') else { continue };
                if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                let mtime = f.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
                let ch = by_id.entry((cluster_name.clone(), id.to_string())).or_insert_with(|| Character {
                    id: id.to_string(),
                    cluster: cluster_name.clone(),
                    last_played: std::time::UNIX_EPOCH,
                    inp: None,
                });
                match ext {
                    "uis" | "opt" => ch.last_played = ch.last_played.max(mtime),
                    "inp" => ch.inp = Some(p.clone()),
                    _ => {}
                }
            }
        }
    }
    let mut out: Vec<Character> = by_id.into_values().collect();
    out.sort_by(|a, b| b.last_played.cmp(&a.last_played));
    out
}

/// The keymap the settings ask for: an explicit file, else the one of the
/// most recently played character.  `Err` says why there is none.
pub fn for_settings(game_dir: &Path, keymap_file: &str) -> Result<(PathBuf, Keymap), String> {
    let path = if keymap_file.trim().is_empty() {
        let chars = find_characters(game_dir);
        let Some(c) = chars.first() else { return Err("no characters under profiles/".into()) };
        c.inp.clone().ok_or_else(|| {
            format!("last-played character {} ({}) has no saved keymap (stock preset); pick another", c.id, c.cluster)
        })?
    } else {
        let p = PathBuf::from(keymap_file.trim());
        if p.is_absolute() { p } else { game_dir.join(p) }
    };
    let km = Keymap::load(&path)?;
    Ok((path, km))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(tag: &[u8], body: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend_from_slice(&(body.len() as u32).to_be_bytes());
        v.extend_from_slice(body);
        v
    }
    fn form(sub: &[u8], kids: &[Vec<u8>]) -> Vec<u8> {
        let mut body = sub.to_vec();
        for k in kids {
            body.extend_from_slice(k);
        }
        chunk(b"FORM", &body)
    }
    fn rec(sc: u8, cmd: &str) -> Vec<u8> {
        let mut b = vec![sc, 0, 0, 0];
        b.extend_from_slice(cmd.as_bytes());
        b.push(0);
        chunk(b"DATA", &b)
    }

    #[test]
    fn labels_from_synthetic_map() {
        let main = form(b"MAP ", &[
            chunk(b"INFO", &[0, 0, 0, 0, 0]),
            form(b"KEYS", &[rec(0x10, "CMD_uiToolbarSlot12"), rec(0x02, "CMD_uiToolbarSlot00"), rec(0x0B, "CMD_uiToolbarSlot09")]),
            form(b"MOSB", &[{
                let mut b = 3u32.to_le_bytes().to_vec();
                b.extend_from_slice(b"CMD_petToolbarSlot2\0");
                chunk(b"DATA", &b)
            }]),
        ]);
        let shifted = form(b"MAP ", &[
            chunk(b"INFO", &[4, 0, 0, 0, 1]),
            form(b"KEYS", &[rec(0x14, "CMD_uiToolbarSlot18"), rec(0x10, "CMD_uiToolbarSlot12")]),
        ]);
        let v6 = form(b"0006", &[main, shifted]);
        let imap = form(b"IMAP", &[v6]);
        let km = Keymap::parse(&imap).unwrap();
        assert_eq!(km.slot(12), "Q");
        assert_eq!(km.slot(0), "1");
        assert_eq!(km.slot(9), "0");
        assert_eq!(km.slot(18), "ST");
        assert_eq!(km.pet_slot(2), "M4");
        assert_eq!(km.slot(23), "");
    }
}
