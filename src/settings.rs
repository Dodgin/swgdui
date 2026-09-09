//! User settings, kept as `settings.json` next to the exe.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
    pub fn parse(s: &str) -> Option<Rgb> {
        let s = s.trim();
        let h = s.strip_prefix('#')?;
        if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let v = u32::from_str_radix(h, 16).ok()?;
        Some(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
    /// Banker's rounding, to match the colour tables the templates were built with.
    pub fn scale(self, mul: f64) -> Rgb {
        let f = |c: u8| ((c as f64) * mul).round_ties_even().clamp(0.0, 255.0) as u8;
        Rgb(f(self.0), f(self.1), f(self.2))
    }
    pub fn arr(self) -> [u8; 3] {
        [self.0, self.1, self.2]
    }
    pub fn from_arr(a: [u8; 3]) -> Rgb {
        Rgb(a[0], a[1], a[2])
    }
}

/// Profession presets (from colors.txt), keyed by the canonical short name.
pub const PROFESSIONS: [(&str, &str, Rgb); 10] = [
    ("mando", "Commando", Rgb(0xC6, 0x9B, 0x6D)),
    ("officer", "Officer", Rgb(0xAB, 0xD4, 0x73)),
    ("jedi", "Jedi", Rgb(0x00, 0xB3, 0xFF)),
    ("spy", "Spy", Rgb(0xFF, 0xF5, 0x69)),
    ("bh", "Bounty Hunter", Rgb(0xC4, 0x1E, 0x3A)),
    ("medic", "Medic", Rgb(0x87, 0x87, 0xED)),
    ("smuggler", "Smuggler", Rgb(0xF4, 0x8C, 0xBA)),
    ("ent", "Entertainer", Rgb(0xFF, 0x88, 0x00)),
    ("trader", "Trader", Rgb(0x8F, 0x8F, 0x8F)),
    ("green", "Green (default)", Rgb(0x4A, 0xAB, 0x4D)),
];

/// Accepted spellings of each profession name.
pub fn canonical_profession(name: &str) -> &'static str {
    match name.trim().to_ascii_lowercase().as_str() {
        "mando" | "commando" => "mando",
        "officer" => "officer",
        "jedi" => "jedi",
        "spy" => "spy",
        "bh" | "bountyhunter" => "bh",
        "medic" => "medic",
        "smuggler" => "smuggler",
        "ent" | "entertainer" => "ent",
        "trader" => "trader",
        _ => "green",
    }
}

pub fn profession_base(key: &str) -> Rgb {
    PROFESSIONS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, c)| *c)
        .unwrap_or(Rgb(0x4A, 0xAB, 0x4D))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "PascalCase")]
pub struct Settings {
    pub game_dir: String,
    /// "" = green default; otherwise a key from PROFESSIONS
    pub profession: String,
    /// "" = use the profession colour; otherwise a #RRGGBB override
    pub health_color: String,
    pub target_health_color: String,
    pub power_color: String,
    pub backdrop_mul: f64,
    pub class_color_mul: f64,
    pub buff_icon_size: u32,
    pub buff_columns: u32,
    pub buff_rows: u32,
    pub debuff_columns: u32,
    pub debuff_rows: u32,
    pub buff_location: String,
    /// "" = directly under the buff window
    pub debuff_location: String,
    pub role_colors: bool,
    pub inline_buffs: bool,
    /// preview only: the display the mockup shows, "WxH"
    pub preview_resolution: String,
    /// preview only: the client's interface scale (Options > Interface), e.g. 2.25 at 4K.
    /// Window coordinates are in UI pixels, i.e. screen pixels / this.
    pub ui_scale: f64,
    /// preview only: the client's buff icon sliders for the pet and group windows
    pub pet_icon_size: u32,
    pub group_icon_size: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            game_dir: r"C:\SWG Legends".into(),
            profession: String::new(),
            health_color: String::new(),
            target_health_color: "#C74040".into(),
            power_color: "#FFD200".into(),
            backdrop_mul: 0.25,
            class_color_mul: 0.75,
            buff_icon_size: 22,
            buff_columns: 12,
            buff_rows: 2,
            debuff_columns: 12,
            debuff_rows: 1,
            buff_location: "10,10".into(),
            debuff_location: "567,760".into(),
            role_colors: true,
            inline_buffs: false,
            preview_resolution: "1920x1080".into(),
            ui_scale: 1.0,
            pet_icon_size: 16,
            group_icon_size: 16,
        }
    }
}

impl Settings {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join("settings.json")
    }

    pub fn load(dir: &Path) -> Settings {
        std::fs::read_to_string(Self::path(dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let s = serde_json::to_string_pretty(self).expect("settings serialise");
        std::fs::write(Self::path(dir), s)
    }

    /// Profession key actually in effect ("green" when unset).
    pub fn profession_key(&self) -> &'static str {
        if self.profession.is_empty() {
            "green"
        } else {
            canonical_profession(&self.profession)
        }
    }

    /// Class palette after ClassColorMul, as the installer applies it.
    pub fn class_color(&self, key: &str) -> Rgb {
        profession_base(key).scale(self.class_color_mul)
    }

    /// The static player health colour (used when role colours are off, and as
    /// the "default" swatch when they are on).
    pub fn health(&self) -> Result<Rgb, String> {
        if self.health_color.is_empty() {
            Ok(self.class_color(self.profession_key()))
        } else {
            Rgb::parse(&self.health_color)
                .ok_or_else(|| format!("HealthColor must be #RRGGBB, got '{}'", self.health_color))
        }
    }
    pub fn target_health(&self) -> Result<Rgb, String> {
        Rgb::parse(&self.target_health_color)
            .ok_or_else(|| format!("TargetHealthColor must be #RRGGBB, got '{}'", self.target_health_color))
    }
    pub fn power(&self) -> Result<Rgb, String> {
        Rgb::parse(&self.power_color)
            .ok_or_else(|| format!("PowerColor must be #RRGGBB, got '{}'", self.power_color))
    }

    pub fn parse_loc(s: &str) -> Option<(i32, i32)> {
        let (a, b) = s.trim().split_once(',')?;
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    }

    /// Debuff window location, defaulting to just under the buff window.
    pub fn debuff_loc(&self) -> Result<(i32, i32), String> {
        if self.debuff_location.trim().is_empty() {
            let (bx, by) = Self::parse_loc(&self.buff_location)
                .ok_or_else(|| format!("location must be 'x,y', got '{}'", self.buff_location))?;
            Ok((bx, by + (self.buff_rows * self.buff_icon_size) as i32 + 6))
        } else {
            Self::parse_loc(&self.debuff_location)
                .ok_or_else(|| format!("location must be 'x,y', got '{}'", self.debuff_location))
        }
    }
}
