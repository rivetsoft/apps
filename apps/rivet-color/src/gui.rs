//! Rivet Color's window. On the left, the color being looked at: a big swatch, the color in
//! each format (click one to copy it), its contrast on white and black, and sliders to change
//! it. On the right, what goes with it: its shades, its harmonies, the saved palettes and the
//! colors picked lately. Clicking any swatch makes it the color being looked at; right-click
//! one to copy it, keep it in a palette or take it out.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::SystemTime;

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::live::{Choice, LiveLook};
use rivet_look::palette;
use rivet_look::theme::{Mode, r_md, r_sm};

use crate::color::{Format, Harmony, Rgb, parse};
use crate::store::{self, Palette};

pub const ICON_SVG: &str = include_str!("../data/rivet-color.svg");

/// Tints and shades each side of the color.
const SHADES: usize = 5;

fn c32(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

fn rgb(c: Color32) -> Rgb {
    Rgb(c.r(), c.g(), c.b())
}

/// Which list a swatch is in, for what its right-click menu offers.
#[derive(Clone, Copy, PartialEq, Eq)]
enum From {
    Elsewhere,
    History,
    /// The palette chosen (its place in `palettes`), with the color's place in it.
    Palette(usize),
}

/// Something a swatch asked for, done once the frame's drawing is over.
enum Do {
    Show(Rgb),
    Copy(Rgb),
    AddTo(usize, Rgb),
    NewPaletteWith(Rgb),
    Forget(Rgb),
    RemoveFrom(usize, usize),
}

pub struct ColorApp {
    look: LiveLook,
    /// Light or dark, and whether the desktop's colors are followed: always the desktop's,
    /// except in checks that ask for one (RIVET_COLOR_THEME=light or dark).
    choice: Choice,
    shot: Option<rivet_kit::Shot>,
    color: Rgb,
    /// The sliders' hue, saturation and lightness, kept apart from `color` so a grey keeps the
    /// hue it had and a slider doesn't jump while it's dragged.
    hsl: [f32; 3],
    typed: String,
    format: Format,
    history: Vec<Rgb>,
    history_seen: Option<SystemTime>,
    palettes: Vec<Palette>,
    palettes_seen: Option<SystemTime>,
    /// The palette shown: an index into `palettes`, or `palettes.len()` for the desktop's.
    chosen: usize,
    /// A new name being typed for the chosen palette.
    renaming: Option<String>,
    /// Asked once: clear the history, or delete the chosen palette?
    confirm_clear: bool,
    confirm_delete: bool,
    picking: bool,
    /// Set by a thread that looks at the files once a second, when the history or the palettes
    /// changed on disk (a pick from the panel, a palette changed elsewhere).
    files_changed: Arc<AtomicBool>,
    /// The line along the bottom, whether it's trouble, and when it was said.
    note: Option<(String, bool, f64)>,
    tx: Sender<Result<Option<Rgb>, String>>,
    rx: Receiver<Result<Option<Rgb>, String>>,
}

impl ColorApp {
    pub fn new(cc: &eframe::CreationContext, start: Option<Rgb>, screenshot: Option<PathBuf>) -> ColorApp {
        let (tx, rx) = channel();
        let shot = screenshot.map(rivet_kit::Shot::new);
        // The files' times first, then what's in them: a write in between is then seen as a change.
        let (history_seen, palettes_seen) = (store::changed(&store::history_path()), store::changed(&store::palettes_dir()));
        let (history, palettes, format) = if shot.is_some() { demo() } else { (store::load_history(), store::load_palettes(), store::load_format()) };
        let color = start.or(history.first().copied()).unwrap_or(Rgb(0xd5, 0x3f, 0x8c));
        let mut app = ColorApp {
            look: LiveLook::new(),
            choice: match std::env::var("RIVET_COLOR_THEME").as_deref() {
                Ok("light") => Choice { mode: Mode::Light, accent: [0xd5, 0x3f, 0x8c], follow_desktop_colors: false, text_size: 0.0 },
                Ok("dark") => Choice { mode: Mode::Dark, accent: [0xd5, 0x3f, 0x8c], follow_desktop_colors: false, text_size: 0.0 },
                _ => Choice { mode: Mode::System, accent: [0xd5, 0x3f, 0x8c], follow_desktop_colors: true, text_size: 0.0 },
            },
            shot,
            color,
            hsl: color.hsl(),
            typed: color.format(format),
            format,
            history,
            history_seen,
            palettes,
            palettes_seen,
            chosen: 0,
            renaming: None,
            confirm_clear: false,
            confirm_delete: false,
            picking: false,
            files_changed: Arc::new(AtomicBool::new(false)),
            note: None,
            tx,
            rx,
        };
        if app.shot.is_some() {
            // Checks show made-up history and palettes and save nothing. RIVET_COLOR_SCENE
            // picks another state: "empty" (no history or palettes), "desktop" (the desktop's
            // colors as the palette shown), "copied" (the note after copying).
            match std::env::var("RIVET_COLOR_SCENE").unwrap_or_default().as_str() {
                "empty" => {
                    app.history.clear();
                    app.palettes.clear();
                }
                "desktop" => app.chosen = app.palettes.len(),
                "copied" => app.note = Some((format!("Copied {}", app.color.format(app.format)), false, f64::MAX)),
                _ => {}
            }
        }
        if app.live() {
            watch_files(app.files_changed.clone(), (app.history_seen, app.palettes_seen), cc.egui_ctx.clone());
        }
        app
    }

    fn live(&self) -> bool {
        self.shot.is_none()
    }

    fn show(&mut self, c: Rgb) {
        self.color = c;
        self.hsl = c.hsl();
        self.typed = c.format(self.format);
    }

    fn say(&mut self, ctx: &egui::Context, text: String, trouble: bool) {
        self.note = Some((text, trouble, ctx.input(|i| i.time)));
    }

    fn copy(&mut self, ctx: &egui::Context, c: Rgb) {
        let text = c.format(self.format);
        ctx.copy_text(text.clone());
        self.remember(c);
        self.say(ctx, format!("Copied {text}"), false);
    }

    fn remember(&mut self, c: Rgb) {
        if self.live() {
            self.history_seen = store::changed(&store::history_path());
            self.history = store::remember(c);
        } else {
            self.history.retain(|x| *x != c);
            self.history.insert(0, c);
        }
    }

    fn pick(&mut self, ctx: &egui::Context) {
        if self.picking || !self.live() {
            return;
        }
        self.picking = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(crate::pick::pick());
            ctx.request_repaint();
        });
    }

    /// Picks that finished, and the files changed by the panel's plugin.
    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(answer) = self.rx.try_recv() {
            self.picking = false;
            match answer {
                Ok(Some(c)) => {
                    self.show(c);
                    self.copy(ctx, c);
                }
                Ok(None) => {}
                Err(e) => self.say(ctx, e, true),
            }
        }
        if self.files_changed.swap(false, Ordering::Relaxed) {
            let h = store::changed(&store::history_path());
            if h != self.history_seen {
                self.history_seen = h;
                self.history = store::load_history();
            }
            let p = store::changed(&store::palettes_dir());
            if p != self.palettes_seen {
                self.palettes_seen = p;
                self.palettes = store::load_palettes();
                self.chosen = self.chosen.min(self.palettes.len());
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let mut pasted = None;
        let copy = ctx.input(|i| {
            for e in &i.events {
                if let egui::Event::Paste(t) = e {
                    pasted = parse(t);
                }
            }
            i.events.iter().any(|e| matches!(e, egui::Event::Copy))
        });
        if copy {
            self.copy(ctx, self.color);
        }
        if let Some(c) = pasted {
            self.show(c);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::P)) {
            self.pick(ctx);
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            ui.spacing_mut().button_padding = Vec2::new(12.0, 6.0);
            let label = if self.picking { "Picking…" } else { "Pick from Screen" };
            let pick = egui::Button::new(egui::RichText::new(label).strong().color(c32(rgb(accent).ink()))).fill(accent);
            if ui.add_enabled(!self.picking, pick).on_hover_text("Click anywhere on the screen to take its color (P)").clicked() {
                self.pick(&ctx);
            }
            ui.add_space(6.0);
            let edit = ui.add(egui::TextEdit::singleline(&mut self.typed).desired_width(200.0).font(FontId::monospace(14.0)).hint_text("#d53f8c"));
            if edit.changed()
                && let Some(c) = parse(&self.typed)
            {
                self.color = c;
                self.hsl = c.hsl();
            }
            if edit.lost_focus() {
                self.typed = self.color.format(self.format);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Copy").on_hover_text("Ctrl+C").clicked() {
                    self.copy(&ctx, self.color);
                }
                let before = self.format;
                egui::ComboBox::from_id_salt("format").selected_text(self.format.name()).width(90.0).show_ui(ui, |ui| {
                    for f in Format::ALL {
                        ui.selectable_value(&mut self.format, f, f.name());
                    }
                });
                if self.format != before {
                    if self.live() {
                        store::save_format(self.format);
                    }
                    self.typed = self.color.format(self.format);
                }
                ui.label(egui::RichText::new("Copy as").color(ui.visuals().weak_text_color()));
            });
        });
    }

    /// The color being looked at, its codes, contrast and sliders.
    fn detail(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, 150.0), Sense::hover());
        ui.painter().rect(r, r_md(), c32(self.color), Stroke::new(1.0, palette::mix(v.panel_fill, v.text_color(), 0.15)), egui::StrokeKind::Inside);
        let ink = c32(self.color.ink());
        ui.painter().text(r.left_bottom() + Vec2::new(14.0, -12.0), Align2::LEFT_BOTTOM, self.color.hex(), FontId::monospace(20.0), ink);
        ui.add_space(10.0);
        for f in Format::ALL {
            let text = self.color.format(f);
            let (row, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(row, r_sm(), v.widgets.hovered.weak_bg_fill);
                ui.painter().text(row.right_center() - Vec2::new(10.0, 0.0), Align2::RIGHT_CENTER, "Copy", FontId::proportional(12.0), v.weak_text_color());
            }
            ui.painter().text(row.left_center() + Vec2::new(8.0, 0.0), Align2::LEFT_CENTER, f.name(), FontId::proportional(12.0), v.weak_text_color());
            ui.painter().text(row.left_center() + Vec2::new(64.0, 0.0), Align2::LEFT_CENTER, &text, FontId::monospace(13.0), v.text_color());
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                ctx.copy_text(text.clone());
                self.remember(self.color);
                self.say(&ctx, format!("Copied {text}"), false);
            }
        }
        ui.add_space(12.0);
        heading(ui, "Contrast");
        for (name, bg) in [("On white", Rgb(255, 255, 255)), ("On black", Rgb(0, 0, 0))] {
            let ratio = self.color.contrast(bg);
            let (row, _) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::hover());
            let chip = Rect::from_min_size(row.left_center() - Vec2::new(0.0, 12.0), Vec2::new(44.0, 24.0));
            ui.painter().rect(chip, r_sm(), c32(bg), Stroke::new(1.0, palette::mix(v.panel_fill, v.text_color(), 0.15)), egui::StrokeKind::Inside);
            ui.painter().text(chip.center(), Align2::CENTER_CENTER, "Aa", FontId::proportional(14.0), c32(self.color));
            ui.painter().text(chip.right_center() + Vec2::new(10.0, 0.0), Align2::LEFT_CENTER, name, FontId::proportional(13.0), v.text_color());
            let (grade, good) = match ratio {
                r if r >= 7.0 => ("AAA", true),
                r if r >= 4.5 => ("AA", true),
                r if r >= 3.0 => ("AA large text", true),
                _ => ("Too faint for text", false),
            };
            let words = format!("{ratio:.1}:1  {grade}");
            ui.painter().text(row.right_center(), Align2::RIGHT_CENTER, words, FontId::proportional(13.0), if good { v.text_color() } else { v.weak_text_color() });
        }
        ui.add_space(12.0);
        heading(ui, "Adjust");
        let [h, s, l] = self.hsl;
        let mut changed = false;
        changed |= gradient(ui, "Hue", &mut self.hsl[0], 360.0, |t| Rgb::from_hsl([t * 360.0, s.max(0.5), l.clamp(0.3, 0.7)]));
        changed |= gradient(ui, "Saturation", &mut self.hsl[1], 1.0, |t| Rgb::from_hsl([h, t, l]));
        changed |= gradient(ui, "Lightness", &mut self.hsl[2], 1.0, |t| Rgb::from_hsl([h, s, t]));
        if changed {
            self.color = Rgb::from_hsl(self.hsl);
            self.typed = self.color.format(self.format);
        }
    }

    /// Shades, harmonies, palettes and history, down the right.
    fn related(&mut self, ui: &mut egui::Ui, todo: &mut Vec<Do>) {
        let palettes = self.palettes.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
        heading(ui, "Shades");
        let shades = self.color.shades(SHADES);
        row(ui, &shades, Some(SHADES), From::Elsewhere, &palettes, todo);
        ui.add_space(16.0);
        heading(ui, "Harmonies");
        for h in Harmony::ALL {
            ui.label(egui::RichText::new(h.name()).size(12.0).color(ui.visuals().weak_text_color()));
            row(ui, &self.color.harmony(h), Some(0), From::Elsewhere, &palettes, todo);
            ui.add_space(6.0);
        }
        ui.add_space(10.0);
        self.palettes_section(ui, todo, &palettes);
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            heading(ui, "History");
            if !self.history.is_empty() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.confirm_clear {
                        if ui.button("Keep Them").clicked() {
                            self.confirm_clear = false;
                        }
                        if ui.button(format!("Clear {} Colors", self.history.len())).clicked() {
                            self.history.clear();
                            if self.live() {
                                store::save_history(&[]);
                            }
                            self.confirm_clear = false;
                        }
                    } else if ui.small_button("Clear History").clicked() {
                        self.confirm_clear = true;
                    }
                });
            }
        });
        if self.history.is_empty() {
            quiet(ui, "Colors you pick or copy show here.");
        } else {
            grid(ui, &self.history, From::History, &palettes, todo);
        }
    }

    fn palettes_section(&mut self, ui: &mut egui::Ui, todo: &mut Vec<Do>, names: &[String]) {
        let desktop = self.desktop_colors();
        let n = self.palettes.len();
        self.chosen = self.chosen.min(if desktop.is_empty() { n.saturating_sub(1) } else { n });
        ui.horizontal(|ui| {
            heading(ui, "Palettes");
            ui.add_space(6.0);
            let title = if self.chosen < n { self.palettes[self.chosen].name.clone() } else if !desktop.is_empty() { "Desktop colors".into() } else { "No palettes".into() };
            let before = self.chosen;
            egui::ComboBox::from_id_salt("palette").selected_text(title).width(170.0).show_ui(ui, |ui| {
                for (i, p) in self.palettes.iter().enumerate() {
                    ui.selectable_value(&mut self.chosen, i, &p.name);
                }
                if !desktop.is_empty() {
                    ui.selectable_value(&mut self.chosen, n, "Desktop colors");
                }
            });
            if self.chosen != before {
                self.renaming = None;
                self.confirm_delete = false;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("New Palette").on_hover_text("Start a palette with this color").clicked() {
                    todo.push(Do::NewPaletteWith(self.color));
                }
                if self.chosen < n {
                    if self.confirm_delete {
                        if ui.small_button("Keep It").clicked() {
                            self.confirm_delete = false;
                        }
                        if ui.small_button("Delete Palette").clicked() {
                            self.palettes[self.chosen].delete();
                            self.palettes.remove(self.chosen);
                            self.chosen = 0;
                            self.confirm_delete = false;
                        }
                    } else {
                        if ui.small_button("Delete").clicked() {
                            self.confirm_delete = true;
                        }
                        if ui.small_button("Rename").clicked() {
                            self.renaming = Some(self.palettes[self.chosen].name.clone());
                        }
                        if ui.small_button("Add Color").on_hover_text("Add this color to the palette").clicked() {
                            todo.push(Do::AddTo(self.chosen, self.color));
                        }
                    }
                }
            });
        });
        if let Some(name) = self.renaming.as_mut() {
            let mut done = false;
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(name).desired_width(220.0));
                r.request_focus();
                if r.lost_focus() || ui.button("Rename").clicked() {
                    done = true;
                }
            });
            if done {
                let name = self.renaming.take().unwrap_or_default();
                let live = self.live();
                let failed = match self.palettes.get_mut(self.chosen) {
                    Some(p) if live => p.rename(&name).err(),
                    Some(p) => {
                        p.name = name;
                        None
                    }
                    None => None,
                };
                if let Some(e) = failed {
                    self.say(ui.ctx(), format!("Couldn't rename the palette: {e}"), true);
                }
            }
        }
        ui.add_space(4.0);
        if self.chosen < n {
            let colors: Vec<Rgb> = self.palettes[self.chosen].colors.iter().map(|(c, _)| *c).collect();
            if colors.is_empty() {
                quiet(ui, "This palette is empty. Add Color puts the color on the left in it.");
            } else {
                grid(ui, &colors, From::Palette(self.chosen), names, todo);
            }
        } else if !desktop.is_empty() {
            grid(ui, &desktop, From::Elsewhere, names, todo);
        } else {
            quiet(ui, "No palettes yet. New Palette starts one with this color.");
        }
    }

    /// The desktop's own colors (the ones the wallpaper made, under Rivet Shell).
    fn desktop_colors(&self) -> Vec<Rgb> {
        let Some(p) = self.look.palette() else { return Vec::new() };
        let mut out: Vec<Rgb> = Vec::new();
        for c in [p.accent, p.secondary, p.tertiary, p.destructive, p.accent_fg, p.window_bg, p.view_bg, p.headerbar_bg, p.sidebar_bg, p.popover_bg, p.window_fg, p.view_fg].into_iter().flatten() {
            let c = rgb(c);
            if !out.contains(&c) {
                out.push(c);
            }
        }
        out
    }

    fn act(&mut self, ctx: &egui::Context, todo: Vec<Do>) {
        for d in todo {
            match d {
                Do::Show(c) => self.show(c),
                Do::Copy(c) => self.copy(ctx, c),
                Do::AddTo(i, c) => {
                    if let Some(p) = self.palettes.get_mut(i) {
                        if p.colors.iter().any(|(x, _)| *x == c) {
                            let name = p.name.clone();
                            self.say(ctx, format!("{} is already in {name}", c.hex()), false);
                            continue;
                        }
                        p.colors.push((c, String::new()));
                        let saved = if self.shot.is_some() { Ok(()) } else { p.save() };
                        let name = p.name.clone();
                        match saved {
                            Ok(()) => self.say(ctx, format!("Added {} to {name}", c.hex()), false),
                            Err(e) => self.say(ctx, format!("Couldn't save {name}: {e}"), true),
                        }
                    }
                }
                Do::NewPaletteWith(c) => {
                    let mut p = Palette { name: store::new_name(&self.palettes), colors: vec![(c, String::new())], path: PathBuf::new() };
                    if self.live()
                        && let Err(e) = p.save()
                    {
                        self.say(ctx, format!("Couldn't save the palette: {e}"), true);
                        continue;
                    }
                    let name = p.name.clone();
                    self.palettes.push(p);
                    self.palettes.sort_by_key(|p| p.name.to_lowercase());
                    self.chosen = self.palettes.iter().position(|p| p.name == name).unwrap_or(0);
                    self.renaming = Some(name);
                }
                Do::Forget(c) => {
                    self.history.retain(|x| *x != c);
                    if self.live() {
                        store::save_history(&self.history);
                    }
                }
                Do::RemoveFrom(i, at) => {
                    let live = self.live();
                    if let Some(p) = self.palettes.get_mut(i)
                        && at < p.colors.len()
                    {
                        p.colors.remove(at);
                        if live {
                            let _ = p.save();
                        }
                    }
                }
            }
        }
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        let v = ui.visuals().clone();
        let (text, color) = match &self.note {
            // A note fades after a few seconds; trouble stays until something else happens.
            Some((t, trouble, at)) if *trouble || *at == f64::MAX || now - at < 4.0 => {
                if !*trouble {
                    ui.ctx().request_repaint_after_secs((4.0 - (now - at)) as f32);
                }
                (t.clone(), if *trouble { v.error_fg_color } else { v.text_color() })
            }
            _ if self.picking => ("Click a color anywhere on the screen. Escape stops.".into(), v.text_color()),
            _ => {
                let count = |n: usize, one: &str, many: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {many}") };
                (format!("{} in the history · {}", count(self.history.len(), "color", "colors"), count(self.palettes.len(), "palette", "palettes")), v.weak_text_color())
            }
        };
        ui.label(egui::RichText::new(text).size(12.5).color(color));
    }
}

