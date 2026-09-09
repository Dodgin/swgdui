//! In-app rendering of the frames with the current colours.  The geometry is
//! the same set of numbers build_unitframes.py bakes into the templates, so
//! what is drawn here is what the client lays out (minus the game's fonts and
//! icon art, which are stand-ins).

use crate::settings::{Rgb, Settings};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, Ui, Vec2};

// ---- frame geometry (build_unitframes.py) ----
pub const FRAME_W: f32 = 287.0;
pub const BODY_H: f32 = 42.0;
const INNER_X: f32 = 1.0;
const INNER_W: f32 = 285.0;
const HEALTH_Y: f32 = 1.0;
const HEALTH_H: f32 = 26.0;
const POWER_Y: f32 = 28.0;
const POWER_H: f32 = 13.0;
const VALUE_W: f32 = 63.0;
const VALUE_X: f32 = INNER_W - 8.0 - VALUE_W - 30.0; // 184
const FACTION: f32 = 13.0;
const FACTION_Y: f32 = HEALTH_Y + 6.0; // (26 - 13) // 2
const CAP: Rgb = Rgb(0x0A, 0x0A, 0x0A);
const GREY: Rgb = Rgb(0x4A, 0x4A, 0x4A);
const GREY_BG: Rgb = Rgb(0x1A, 0x1A, 0x1A);
const TEXT: Rgb = Rgb(0xFF, 0xFF, 0xFF);
const TEXT_DIM: Rgb = Rgb(0xD8, 0xD8, 0xD8);
const ELITE: Rgb = Rgb(0xEF, 0xDF, 0xA5);
const ROLE_BACK: Rgb = Rgb(0x1C, 0x1C, 0x1C);

// ---- group / pet geometry ----
pub const GROUP_W: f32 = 255.0;
pub const GROUP_ROW_H: f32 = 60.0;
pub const PET_W: f32 = 220.0;
pub const PET_H: f32 = BODY_H + 2.0 + 16.0;

// ---- nameplate geometry ----
const NP_W: f32 = 120.0;
const NP_H_H: f32 = 12.0;
const NP_A_H: f32 = 3.0;

fn c32(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// What the preview shows; all of it is fake sample data.
pub struct Sample {
    pub player_name: &'static str,
    pub player_level: &'static str,
    pub player_hp: &'static str,
    pub player_frac: f32,
    pub player_action: f32,
    pub target_name: &'static str,
    pub target_level: &'static str,
    pub target_hp: &'static str,
    pub target_frac: f32,
    pub target_action: f32,
    pub target_range: &'static str,
    pub target_elite: bool,
    pub tot_name: &'static str,
    pub tot_level: &'static str,
    pub tot_hp: &'static str,
    pub tot_frac: f32,
    pub tot_range: &'static str,
    pub np_name: &'static str,
    pub np_title: &'static str,
    pub np_hp: &'static str,
    pub np_frac: f32,
    pub np_range: &'static str,
}

pub const SAMPLE: Sample = Sample {
    player_name: "Dodgin",
    player_level: "90",
    player_hp: "23408",
    player_frac: 0.78,
    player_action: 0.92,
    target_name: "Krayt Dragon Ancient",
    target_level: "90",
    target_hp: "14673",
    target_frac: 0.55,
    target_action: 0.7,
    target_range: "12m",
    target_elite: true,
    tot_name: "Dodgin",
    tot_level: "90",
    tot_hp: "23408",
    tot_frac: 0.78,
    tot_range: "12m",
    np_name: "Lava Flea",
    np_title: "a lava flea",
    np_hp: "3120",
    np_frac: 0.63,
    np_range: "4m",
};

/// Colours the preview needs, derived once from the settings.
pub struct Palette {
    pub health: Rgb,
    pub health_bg: Rgb,
    pub target: Rgb,
    pub target_bg: Rgb,
    pub power: Rgb,
    pub power_bg: Rgb,
    pub role_colors: bool,
    /// class colour shown in the player bar when role colours are on
    pub role_fill: Rgb,
}

impl Palette {
    pub fn from(s: &Settings, preview_class: &str) -> Palette {
        let health = s.health().unwrap_or(Rgb(0x4A, 0xAB, 0x4D));
        let target = s.target_health().unwrap_or(Rgb(0xC7, 0x40, 0x40));
        let power = s.power().unwrap_or(Rgb(0xFF, 0xD2, 0x00));
        Palette {
            health,
            health_bg: health.scale(s.backdrop_mul),
            target,
            target_bg: target.scale(s.backdrop_mul),
            power,
            power_bg: power.scale(s.backdrop_mul),
            role_colors: s.role_colors,
            role_fill: s.class_color(preview_class),
        }
    }
}

/// Painter in frame pixel space: origin + uniform scale.
pub struct Canvas<'a> {
    pub painter: &'a egui::Painter,
    pub origin: Pos2,
    pub scale: f32,
}

