#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Dodgin's UI mod - installer and live preview for the ElvUI-style SWG
//! unitframes (player, target, target-of-target, group, pet, overhead
//! nameplates) and action bars (toolbar, double toolbar, pet bar).
//!
//! Single exe: the templates and the stock stylesheet are embedded, and
//! settings + install manifest live next to the executable.

mod cfg;
mod import;
mod install;
mod keymap;
mod preview;
mod settings;
mod templates;

use eframe::egui::{self, Color32, RichText};
use install::Manifest;
use preview::KeyLabels;
use settings::{Rgb, Settings, PROFESSIONS};
use std::path::PathBuf;

fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

struct App {
    dir: PathBuf,
    s: Settings,
    saved: Settings,
    preview_class: String,
    scale: f32,
    screen_zoom: f32,
    show_closeups: bool,
    log: Vec<(bool, String)>,
    confirm_remove: bool,
    force_remove: bool,
    installed: Option<Manifest>,
    /// the character keymap the action bar labels come from (path, parsed), or why there is none
    keymap: Result<(PathBuf, keymap::Keymap), String>,
    characters: Vec<keymap::Character>,
    /// preview: show the double toolbar (the client's useDoubleToolbar option)
    preview_double: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let dir = app_dir();
        let fresh = !Settings::path(&dir).is_file();
        let s = Settings::load(&dir);
        let preview_class = if s.profession.is_empty() { "jedi".into() } else { s.profession_key().into() };
        let mut app = App {
            installed: Manifest::load(&dir),
            keymap: Err(String::new()),
            characters: Vec::new(),
            preview_double: cfg::use_double_toolbar(std::path::Path::new(&s.game_dir)).unwrap_or(false),
            saved: s.clone(),
            s,
            preview_class,
            scale: 2.0,
            screen_zoom: 1.0,
            show_closeups: true,
            log: Vec::new(),
            confirm_remove: false,
            force_remove: false,
            dir,
        };
        if fresh {
            app.import_from_game(&cc.egui_ctx);
        }
        app.reload_keymap();
        app
    }

    /// Re-read the keymap the labels come from (settings' file, else the newest).
    fn reload_keymap(&mut self) {
        let gd = std::path::Path::new(&self.s.game_dir);
        self.characters = keymap::find_characters(gd);
        self.keymap = keymap::for_settings(gd, &self.s.keymap_file);
    }

    fn key_labels(&self) -> KeyLabels {
        KeyLabels::from_keymap(self.keymap.as_ref().ok().map(|(_, k)| k))
    }

    /// Everything from the game folder in one go: resolution and UI scale from
    /// the cfg chain, the buff icon sliders, then the installed pages (if any)
    /// measured in the client's icon cells - so the preview shows what is on
    /// screen and Install writes it back the same size.
    fn import_from_game(&mut self, ctx: &egui::Context) {
        if !install::is_game_dir(&self.s.game_dir) {
            self.push_log(false, "import: not a SWG Legends directory");
            return;
        }
        self.detect_from_game(ctx);
        self.reload_keymap();
        self.preview_double = cfg::use_double_toolbar(std::path::Path::new(&self.s.game_dir)).unwrap_or(self.preview_double);
        let gd = std::path::PathBuf::from(&self.s.game_dir);
        let cell = self.client_icon_size();
        match import::import_installed(&gd, &mut self.s, cell) {
            Ok(r) => self.push_log(true, format!("imported installed layout: {}", r.applied.join(", "))),
            Err(e) => self.push_log(false, format!("import: {e} (nothing installed yet; using current settings)")),
        }
    }

    /// The buff icon size the client is using, if its options file says.
    fn client_icon_size(&self) -> Option<u32> {
        cfg::buff_icon_sizes(std::path::Path::new(&self.s.game_dir)).map(|(s, _, _, _)| s)
    }

    /// Keep the icon size in step with the game's slider so the windows are
    /// sized in the cells the client will actually draw.
    fn sync_icon_size(&mut self) {
        if let Some((s, pet, group, _)) = cfg::buff_icon_sizes(std::path::Path::new(&self.s.game_dir)) {
            self.s.buff_icon_size = s;
            self.s.pet_icon_size = pet;
            self.s.group_icon_size = group;
        }
    }

    /// Pre-fill the preview's resolution and UI scale from the client's config
    /// chain (client.cfg -> options.cfg -> custom.cfg ...).  With borderless
    /// window on, the client uses the desktop size, so ask the OS for it.
    fn detect_from_game(&mut self, ctx: &egui::Context) {
        if !install::is_game_dir(&self.s.game_dir) {
            self.push_log(false, "detect: not a SWG Legends directory");
            return;
        }
        let c = cfg::ClientCfg::load(std::path::Path::new(&self.s.game_dir));
        let mut found = Vec::new();
        if let Some(sc) = c.ui_scale() {
            self.s.ui_scale = sc;
            found.push(format!("UI scale {sc:.2} (uiScalingFactor)"));
        }
        let desktop = ctx.input(|i| i.viewport().monitor_size).filter(|m| m.x > 100.0 && m.y > 100.0);
        let res = if c.borderless() {
            desktop.map(|m| ((m.x.round() as u32, m.y.round() as u32), "desktop size, borderless window"))
                .or_else(|| c.resolution().map(|r| (r, "screenWidth/Height")))
        } else {
            c.resolution().map(|r| (r, "screenWidth/Height"))
        };
        if let Some(((w, h), how)) = res {
            self.s.preview_resolution = format!("{w}x{h}");
            found.push(format!("{w} x {h} ({how})"));
        }
        if let Some((size, pet, group, how)) = cfg::buff_icon_sizes(std::path::Path::new(&self.s.game_dir)) {
            self.s.buff_icon_size = size;
            self.s.pet_icon_size = pet;
            self.s.group_icon_size = group;
            found.push(format!("buff icons {size} / pet {pet} / group {group} ({how})"));
        }
        if found.is_empty() {
            self.push_log(false, "detect: no resolution / uiScalingFactor in the client's cfg files");
        } else {
            self.push_log(true, format!("detected from game: {}", found.join(", ")));
        }
    }

    fn push_log(&mut self, ok: bool, line: impl Into<String>) {
        self.log.push((ok, line.into()));
        if self.log.len() > 200 {
            self.log.drain(..self.log.len() - 200);
        }
    }

    fn do_install(&mut self) {
        self.sync_icon_size();
        let r = match install::render(&self.s, None) {
            Ok(r) => r,
            Err(e) => {
                self.push_log(false, e);
                return;
            }
        };
        match (&r.keymap, &r.keymap_err) {
            (Some(k), _) => self.push_log(true, format!("action bar labels from {}", k.display())),
            (None, Some(e)) => self.push_log(false, format!("top-row and pet-bar labels left empty: {e}")),
            _ => {}
        }
        match install::install(&self.dir, &self.s, &r) {
            Ok(lines) => {
                for l in lines {
                    self.push_log(true, l);
                }
                self.saved = self.s.clone();
                self.installed = Manifest::load(&self.dir);
            }
            Err(e) => self.push_log(false, e),
        }
    }

    fn do_remove(&mut self) {
        match install::remove(&self.dir, Some(&self.s.game_dir.clone()), self.force_remove) {
            Ok(lines) => {
                for l in lines {
                    let ok = !l.contains("leaving it");
                    self.push_log(ok, l);
                }
                self.installed = Manifest::load(&self.dir);
            }
            Err(e) => self.push_log(false, e),
        }
    }

    fn do_render_to(&mut self, out: PathBuf) {
        match install::render(&self.s, None) {
            Ok(r) => {
                let mut n = 0;
                for (rel, bytes) in &r.files {
                    let p = out.join(rel);
                    if let Some(parent) = p.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    match std::fs::write(&p, bytes) {
                        Ok(()) => n += 1,
                        Err(e) => self.push_log(false, format!("{}: {e}", p.display())),
                    }
                }
                self.push_log(true, format!("rendered {n} file(s) to {} (nothing installed)", out.display()));
            }
            Err(e) => self.push_log(false, e),
        }
    }

    fn color_field(ui: &mut egui::Ui, label: &str, hex: &mut String, fallback: Rgb) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(label);
            let mut arr = Rgb::parse(hex).unwrap_or(fallback).arr();
            if ui.color_edit_button_srgb(&mut arr).changed() {
                *hex = Rgb::from_arr(arr).hex();
                changed = true;
            }
            let te = egui::TextEdit::singleline(hex).desired_width(72.0);
            let resp = ui.add(te);
            if resp.changed() {
                changed = true;
            }
            if Rgb::parse(hex).is_none() {
                ui.colored_label(Color32::from_rgb(0xFF, 0x70, 0x70), "#RRGGBB");
            }
        });
        changed
    }

    fn settings_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Dodgin's UI mod");
        ui.label(RichText::new("ElvUI-style unitframes and action bars for SWG Legends").weak());
        ui.add_space(6.0);

        ui.group(|ui| {
            ui.label(RichText::new("Game folder").strong());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.s.game_dir).desired_width(230.0));
                if ui.button("Browse…").clicked() {
                    if let Some(d) = rfd::FileDialog::new().set_title("Select the SWG Legends folder").pick_folder() {
                        self.s.game_dir = d.display().to_string();
                    }
                }
            });
            if install::is_game_dir(&self.s.game_dir) {
                ui.colored_label(Color32::from_rgb(0x80, 0xE0, 0x80), "SWG Legends directory found");
            } else {
                ui.colored_label(Color32::from_rgb(0xFF, 0x90, 0x60), "SWG Legends directory not found (no legends.cfg here)");
            }
        });

        ui.add_space(6.0);
        ui.group(|ui| {
            ui.label(RichText::new("Player health").strong());
            ui.checkbox(&mut self.s.role_colors, "Class colours by role icon (automatic per character)")
                .on_hover_text("Installs a loose ui_styles.inc with the 36 role-icon styles turned into solid class-colour swatches.\nRole icons in the group window / character sheet become coloured squares.\nRerun after a game patch that changes ui_styles.inc.");
            if self.s.role_colors {
                ui.horizontal(|ui| {
                    ui.label("Preview as");
                    egui::ComboBox::from_id_salt("preview_class")
                        .selected_text(class_label(&self.preview_class))
                        .show_ui(ui, |ui| {
                            for (k, l, _) in PROFESSIONS.iter() {
                                ui.selectable_value(&mut self.preview_class, k.to_string(), *l);
                            }
                        });
                });
            }
            ui.add_enabled_ui(!self.s.role_colors, |ui| {
                let mut custom = !self.s.health_color.is_empty();
                ui.horizontal(|ui| {
                    ui.label("Profession preset");
                    egui::ComboBox::from_id_salt("profession")
                        .selected_text(class_label(self.s.profession_key()))
                        .show_ui(ui, |ui| {
                            for (k, l, _) in PROFESSIONS.iter() {
                                let v = if *k == "green" { String::new() } else { k.to_string() };
                                ui.selectable_value(&mut self.s.profession, v, *l);
                            }
                        });
                    let sw = self.s.class_color(self.s.profession_key());
                    let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 14.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 2.0, Color32::from_rgb(sw.0, sw.1, sw.2));
                });
                if ui.checkbox(&mut custom, "Custom colour instead").changed() {
                    self.s.health_color = if custom { self.s.class_color(self.s.profession_key()).hex() } else { String::new() };
                }
                if custom {
                    Self::color_field(ui, "Health", &mut self.s.health_color, Rgb(0x4A, 0xAB, 0x4D));
                }
            });
            ui.horizontal(|ui| {
                ui.label("Class palette brightness");
                ui.add(egui::Slider::new(&mut self.s.class_color_mul, 0.05..=1.0).fixed_decimals(2));
            });
            ui.label(RichText::new("Scales every profession colour (1 = colors.txt as-is).").weak().small());
        });

        ui.add_space(6.0);
        ui.group(|ui| {
            ui.label(RichText::new("Colours").strong());
            Self::color_field(ui, "Target health", &mut self.s.target_health_color, Rgb(0xC7, 0x40, 0x40));
            Self::color_field(ui, "Action bar", &mut self.s.power_color, Rgb(0xFF, 0xD2, 0x00));
            ui.horizontal(|ui| {
                ui.label("Bar backdrop");
                ui.add(egui::Slider::new(&mut self.s.backdrop_mul, 0.0..=1.0).fixed_decimals(2));
            });
            ui.label(RichText::new("Backdrop = bar colour x this (ElvUI custom backdrop look).").weak().small());
        });

        ui.add_space(6.0);
        ui.group(|ui| {
            ui.label(RichText::new("Buff / debuff icons").strong());
            ui.checkbox(&mut self.s.inline_buffs, "Keep icon rows under the player frame (stock layout)");
            ui.horizontal(|ui| {
                ui.label("Icon size");
                match self.client_icon_size() {
                    Some(sz) => {
                        self.s.buff_icon_size = sz;
                        ui.label(RichText::new(format!("{sz}")).strong());
                        ui.label(RichText::new("from the game's Options > Interface > Buff Icon slider").weak().small());
                    }
                    None => {
                        ui.add(egui::DragValue::new(&mut self.s.buff_icon_size).range(5..=120));
                        ui.label(RichText::new("match Options > Interface > Buff Icon").weak().small());
                    }
                }
            });
            ui.add_enabled_ui(!self.s.inline_buffs, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Buffs");
                    ui.add(egui::DragValue::new(&mut self.s.buff_columns).range(1..=40).prefix("cols "));
                    ui.add(egui::DragValue::new(&mut self.s.buff_rows).range(1..=20).prefix("rows "));
                    ui.label("at");
                    ui.add(egui::TextEdit::singleline(&mut self.s.buff_location).desired_width(70.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Debuffs");
                    ui.add(egui::DragValue::new(&mut self.s.debuff_columns).range(1..=40).prefix("cols "));
                    ui.add(egui::DragValue::new(&mut self.s.debuff_rows).range(1..=20).prefix("rows "));
                    ui.label("at");
                    ui.add(egui::TextEdit::singleline(&mut self.s.debuff_location).desired_width(70.0))
                        .on_hover_text("leave empty to place it directly under the buff window");
                });
                let bad = Settings::parse_loc(&self.s.buff_location).is_none()
                    || (!self.s.debuff_location.trim().is_empty() && Settings::parse_loc(&self.s.debuff_location).is_none());
                if bad {
                    ui.colored_label(Color32::from_rgb(0xFF, 0x70, 0x70), "locations must be x,y screen pixels");
                }
            });
        });

        ui.add_space(6.0);
        ui.group(|ui| {
            ui.label(RichText::new("Action bars").strong());
            ui.checkbox(&mut self.s.action_bars, "ElvUI-style toolbar, double toolbar and pet bar")
                .on_hover_text("Replaces ui_ground_hud_toolbar_skinned.inc: 36px buttons 2px apart, each in a 1px black frame, icons zoomed to fill\nthe frame, keybind labels on every button, the pane number and pane buttons in a column at the left, the big\ndefault-attack button as one more slot at the right.  Off = the toolbar page is left alone (or put back).");
            ui.add_enabled_ui(self.s.action_bars, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Keybind labels");
                    ui.selectable_value(&mut self.s.action_bar_keybinds_inside, true, "top-right of the button")
                        .on_hover_text("ElvUI placement: the label sits over the icon's top-right corner and lets clicks through.");
                    ui.selectable_value(&mut self.s.action_bar_keybinds_inside, false, "row under the bar")
                        .on_hover_text("The stock placement, restyled: a 12px row of labels under the buttons (above the top row of the double toolbar).");
                });
                Self::color_field(ui, "Label text", &mut self.s.action_bar_key_color, Rgb(0xFF, 0xFF, 0xFF));
                ui.horizontal(|ui| {
                    ui.label("Labels from keymap");
                    let current = match &self.keymap {
                        Ok((p, _)) => {
                            let who = self.characters.iter().find(|c| c.inp.as_deref() == Some(p.as_path()));
                            let name = who.map(|c| c.label()).unwrap_or_else(|| p.display().to_string());
                            if self.s.keymap_file.trim().is_empty() { format!("last played: {name}") } else { name }
                        }
                        Err(e) => format!("none: {e}"),
                    };
                    let mut pick: Option<String> = None;
                    egui::ComboBox::from_id_salt("keymap").width(360.0).selected_text(current).show_ui(ui, |ui| {
                        if ui.selectable_label(self.s.keymap_file.trim().is_empty(), "last-played character (auto)").clicked() {
                            pick = Some(String::new());
                        }
                        for c in &self.characters {
                            let Some(p) = &c.inp else {
                                ui.add_enabled(false, egui::Button::new(c.label()).frame(false));
                                continue;
                            };
                            if ui.selectable_label(self.s.keymap_file == p.display().to_string(), c.label()).clicked() {
                                pick = Some(p.display().to_string());
                            }
                        }
                    });
                    if let Some(v) = pick {
                        self.s.keymap_file = v;
                        self.reload_keymap();
                    }
                });
                ui.label(RichText::new("The client labels slots 1-12 itself; the double toolbar's top row and the pet bar get the keys bound in this character's keymap (profiles\\...\\<id>.inp). Characters are listed by ID with when they last played (the files carry no names); the client only saves a keymap once a binding is changed, so a character still on a stock preset has none. Reinstall after rebinding.").weak().small());
                ui.checkbox(&mut self.s.action_bar_backdrop, "Backdrop panel behind the buttons");
                ui.add_enabled_ui(self.s.action_bar_backdrop, |ui| {
                    Self::color_field(ui, "Backdrop", &mut self.s.action_bar_backdrop_color, Rgb(0x0F, 0x0F, 0x0F));
                    ui.horizontal(|ui| {
                        ui.label("Backdrop opacity");
                        ui.add(egui::Slider::new(&mut self.s.action_bar_backdrop_opacity, 0.0..=1.0).fixed_decimals(2));
                    });
                });
                ui.checkbox(&mut self.s.action_bar_queue_bar, "Queue timer bar along the top")
                    .on_hover_text("The client's throttle bar: a strip in the action bar colour that fills while a queued ability waits\nfor the previous one to finish.  Off hides it.");
                ui.label(RichText::new("Bars sit wherever you drag them in game.").weak().small());
            });
        });

        ui.add_space(8.0);
        let dirty = self.s != self.saved;
        ui.horizontal(|ui| {
            let can = install::is_game_dir(&self.s.game_dir) && self.s.health().is_ok()
                && self.s.target_health().is_ok() && self.s.power().is_ok() && self.s.debuff_loc().is_ok()
                && self.s.action_bar_backdrop().is_ok() && self.s.action_bar_key().is_ok();
            let label = if self.installed.is_some() { "Reinstall" } else { "Install" };
            if ui.add_enabled(can, egui::Button::new(RichText::new(label).strong())).clicked() {
                self.do_install();
            }
            if ui.add_enabled(self.installed.is_some() || install::is_game_dir(&self.s.game_dir), egui::Button::new("Remove")).clicked() {
                self.confirm_remove = true;
            }
            if ui.button("Save settings").clicked() {
                match self.s.save(&self.dir) {
                    Ok(()) => {
                        self.saved = self.s.clone();
                        self.push_log(true, "settings saved");
                    }
                    Err(e) => self.push_log(false, format!("settings: {e}")),
                }
            }
            if ui.button("Defaults").clicked() {
                let gd = self.s.game_dir.clone();
                self.s = Settings { game_dir: gd, ..Settings::default() };
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Render to folder…").on_hover_text("write the ui\\ files somewhere else; installs nothing").clicked() {
                if let Some(d) = rfd::FileDialog::new().set_title("Folder to render into").pick_folder() {
                    self.do_render_to(d);
                }
            }
            if dirty {
                ui.label(RichText::new("unsaved changes").weak());
            }
        });
        match &self.installed {
            Some(m) => {
                ui.label(RichText::new(format!("Installed: {} file(s) in {}", m.files.len(), m.game_dir)).small());
            }
            None => {
                ui.label(RichText::new("Not installed (by this app)").small().weak());
            }
        }
        ui.label(RichText::new("Safe to run with the game open; relog (character select and back) to load.").small().weak());

        ui.add_space(6.0);
        ui.separator();
        egui::ScrollArea::vertical().max_height(150.0).stick_to_bottom(true).show(ui, |ui| {
            for (ok, line) in &self.log {
                let c = if *ok { ui.visuals().text_color() } else { Color32::from_rgb(0xFF, 0x70, 0x70) };
                ui.label(RichText::new(line).color(c).small());
            }
        });
    }
}