/// Look at the history file and the palettes folder once a second on a thread, and wake the
/// window only when one of them changed since `seen` (their times as the window read them);
/// stops when the window is gone.
fn watch_files(changed: Arc<AtomicBool>, mut seen: (Option<SystemTime>, Option<SystemTime>), ctx: egui::Context) {
    std::thread::spawn(move || {
        let look = || (store::changed(&store::history_path()), store::changed(&store::palettes_dir()));
        while Arc::strong_count(&changed) > 1 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let now = look();
            if now != seen {
                seen = now;
                changed.store(true, Ordering::Relaxed);
                ctx.request_repaint();
            }
        }
    });
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).strong().size(14.0));
    ui.add_space(2.0);
}

fn quiet(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(12.5).color(ui.visuals().weak_text_color()));
}

/// A slider drawn as the colors it runs through, with a ring where it is.
fn gradient(ui: &mut egui::Ui, name: &str, value: &mut f32, whole: f32, at: impl Fn(f32) -> Rgb) -> bool {
    let v = ui.visuals().clone();
    ui.label(egui::RichText::new(name).size(12.0).color(v.weak_text_color()));
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 18.0), Sense::click_and_drag());
    let bar = r.shrink2(Vec2::new(8.0, 2.0));
    let mut mesh = egui::Mesh::default();
    const STEPS: usize = 36;
    for i in 0..=STEPS {
        let t = i as f32 / STEPS as f32;
        let x = bar.left() + bar.width() * t;
        let c = c32(at(t));
        mesh.colored_vertex(Pos2::new(x, bar.top()), c);
        mesh.colored_vertex(Pos2::new(x, bar.bottom()), c);
        if i > 0 {
            let k = (i * 2) as u32;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k, k + 1);
        }
    }
    ui.painter().add(mesh);
    ui.painter().rect_stroke(bar, CornerRadius::same(2), Stroke::new(1.0, palette::mix(v.panel_fill, v.text_color(), 0.2)), egui::StrokeKind::Outside);
    let mut changed = false;
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        *value = ((p.x - bar.left()) / bar.width()).clamp(0.0, 1.0) * whole;
        changed = true;
    }
    let t = *value / whole;
    let c = Pos2::new(bar.left() + bar.width() * t, bar.center().y);
    ui.painter().circle(c, 8.0, c32(at(t)), Stroke::new(2.0, Color32::WHITE));
    ui.painter().circle_stroke(c, 9.5, Stroke::new(1.0, Color32::from_black_alpha(140)));
    ui.add_space(4.0);
    changed
}