impl<'a> Canvas<'a> {
    pub fn new(painter: &'a egui::Painter, origin: Pos2, scale: f32) -> Self {
        Canvas { painter, origin, scale }
    }
    fn r(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        let s = self.scale;
        Rect::from_min_size(
            self.origin + Vec2::new(x * s, y * s),
            Vec2::new(w * s, h * s),
        )
    }
    pub fn fill(&self, x: f32, y: f32, w: f32, h: f32, c: Rgb) {
        self.painter.rect_filled(self.r(x, y, w, h), 0.0, c32(c));
    }
    pub fn fill_a(&self, x: f32, y: f32, w: f32, h: f32, c: Rgb, a: u8) {
        let c = Color32::from_rgba_unmultiplied(c.0, c.1, c.2, a);
        self.painter.rect_filled(self.r(x, y, w, h), 0.0, c);
    }
    pub fn outline(&self, x: f32, y: f32, w: f32, h: f32, c: Rgb) {
        self.painter.rect_stroke(
            self.r(x, y, w, h),
            0.0,
            Stroke::new(self.scale.max(1.0), c32(c)),
            egui::StrokeKind::Inside,
        );
    }
    /// Text inside a box.  `px` is the game font's nominal pixel size.
    pub fn text(&self, x: f32, y: f32, w: f32, h: f32, align: Align2, px: f32, c: Rgb, s: &str) {
        let rect = self.r(x, y, w, h);
        let font = FontId::proportional(px * self.scale * 0.95);
        let anchor = match align {
            Align2::LEFT_CENTER => Pos2::new(rect.min.x, rect.center().y),
            Align2::RIGHT_CENTER => Pos2::new(rect.max.x, rect.center().y),
            _ => rect.center(),
        };
        // clip long names to their box the way the client does
        let clip = self.painter.with_clip_rect(rect.intersect(self.painter.clip_rect()));
        clip.text(anchor, align, s, font, c32(c));
    }
    fn icon(&self, x: f32, y: f32, size: f32, c: Rgb) {
        let rect = self.r(x, y, size, size);
        self.painter.circle_filled(rect.center(), rect.width() * 0.42, c32(c));
        self.painter.circle_stroke(rect.center(), rect.width() * 0.42, Stroke::new(1.0, Color32::BLACK));
    }
}

/// Flat status bar: backdrop, fill by fraction, dark "reduced max" cap.
fn bar(cv: &Canvas, x: f32, y: f32, w: f32, h: f32, fill: Rgb, bg: Rgb, frac: f32, cap_w: f32) {
    cv.fill(x, y, w, h, bg);
    cv.fill(x, y, (w * frac).round(), h, fill);
    if cap_w > 0.0 {
        cv.fill(x + w - cap_w, y, cap_w, h, CAP);
    }
}

fn frame_shell(cv: &Canvas) {
    cv.fill(0.0, 0.0, FRAME_W, BODY_H, Rgb(0, 0, 0));
}

fn faction_badge(cv: &Canvas) {
    cv.icon(FRAME_W - 3.0 - FACTION, FACTION_Y, FACTION, Rgb(0xC8, 0xC8, 0xC8));
}

pub fn draw_player(cv: &Canvas, p: &Palette, s: &Sample) {
    frame_shell(cv);
    if p.role_colors {
        cv.fill(INNER_X, HEALTH_Y, INNER_W, HEALTH_H, ROLE_BACK);
        cv.fill(INNER_X, HEALTH_Y, (INNER_W * s.player_frac).round(), HEALTH_H, p.role_fill);
        cv.fill(INNER_X + INNER_W - 24.0, HEALTH_Y, 24.0, HEALTH_H, CAP);
    } else {
        bar(cv, INNER_X, HEALTH_Y, INNER_W, HEALTH_H, p.health, p.health_bg, s.player_frac, 24.0);
    }
    bar(cv, INNER_X, POWER_Y, INNER_W, POWER_H, p.power, p.power_bg, s.player_action, 24.0);

    let name_x;
    if p.role_colors {
        // role box keeps only the level, centred in 28px
        cv.text(INNER_X + 2.0, HEALTH_Y, 28.0, HEALTH_H, Align2::CENTER_CENTER, 13.0, TEXT, s.player_level);
        name_x = INNER_X + 2.0 + 28.0 + 4.0;
    } else {
        // profession icon 24x24 + level
        cv.fill_a(INNER_X + 2.0, HEALTH_Y + 1.0, 24.0, 24.0, Rgb(0, 0, 0), 110);
        cv.icon(INNER_X + 2.0 + 3.0, HEALTH_Y + 4.0, 18.0, Rgb(0xE0, 0xE0, 0xE0));
        cv.text(INNER_X + 2.0 + 27.0, HEALTH_Y, 25.0, HEALTH_H, Align2::LEFT_CENTER, 13.0, TEXT, s.player_level);
        name_x = INNER_X + 2.0 + 52.0 + 3.0;
    }
    let name_w = VALUE_X - name_x - 4.0;
    cv.text(name_x, HEALTH_Y, name_w, HEALTH_H, Align2::LEFT_CENTER, 13.0, TEXT, s.player_name);
    cv.text(VALUE_X, HEALTH_Y, VALUE_W, HEALTH_H, Align2::RIGHT_CENTER, 13.0, TEXT, s.player_hp);
    faction_badge(cv);
}