/// The screen mockup, sized from the panel width so the canvas keeps the
/// display's aspect ratio (16:9 by default) instead of growing with the scroll area.
#[allow(clippy::too_many_arguments)]
fn preview_screen_block(ui: &mut egui::Ui, s: &mut Settings, class: &str, res: (u32, u32), ui_scale: f32, zoom: f32, width: f32, keys: &KeyLabels, double: bool) {
    let bg = egui::Frame::new().stroke(egui::Stroke::new(1.0, Color32::from_gray(70)));
    bg.show(ui, |ui| {
        preview::draw_screen(ui, s, class, res, ui_scale, zoom, width, keys, double);
    });
}

fn class_label(key: &str) -> &'static str {
    PROFESSIONS.iter().find(|(k, _, _)| *k == key).map(|(_, l, _)| *l).unwrap_or("Green (default)")
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("settings")
            .resizable(true)
            .default_width(360.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.settings_panel(ui));
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().slider_width = 110.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Preview").strong());
                ui.label("Resolution");
                let cur = preview::resolution_index(&self.s.preview_resolution);
                let name: String = match cur {
                    Some(i) => preview::RESOLUTIONS[i].0.to_string(),
                    None => format!("{}  (from game)", self.s.preview_resolution.replace('x', " x ")),
                };
                let mut pick: Option<usize> = None;
                egui::ComboBox::from_id_salt("resolution").selected_text(name).show_ui(ui, |ui| {
                    for (i, (n, _, _)) in preview::RESOLUTIONS.iter().enumerate() {
                        if ui.selectable_label(cur == Some(i), *n).clicked() {
                            pick = Some(i);
                        }
                    }
                });
                if let Some(i) = pick.filter(|i| Some(*i) != cur) {
                    let (_, w, h) = preview::RESOLUTIONS[i];
                    self.s.preview_resolution = format!("{w}x{h}");
                    self.s.ui_scale = preview::suggested_ui_scale(h);
                }
                if ui.button("Import from game").on_hover_text("read resolution, UI scale, the buff icon sliders and the installed pages back from the game folder").clicked() {
                    self.import_from_game(ui.ctx());
                }
                if false && ui.button("Detect from game").on_hover_text("read screenWidth/Height and uiScalingFactor from the client's cfg files, and the buff icon sliders from local_machine_options.iff").clicked() {
                    self.detect_from_game(ui.ctx());
                }
                ui.label("UI scale");
                ui.add(egui::DragValue::new(&mut self.s.ui_scale).range(0.5..=4.0).speed(0.05).fixed_decimals(2))
                    .on_hover_text("The client's interface scale (Options > Interface).  Window coordinates are UI pixels = screen pixels / scale, so 1202,0 at 225% on 4K is 70% of the way across.  Picking a resolution fills in the usual value; change it to match yours.");
            });
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.screen_zoom, 1.0..=6.0).step_by(0.5).text("screen zoom"));
                ui.separator();
                ui.checkbox(&mut self.preview_double, "double toolbar").on_hover_text("the client's Options > Interface > double toolbar setting (read from local_machine_options.iff)");
                ui.separator();
                ui.checkbox(&mut self.show_closeups, "close-ups");
                if self.show_closeups {
                    ui.add(egui::Slider::new(&mut self.scale, 1.0..=4.0).step_by(0.5).text("close-up zoom"));
                }
            });
            ui.label(RichText::new("Sample data; fonts and icon art stand in for the game's. Frames and bars sit wherever you drag them in game (stock bottom-centre layout shown). Drag the buff / debuff windows to set their coordinates.").weak().small());
            ui.separator();
            let (rw, rh) = preview::parse_resolution(&self.s.preview_resolution).unwrap_or((1920, 1080));
            let inner_w = ui.available_width() - 24.0;
            let ui_scale = self.s.ui_scale as f32;
            egui::ScrollArea::both().show(ui, |ui| {
                let keys = self.key_labels();
                preview_screen_block(ui, &mut self.s, &self.preview_class, (rw, rh), ui_scale, self.screen_zoom, inner_w, &keys, self.preview_double);
                if self.show_closeups {
                    ui.add_space(10.0);
                    ui.label(RichText::new("Close-ups").strong());
                    let bg = egui::Frame::new().fill(Color32::from_rgb(0x2A, 0x2E, 0x33)).inner_margin(12.0);
                    bg.show(ui, |ui| {
                        preview::draw_all(ui, &self.s, &self.preview_class, self.scale, &keys);
                    });
                }
            });
        });

        if self.confirm_remove {
            egui::Window::new("Remove the UI mod?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Puts the previous loose files back (or deletes ours if there were none).");
                    ui.checkbox(&mut self.force_remove, "Force: also replace files changed since install");
                    ui.horizontal(|ui| {
                        if ui.button("Remove").clicked() {
                            self.confirm_remove = false;
                            self.do_remove();
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_remove = false;
                        }
                    });
                });
        }
    }
}