/// One swatch: a rounded square of the color, which shows its code on hover.
fn swatch(ui: &mut egui::Ui, c: Rgb, size: Vec2, marked: bool) -> egui::Response {
    let v = ui.visuals().clone();
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let lift = if resp.hovered() { -2.0 } else { 0.0 };
    let r = r.translate(Vec2::new(0.0, lift));
    ui.painter().rect(r, r_sm(), c32(c), Stroke::new(1.0, palette::mix(v.panel_fill, v.text_color(), 0.15)), egui::StrokeKind::Inside);
    if marked {
        ui.painter().circle_filled(r.center(), 3.5, c32(c.ink()));
    }
    resp.on_hover_text(c.hex()).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// What a swatch's click and right-click menu ask for.
fn menu(resp: egui::Response, c: Rgb, from: From, at: usize, palettes: &[String], todo: &mut Vec<Do>) {
    if resp.clicked() {
        todo.push(Do::Show(c));
    }
    resp.context_menu(|ui| {
        if ui.button("Copy").clicked() {
            todo.push(Do::Copy(c));
        }
        ui.menu_button("Add to Palette", |ui| {
            for (i, name) in palettes.iter().enumerate() {
                if from != From::Palette(i) && ui.button(name).clicked() {
                    todo.push(Do::AddTo(i, c));
                }
            }
            if ui.button("New Palette").clicked() {
                todo.push(Do::NewPaletteWith(c));
            }
        });
        match from {
            From::History => {
                if ui.button("Remove from History").clicked() {
                    todo.push(Do::Forget(c));
                }
            }
            From::Palette(i) => {
                if ui.button("Remove from Palette").clicked() {
                    todo.push(Do::RemoveFrom(i, at));
                }
            }
            From::Elsewhere => {}
        }
    });
}

/// Swatches sharing the width in one row; `mark` is the one with a dot (the color itself).
fn row(ui: &mut egui::Ui, colors: &[Rgb], mark: Option<usize>, from: From, palettes: &[String], todo: &mut Vec<Do>) {
    let gap = 6.0;
    let w = ((ui.available_width() - gap * (colors.len() as f32 - 1.0)) / colors.len() as f32).min(96.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (i, c) in colors.iter().enumerate() {
            let resp = swatch(ui, *c, Vec2::new(w, 40.0), mark == Some(i));
            menu(resp, *c, from, i, palettes, todo);
        }
    });
}