fn draw_target_like(
    cv: &Canvas,
    p: &Palette,
    name: &str,
    level: &str,
    hp: &str,
    frac: f32,
    action: f32,
    range: &str,
    elite: bool,
) {
    frame_shell(cv);
    bar(cv, INNER_X, HEALTH_Y, INNER_W, HEALTH_H, p.target, p.target_bg, frac, 24.0);
    bar(cv, INNER_X, POWER_Y, INNER_W, POWER_H, p.power, p.power_bg, action, 24.0);

    // con box: difficulty glyph + level, 28px at the left of the health bar
    let con_x = INNER_X + 2.0;
    let tri = [
        cv.r(con_x + 2.0, HEALTH_Y + 1.0, 6.0, 6.0).left_bottom(),
        cv.r(con_x + 2.0, HEALTH_Y + 1.0, 6.0, 6.0).right_bottom(),
        cv.r(con_x + 2.0, HEALTH_Y + 1.0, 6.0, 6.0).center_top(),
    ];
    cv.painter.add(egui::Shape::convex_polygon(tri.to_vec(), Color32::from_rgb(0xFF, 0x50, 0x50), Stroke::NONE));
    cv.text(con_x, HEALTH_Y, 28.0, HEALTH_H, Align2::CENTER_CENTER, 13.0, TEXT, level);

    let range_w = 42.0;
    let range_x = VALUE_X - 2.0 - range_w;
    let name_x = INNER_X + 2.0 + 28.0 + 4.0;
    let name_w = range_x - name_x - 2.0;
    cv.text(name_x, HEALTH_Y, name_w, HEALTH_H, Align2::LEFT_CENTER, 13.0, TEXT, name);
    cv.text(range_x, HEALTH_Y, range_w, HEALTH_H, Align2::RIGHT_CENTER, 11.0, TEXT_DIM, range);
    cv.text(VALUE_X, HEALTH_Y, VALUE_W, HEALTH_H, Align2::RIGHT_CENTER, 13.0, TEXT, hp);

    if elite {
        cv.fill_a(INNER_X + 3.0, POWER_Y, 40.0, POWER_H, Rgb(0, 0, 0), 140);
        cv.text(INNER_X + 3.0, POWER_Y, 40.0, POWER_H, Align2::CENTER_CENTER, 11.0, ELITE, "ELITE");
    }
    faction_badge(cv);
}

pub fn draw_target(cv: &Canvas, p: &Palette, s: &Sample) {
    draw_target_like(
        cv, p, s.target_name, s.target_level, s.target_hp, s.target_frac, s.target_action,
        s.target_range, s.target_elite,
    );
}

pub fn draw_tot(cv: &Canvas, p: &Palette, s: &Sample) {
    draw_target_like(cv, p, s.tot_name, s.tot_level, s.tot_hp, s.tot_frac, 0.9, s.tot_range, false);
}