/// Headless use: `--settings FILE` (default: settings.json next to the exe),
/// then one of `--render DIR` (write the ui\ files there, install nothing),
/// `--install`, `--remove [--force]`.
fn cli(args: &[String]) -> Result<(), String> {
    let dir = app_dir();
    let mut settings_path = None;
    let mut i = 0;
    let mut cmd: Vec<&str> = Vec::new();
    while i < args.len() {
        if args[i] == "--settings" {
            settings_path = args.get(i + 1).cloned();
            i += 2;
        } else {
            cmd.push(&args[i]);
            i += 1;
        }
    }
    let s = match settings_path {
        Some(p) => {
            let t = std::fs::read_to_string(&p).map_err(|e| format!("{p}: {e}"))?;
            serde_json::from_str::<Settings>(&t).map_err(|e| format!("{p}: {e}"))?
        }
        None => Settings::load(&dir),
    };
    match cmd.as_slice() {
        ["--render", out] => {
            let r = install::render(&s, None)?;
            for (rel, bytes) in &r.files {
                let p = PathBuf::from(out).join(rel);
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                std::fs::write(&p, bytes).map_err(|e| format!("{}: {e}", p.display()))?;
                eprintln!("wrote {}", p.display());
            }
            Ok(())
        }
        ["--install"] => {
            let r = install::render(&s, None)?;
            for l in install::install(&dir, &s, &r)? {
                eprintln!("{l}");
            }
            Ok(())
        }
        ["--remove"] | ["--remove", "--force"] => {
            for l in install::remove(&dir, Some(&s.game_dir), cmd.len() == 2)? {
                eprintln!("{l}");
            }
            Ok(())
        }
        ["--import"] => {
            let mut s2 = s.clone();
            let cell = cfg::buff_icon_sizes(std::path::Path::new(&s.game_dir)).map(|(c, _, _, _)| c);
            let r = import::import_installed(std::path::Path::new(&s.game_dir), &mut s2, cell)?;
            eprintln!("imported: {}", r.applied.join(", "));
            eprintln!("{}", serde_json::to_string_pretty(&s2).unwrap_or_default());
            Ok(())
        }
        ["--detect"] => {
            let gd = std::path::Path::new(&s.game_dir);
            let c = cfg::ClientCfg::load(gd);
            eprintln!("resolution: {:?} (borderless: {})", c.resolution(), c.borderless());
            eprintln!("ui scale:   {:?}", c.ui_scale());
            eprintln!("buff icons: {:?}", cfg::buff_icon_sizes(gd));
            Ok(())
        }
        _ => Err("usage: dodgins-ui-mod [--settings FILE] (--render DIR | --install | --remove [--force] | --detect | --import)".into()),
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        if let Err(e) = cli(&args) {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1060.0, 760.0])
            .with_min_inner_size([760.0, 520.0])
            .with_title("Dodgin's UI mod"),
        ..Default::default()
    };
    eframe::run_native("dodgins-ui-mod", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