/// Small swatches that wrap onto more lines.
fn grid(ui: &mut egui::Ui, colors: &[Rgb], from: From, palettes: &[String], todo: &mut Vec<Do>) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(6.0);
        for (i, c) in colors.iter().enumerate() {
            let resp = swatch(ui, *c, Vec2::splat(34.0), false);
            menu(resp, *c, from, i, palettes, todo);
        }
    });
}

/// Made-up history and palettes, for screenshots.
fn demo() -> (Vec<Rgb>, Vec<Palette>, Format) {
    let hex = |s: &[&str]| s.iter().filter_map(|h| parse(h)).collect::<Vec<_>>();
    let history = hex(&["#d53f8c", "#0084b8", "#3ecf9a", "#fff270", "#24121b", "#e07aab", "#40a3ca", "#f7eef3", "#ff7a59", "#7c5cff", "#1f8f66", "#992f64", "#9ad7ef", "#e9dee5"]);
    let pal = |name: &str, colors: &[&str]| Palette { name: name.into(), colors: hex(colors).into_iter().map(|c| (c, String::new())).collect(), path: PathBuf::new() };
    let palettes = vec![pal("Rivet website", &["#d53f8c", "#e07aab", "#f4b7d2", "#0084b8", "#40a3ca", "#9ad7ef", "#fff270", "#3ecf9a", "#24121b", "#f7eef3"]), pal("Sunset", &["#ff7a59", "#ffb347", "#ffd56b", "#c86b98", "#5b3a70"])];
    (history, palettes, Format::Hex)
}