/// A group member row or the pet frame: the player layout at `w` px, distance
/// on the action row next to the direction / voice icons.
#[allow(clippy::too_many_arguments)]
fn draw_member(cv: &Canvas, p: &Palette, w: f32, fill: Rgb, fill_bg: Rgb, name: &str, level: &str,
               hp: &str, frac: f32, action: f32, range: &str, voice: bool, focus: bool) {
    let inner_w = w - 2.0;
    let value_x = inner_w - 8.0 - VALUE_W;
    cv.fill(0.0, 0.0, w, BODY_H, Rgb(0, 0, 0));
    if p.role_colors {
        cv.fill(INNER_X, HEALTH_Y, inner_w, HEALTH_H, ROLE_BACK);
        cv.fill(INNER_X, HEALTH_Y, (inner_w * frac).round(), HEALTH_H, fill);
        cv.fill(INNER_X + inner_w - 24.0, HEALTH_Y, 24.0, HEALTH_H, CAP);
    } else {
        bar(cv, INNER_X, HEALTH_Y, inner_w, HEALTH_H, fill, fill_bg, frac, 24.0);
    }
    bar(cv, INNER_X, POWER_Y, inner_w, POWER_H, p.power, p.power_bg, action, 24.0);
    let name_x = if p.role_colors {
        cv.text(INNER_X + 2.0, HEALTH_Y, 28.0, HEALTH_H, Align2::CENTER_CENTER, 13.0, TEXT, level);
        INNER_X + 2.0 + 28.0 + 4.0
    } else {
        cv.fill_a(INNER_X + 2.0, HEALTH_Y + 1.0, 24.0, 24.0, Rgb(0, 0, 0), 110);
        cv.icon(INNER_X + 2.0 + 3.0, HEALTH_Y + 4.0, 18.0, Rgb(0xE0, 0xE0, 0xE0));
        cv.text(INNER_X + 2.0 + 27.0, HEALTH_Y, 25.0, HEALTH_H, Align2::LEFT_CENTER, 13.0, TEXT, level);
        INNER_X + 2.0 + 52.0 + 3.0
    };
    cv.text(name_x, HEALTH_Y, value_x - name_x - 4.0, HEALTH_H, Align2::LEFT_CENTER, 13.0, TEXT, name);
    cv.text(value_x, HEALTH_Y, VALUE_W, HEALTH_H, Align2::RIGHT_CENTER, 13.0, TEXT, hp);
    // action row: [distance] [voice] [arrow]
    let ax = w - 3.0 - 16.0;
    let arrow = [
        cv.r(ax + 3.0, POWER_Y, 10.0, 10.0).left_bottom(),
        cv.r(ax + 3.0, POWER_Y, 10.0, 10.0).right_bottom(),
        cv.r(ax + 3.0, POWER_Y, 10.0, 10.0).center_top(),
    ];
    cv.painter.add(egui::Shape::convex_polygon(arrow.to_vec(), Color32::from_rgb(0xF0, 0xF0, 0xF0), Stroke::NONE));
    let mut right = ax;
    if voice {
        cv.icon(ax - 2.0 - 16.0, POWER_Y - 2.0, 16.0, Rgb(0xC8, 0xC8, 0xC8));
        right = ax - 2.0 - 16.0;
    }
    cv.text(right - 4.0 - 42.0, POWER_Y, 42.0, POWER_H, Align2::RIGHT_CENTER, 11.0, TEXT_DIM, range);
    if focus {
        cv.fill(w - 3.0 - 13.0, FACTION_Y, 13.0, 12.0, Rgb(0xFF, 0xD2, 0x00));
    }
}

pub struct Member {
    pub name: &'static str,
    pub level: &'static str,
    pub hp: &'static str,
    pub frac: f32,
    pub action: f32,
    pub range: &'static str,
    pub class: &'static str,
    pub voice: bool,
}

pub const GROUP_SAMPLE: [Member; 4] = [
    Member { name: "Dodgin", level: "90", hp: "23408", frac: 0.78, action: 0.92, range: "0m", class: "jedi", voice: false },
    Member { name: "Kessa Varn", level: "90", hp: "19870", frac: 0.42, action: 0.66, range: "8m", class: "medic", voice: true },
    Member { name: "Brakk", level: "88", hp: "31200", frac: 0.95, action: 0.8, range: "14m", class: "mando", voice: false },
    Member { name: "Ilo Tadan", level: "90", hp: "17550", frac: 0.6, action: 0.3, range: "22m", class: "bh", voice: false },
];

/// Group window: one row per member on the client's 60px pitch.
pub fn draw_group(cv: &Canvas, p: &Palette, s: &Settings) {
    for (i, m) in GROUP_SAMPLE.iter().enumerate() {
        let row = Canvas::new(cv.painter, cv.origin + Vec2::new(0.0, i as f32 * GROUP_ROW_H * cv.scale), cv.scale);
        let fill = if p.role_colors { s.class_color(m.class) } else { p.health };
        draw_member(&row, p, GROUP_W, fill, p.health_bg, m.name, m.level, m.hp, m.frac, m.action, m.range, m.voice, i == 1);
        // buff / debuff row under the body, at the client's group icon size
        let cell = s.group_icon_size as f32;
        draw_icon_grid(&row, 0.0, BODY_H + 2.0, (GROUP_W / cell).floor() as u32, 1, cell, 3 + i as u32, Rgb(0x3A, 0x7B, 0xFF));
    }
}

/// Pet frame: player layout at 220px, buff row underneath.
pub fn draw_pet(cv: &Canvas, p: &Palette, s: &Settings) {
    let fill = if p.role_colors { s.class_color("green") } else { p.health };
    draw_member(cv, p, PET_W, fill, p.health_bg, "Nightsister Rancor", "85", "12040", 0.7, 0.85, "6m", false, false);
    let cell = s.pet_icon_size as f32;
    draw_icon_grid(cv, 0.0, BODY_H + 2.0, (PET_W / cell).floor() as u32, 1, cell, 2, Rgb(0x3A, 0x7B, 0xFF));
}

/// Grey "unknown health" variant of the target frame (non-attackable targets).
pub fn draw_target_grey(cv: &Canvas, p: &Palette) {
    frame_shell(cv);
    bar(cv, INNER_X, HEALTH_Y, INNER_W, HEALTH_H, GREY, GREY_BG, 1.0, 0.0);
    bar(cv, INNER_X, POWER_Y, INNER_W, POWER_H, p.power, p.power_bg, 1.0, 24.0);
    cv.text(INNER_X + 2.0 + 32.0, HEALTH_Y, 120.0, HEALTH_H, Align2::LEFT_CENTER, 13.0, TEXT, "Junk Dealer");
    faction_badge(cv);
}

/// Buff / debuff icon grid as the PlayerBuffs / PlayerDebuffs windows lay it out.
pub fn draw_icon_grid(cv: &Canvas, x: f32, y: f32, cols: u32, rows: u32, cell: f32, filled: u32, tint: Rgb) {
    let mut n = 0;
    for r in 0..rows {
        for c in 0..cols {
            let cx = x + c as f32 * cell;
            let cy = y + r as f32 * cell;
            if n < filled {
                cv.fill(cx + 1.0, cy + 1.0, cell - 2.0, cell - 2.0, tint);
                cv.outline(cx + 1.0, cy + 1.0, cell - 2.0, cell - 2.0, Rgb(0, 0, 0));
            } else {
                cv.fill_a(cx + 1.0, cy + 1.0, cell - 2.0, cell - 2.0, Rgb(0xFF, 0xFF, 0xFF), 12);
            }
            n += 1;
        }
    }
}

/// Overhead nameplate.  Returns its height in frame pixels.
pub fn draw_nameplate(cv: &Canvas, p: &Palette, s: &Sample, center_x: f32) -> f32 {
    let mut y = 0.0;
    // distance / status line
    cv.text(center_x - 100.0, y, 200.0, 13.0, Align2::CENTER_CENTER, 11.0, TEXT_DIM, s.np_range);
    y += 13.0;
    cv.text(center_x - 150.0, y, 300.0, 17.0, Align2::CENTER_CENTER, 14.0, TEXT, s.np_name);
    y += 17.0;

    // ham row: faction 14 | spacer 1 | bracket 12 | bars 122 | bracket 12 | spacer 1 | con 16, 1px gaps
    let total_h = 1.0 + NP_H_H + 1.0 + NP_A_H + 1.0; // 18
    let row_w = 14.0 + 1.0 + 12.0 + 12.0 + 1.0 + 16.0 + NP_W + 2.0 + 6.0;
    let mut x = center_x - row_w / 2.0;
    cv.icon(x, (total_h - 14.0) / 2.0 + y, 14.0, Rgb(0xC8, 0xC8, 0xC8)); // faction glyph
    x += 14.0 + 1.0 + 1.0 + 1.0; // + spacer + gaps
    // elite bracket (left)
    cv.text(x, y, 12.0, total_h, Align2::CENTER_CENTER, 14.0, ELITE, "[");
    x += 12.0 + 1.0;
    // bars: black page = border
    cv.fill(x, y, NP_W + 2.0, total_h, Rgb(0, 0, 0));
    bar(cv, x + 1.0, y + 1.0, NP_W, NP_H_H, p.target, p.target_bg, s.np_frac, 12.0);
    cv.text(x + 1.0, y + 1.0, NP_W, NP_H_H, Align2::CENTER_CENTER, 11.0, TEXT, s.np_hp);
    bar(cv, x + 1.0, y + 1.0 + NP_H_H + 1.0, NP_W, NP_A_H, p.power, p.power_bg, 0.8, 12.0);
    x += NP_W + 2.0 + 1.0;
    cv.text(x, y, 12.0, total_h, Align2::CENTER_CENTER, 14.0, ELITE, "]");
    x += 12.0 + 1.0 + 1.0 + 1.0;
    // con glyph
    let tri = [
        cv.r(x + 4.0, y + 4.0, 8.0, 8.0).left_bottom(),
        cv.r(x + 4.0, y + 4.0, 8.0, 8.0).right_bottom(),
        cv.r(x + 4.0, y + 4.0, 8.0, 8.0).center_top(),
    ];
    cv.painter.add(egui::Shape::convex_polygon(tri.to_vec(), Color32::from_rgb(0xFF, 0xD0, 0x40), Stroke::NONE));
    y += total_h;

    cv.text(center_x - 150.0, y, 300.0, 14.0, Align2::CENTER_CENTER, 12.0, TEXT_DIM, s.np_title);
    y += 14.0;
    y
}