impl eframe::App for ColorApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(shot) = &self.shot {
            raw.focused = true;
            rivet_kit::scripted_click("RIVET_COLOR", shot.frames(), raw);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let choice = self.choice;
        self.look.frame(&ctx, &choice, self.live());
        self.poll(&ctx);
        self.shortcuts(&ctx);
        let v = ctx.global_style().visuals.clone();
        let accent = self.look.accent(&choice);
        let bar = |top: i8, bottom: i8| egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 14, right: 14, top, bottom });
        egui::Panel::top("color-top").show_separator_line(false).frame(bar(10, 8)).show(ui, |ui| self.top_bar(ui, accent));
        egui::Panel::bottom("color-bottom").show_separator_line(false).frame(bar(4, 8)).show(ui, |ui| self.bottom_bar(ui));
        egui::Panel::left("color-detail").show_separator_line(false).resizable(false).exact_size(300.0).frame(bar(6, 6)).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("detail").show(ui, |ui| self.detail(ui));
        });
        let middle = egui::Frame::new().fill(palette::mix(v.panel_fill, v.extreme_bg_color, 0.5)).inner_margin(egui::Margin::same(14)).corner_radius(r_md());
        let mut todo = Vec::new();
        egui::CentralPanel::default().frame(middle).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("related").show(ui, |ui| self.related(ui, &mut todo));
        });
        self.act(&ctx, todo);
        if let Some(shot) = self.shot.as_mut() {
            shot.step(&ctx);
        }
    }
}