/// Lay out every frame in one scrollable area and return the size used.
pub fn draw_all(ui: &mut Ui, s: &Settings, preview_class: &str, scale: f32) {
    let p = Palette::from(s, preview_class);
    let sample = &SAMPLE;
    let cell = s.buff_icon_size as f32;
    let gap = 18.0; // frame-space gap between frames

    // total height in frame space
    let buff_h = if s.inline_buffs {
        2.0 * cell + 2.0 // stock: buff row + debuff row under the frame
    } else {
        s.buff_rows as f32 * cell + 6.0 + s.debuff_rows as f32 * cell
    };
    let grid_w = (s.buff_columns.max(s.debuff_columns) as f32 * cell).max(FRAME_W);
    let np_h = 13.0 + 17.0 + 18.0 + 14.0;
    let group_h = GROUP_SAMPLE.len() as f32 * GROUP_ROW_H;
    let total_h = 14.0 + BODY_H + 4.0 + buff_h + gap
        + 14.0 + BODY_H + gap
        + 14.0 + BODY_H + gap
        + 14.0 + group_h + gap
        + 14.0 + PET_H + gap
        + 14.0 + np_h + gap
        + 14.0 + BODY_H;
    let total_w = grid_w.max(FRAME_W) + 8.0;

    let (resp, painter) = ui.allocate_painter(Vec2::new(total_w * scale, total_h * scale), egui::Sense::hover());
    let origin = resp.rect.min;
    let label = |y: f32, text: &str| {
        painter.text(
            origin + Vec2::new(0.0, y * scale),
            Align2::LEFT_TOP,
            text,
            FontId::proportional(12.0),
            ui.visuals().weak_text_color(),
        );
    };

    let mut y = 0.0;
    label(y, "Player frame");
    y += 14.0;
    let cv = Canvas::new(&painter, origin + Vec2::new(0.0, y * scale), scale);
    draw_player(&cv, &p, sample);
    y += BODY_H + 4.0;
    let cv = Canvas::new(&painter, origin + Vec2::new(0.0, y * scale), scale);
    if s.inline_buffs {
        draw_icon_grid(&cv, 0.0, 0.0, 12, 1, cell, 5, Rgb(0x3A, 0x7B, 0xFF));
        draw_icon_grid(&cv, 0.0, cell + 2.0, 12, 1, cell, 2, Rgb(0xC0, 0x30, 0x30));
    } else {
        let (bx, by) = Settings::parse_loc(&s.buff_location).unwrap_or((0, 0));
        let (dx, dy) = s.debuff_loc().unwrap_or((0, 0));
        draw_icon_grid(&cv, 0.0, 0.0, s.buff_columns, s.buff_rows, cell, 7, Rgb(0x3A, 0x7B, 0xFF));
        cv.text(0.0, s.buff_rows as f32 * cell - 2.0, 300.0, 10.0, Align2::LEFT_CENTER, 8.0, TEXT_DIM,
                &format!("PlayerBuffs window at {bx},{by}"));
        let dyy = s.buff_rows as f32 * cell + 6.0;
        draw_icon_grid(&cv, 0.0, dyy, s.debuff_columns, s.debuff_rows, cell, 2, Rgb(0xC0, 0x30, 0x30));
        cv.text(0.0, dyy + s.debuff_rows as f32 * cell - 2.0, 300.0, 10.0, Align2::LEFT_CENTER, 8.0, TEXT_DIM,
                &format!("PlayerDebuffs window at {dx},{dy}"));
    }
    y += buff_h + gap;

    label(y, "Target frame");
    y += 14.0;
    let cv = Canvas::new(&painter, origin + Vec2::new(0.0, y * scale), scale);
    draw_target(&cv, &p, sample);
    y += BODY_H + gap;

    label(y, "Target of target (look-at target)");
    y += 14.0;
    let cv = Canvas::new(&painter, origin + Vec2::new(0.0, y * scale), scale);
    draw_tot(&cv, &p, sample);
    y += BODY_H + gap;

    label(y, "Group window");
    y += 14.0;
    let cv = Canvas::new(&painter, origin + Vec2::new(0.0, y * scale), scale);
    draw_group(&cv, &p, s);
    y += group_h + gap;

    label(y, "Pet frame");
    y += 14.0;
    let cv = Canvas::new(&painter, origin + Vec2::new(0.0, y * scale), scale);
    draw_pet(&cv, &p, s);
    y += PET_H + gap;

    label(y, "Nameplate (overhead)");
    y += 14.0;
    let cv = Canvas::new(&painter, origin + Vec2::new(0.0, y * scale), scale);
    draw_nameplate(&cv, &p, sample, FRAME_W / 2.0);
    y += np_h + gap;

    label(y, "Target frame, non-attackable (grey)");
    y += 14.0;
    let cv = Canvas::new(&painter, origin + Vec2::new(0.0, y * scale), scale);
    draw_target_grey(&cv, &p);
}

/// Common display resolutions, up to 8K.
pub const RESOLUTIONS: [(&str, u32, u32); 14] = [
    ("1280 x 720  (16:9)", 1280, 720),
    ("1366 x 768  (16:9)", 1366, 768),
    ("1600 x 900  (16:9)", 1600, 900),
    ("1920 x 1080  (16:9)", 1920, 1080),
    ("1920 x 1200  (16:10)", 1920, 1200),
    ("2560 x 1080  (21:9)", 2560, 1080),
    ("2560 x 1440  (16:9)", 2560, 1440),
    ("2560 x 1600  (16:10)", 2560, 1600),
    ("3440 x 1440  (21:9)", 3440, 1440),
    ("3840 x 1600  (24:10)", 3840, 1600),
    ("3840 x 2160  (16:9, 4K)", 3840, 2160),
    ("5120 x 1440  (32:9)", 5120, 1440),
    ("5120 x 2880  (16:9, 5K)", 5120, 2880),
    ("7680 x 4320  (16:9, 8K)", 7680, 4320),
];

/// Parse a "WxH" string.
pub fn parse_resolution(s: &str) -> Option<(u32, u32)> {
    let key = s.replace(' ', "").to_ascii_lowercase();
    let (w, h) = key.split_once('x')?;
    let (w, h) = (w.parse::<u32>().ok()?, h.parse::<u32>().ok()?);
    (w >= 320 && h >= 240).then_some((w, h))
}

/// Index into RESOLUTIONS for a "WxH" string; None when it is not a listed size.
pub fn resolution_index(s: &str) -> Option<usize> {
    let (w, h) = parse_resolution(s)?;
    RESOLUTIONS.iter().position(|(_, rw, rh)| *rw == w && *rh == h)
}

/// The interface scale most people run at a given height (the client's
/// Options > Interface slider); only a starting point.
pub fn suggested_ui_scale(height: u32) -> f64 {
    match height {
        h if h >= 4320 => 4.0,
        h if h >= 2880 => 3.0,
        h if h >= 2160 => 2.25,
        h if h >= 1440 => 1.5,
        _ => 1.0,
    }
}

/// Drag a window rect in UI space; returns the new top-left when it moved.
fn drag_window(ui: &mut Ui, id: egui::Id, rect: Rect, scale: f32, cur: (i32, i32), bounds: (f32, f32)) -> Option<(i32, i32)> {
    let resp = ui.interact(rect, id, egui::Sense::drag());
    let resp = resp.on_hover_cursor(egui::CursorIcon::Grab)
        .on_hover_text(format!("drag to move  ({}, {})", cur.0, cur.1));
    if resp.drag_started() {
        ui.data_mut(|d| d.insert_temp(id, (cur.0 as f32, cur.1 as f32)));
    }
    if resp.dragged() {
        let mut pos: (f32, f32) = ui.data(|d| d.get_temp(id)).unwrap_or((cur.0 as f32, cur.1 as f32));
        let d = resp.drag_delta() / scale;
        pos.0 = (pos.0 + d.x).clamp(-rect.width() / scale + 8.0, bounds.0 - 8.0);
        pos.1 = (pos.1 + d.y).clamp(-rect.height() / scale + 8.0, bounds.1 - 8.0);
        ui.data_mut(|d| d.insert_temp(id, pos));
        let np = (pos.0.round() as i32, pos.1.round() as i32);
        if np != cur {
            return Some(np);
        }
    }
    None
}

/// The whole screen at `res`, drawn `zoom` x the size that fits the panel width.
/// Frames are laid out in the client's UI coordinate space (screen / UI scale)
/// at the stock bottom-centre positions; the buff windows use their configured
/// coordinates, so this shows where they land at each resolution.
pub fn draw_screen(ui: &mut Ui, s: &mut Settings, preview_class: &str, res: (u32, u32), ui_scale: f32, zoom: f32, width: f32) {
    let p = Palette::from(s, preview_class);
    let sample = &SAMPLE;
    let ui_scale = ui_scale.max(0.25);
    let (sw, sh) = (res.0 as f32, res.1 as f32);
    let (uw, uh) = (sw / ui_scale, sh / ui_scale); // UI-space canvas

    let px_w = width.max(200.0) * zoom;
    let px_h = px_w * sh / sw;
    let scale = px_w / uw; // UI px -> screen px

    let (resp, painter) = ui.allocate_painter(Vec2::new(px_w, px_h), egui::Sense::hover());
    let origin = resp.rect.min;
    painter.rect_filled(resp.rect, 0.0, Color32::from_rgb(0x23, 0x2A, 0x2E));
    // a faint centre cross helps read the layout
    let cross = Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 14);
    painter.line_segment(
        [Pos2::new(resp.rect.center().x, resp.rect.top()), Pos2::new(resp.rect.center().x, resp.rect.bottom())],
        Stroke::new(1.0, cross),
    );
    painter.line_segment(
        [Pos2::new(resp.rect.left(), resp.rect.center().y), Pos2::new(resp.rect.right(), resp.rect.center().y)],
        Stroke::new(1.0, cross),
    );
    painter.text(
        origin + Vec2::new(6.0, 4.0),
        Align2::LEFT_TOP,
        format!("{} x {}  |  UI scale {ui_scale:.2}  ->  {:.0} x {:.0} UI px", res.0, res.1, uw, uh),
        FontId::proportional(11.0),
        Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 120),
    );

    let at = |x: f32, y: f32| Canvas::new(&painter, origin + Vec2::new(x * scale, y * scale), scale);

    // stock-like bottom row: player left of centre, target right of it, look-at target beside
    let row_y = (uh * 0.74).round();
    let player_x = (uw * 0.5 - FRAME_W - 12.0).round();
    let target_x = (uw * 0.5 + 12.0).round();
    let tot_x = target_x + FRAME_W + 8.0;
    draw_player(&at(player_x, row_y), &p, sample);
    draw_target(&at(target_x, row_y), &p, sample);
    draw_tot(&at(tot_x, row_y), &p, sample);

    let cell = s.buff_icon_size as f32;
    if s.inline_buffs {
        let cv = at(player_x, row_y + BODY_H + 2.0);
        draw_icon_grid(&cv, 0.0, 0.0, 12, 1, cell, 5, Rgb(0x3A, 0x7B, 0xFF));
        draw_icon_grid(&cv, 0.0, cell + 2.0, 12, 1, cell, 2, Rgb(0xC0, 0x30, 0x30));
    } else {
        let (bx, by) = Settings::parse_loc(&s.buff_location).unwrap_or((0, 0));
        let (dx, dy) = s.debuff_loc().unwrap_or((0, 0));
        let (bw, bh) = (s.buff_columns as f32 * cell, s.buff_rows as f32 * cell);
        let (dw, dh) = (s.debuff_columns as f32 * cell, s.debuff_rows as f32 * cell);
        let blue = Rgb(0x3A, 0x7B, 0xFF);
        let red = Rgb(0xC0, 0x30, 0x30);

        let brect = Rect::from_min_size(origin + Vec2::new(bx as f32 * scale, by as f32 * scale), Vec2::new(bw * scale, bh * scale));
        let drect = Rect::from_min_size(origin + Vec2::new(dx as f32 * scale, dy as f32 * scale), Vec2::new(dw * scale, dh * scale));
        // drag handling first so the drawn position is this frame's
        if let Some((nx, ny)) = drag_window(ui, ui.id().with("buffs"), brect, scale, (bx, by), (uw, uh)) {
            s.buff_location = format!("{nx},{ny}");
        }
        if let Some((nx, ny)) = drag_window(ui, ui.id().with("debuffs"), drect, scale, (dx, dy), (uw, uh)) {
            s.debuff_location = format!("{nx},{ny}");
        }
        let (bx, by) = Settings::parse_loc(&s.buff_location).unwrap_or((0, 0));
        let (dx, dy) = s.debuff_loc().unwrap_or((0, 0));

        let cv = at(bx as f32, by as f32);
        cv.fill_a(0.0, 0.0, bw, bh, blue, 28);
        cv.outline(0.0, 0.0, bw, bh, blue);
        draw_icon_grid(&cv, 0.0, 0.0, s.buff_columns, s.buff_rows, cell, 7, blue);
        cv.text(2.0, bh, 200.0, 10.0, Align2::LEFT_CENTER, 8.0, TEXT_DIM, &format!("buffs {bx},{by}"));
        let cv = at(dx as f32, dy as f32);
        cv.fill_a(0.0, 0.0, dw, dh, red, 28);
        cv.outline(0.0, 0.0, dw, dh, red);
        draw_icon_grid(&cv, 0.0, 0.0, s.debuff_columns, s.debuff_rows, cell, 2, red);
        cv.text(2.0, dh, 200.0, 10.0, Align2::LEFT_CENTER, 8.0, TEXT_DIM, &format!("debuffs {dx},{dy}"));
    }

    // group window at the client's default spot (left edge, below the top), pet by the player frame
    draw_group(&at(4.0, (uh * 0.18).round()), &p, s);
    draw_pet(&at(player_x - PET_W - 8.0, row_y), &p, s);

    // an overhead plate roughly where a target stands
    draw_nameplate(&at(0.0, (uh * 0.30).round()), &p, sample, (uw * 0.5).round());
}
