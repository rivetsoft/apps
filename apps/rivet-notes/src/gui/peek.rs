//! Peek (F11, or the button at the top right): a panel on the right about the open note — how
//! it's shown (full width or centered, properties above the text), the file, its dates and its
//! properties.

use std::time::SystemTime;

use eframe::egui::{self, Align2, Color32, Sense, Vec2};
use rivet_look::theme::{r_md, r_xs};
use rivet_look::{fonts, palette};

use super::*;

/// The widest a centered note's text gets, in points.
pub(super) const CENTERED_WIDTH: f32 = 720.0;

impl NotesApp {
    /// The open note's backlinks, worked out again only when the note or the list changes.
    fn backlinks_now(&mut self) -> Vec<Backlink> {
        let Some(path) = self.doc.as_ref().map(|d| d.path.clone()) else { return Vec::new() };
        if let Some((p, g, list)) = &self.backlinks
            && *p == path
            && *g == self.notes_gen
        {
            return list.clone();
        }
        let list: Vec<Backlink> = vault::backlinks(&self.notes, &path).into_iter().map(|(n, lines)| (n.path.clone(), n.title.clone(), lines)).collect();
        self.backlinks = Some((path, self.notes_gen, list.clone()));
        list
    }

    pub(super) fn peek(&mut self, ui: &mut egui::Ui) {
        let back = self.backlinks_now();
        let full_icon = self.icon(ui.ctx(), &["rivet-full-width-symbolic", "format-justify-fill-symbolic"], 16.0);
        let center_icon = self.icon(ui.ctx(), &["rivet-centered-symbolic", "format-justify-center-symbolic"], 16.0);
        let mut go: Option<PathBuf> = None;
        let mut make: Option<String> = None;
        let v = ui.visuals().clone();
        let weak = v.weak_text_color();
        // No title or close button: the toolbar's Peek button and F11 open and close it.
        egui::ScrollArea::vertical().id_salt("notes-peek").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            // ---- how the note is shown
            // The width chosen here is this note's own; the default is in Preferences.
            let note = self.doc.as_ref().map(|d| d.path.clone());
            card(ui, "Display", |ui| {
                let before = self.s.clone();
                if let Some(note) = &note {
                    // Width: this note's own; Reset goes back to the default.
                    let own = self.s.note_width(note);
                    let mut centered = self.s.centered_for(note);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let was = centered;
                        icon_segment(ui, &mut centered, false, &full_icon, "Full width");
                        icon_segment(ui, &mut centered, true, &center_icon, "Centered");
                        if centered != was {
                            self.s.set_note_width(note, Some(centered));
                        }
                        if own.is_some() {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let default = if self.s.centered { "centered" } else { "full width" };
                                if ui.small_button("Reset").on_hover_text(format!("Back to the default ({default})")).clicked() {
                                    self.s.set_note_width(note, None);
                                }
                            });
                        }
                    });
                    ui.add_space(4.0);
                }
                ui.add_space(2.0);
                ui.checkbox(&mut self.s.show_properties, "Show properties");
                if self.s != before {
                    self.s.save();
                }
            });
            let Some(d) = self.doc.as_ref() else {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| ui.label(egui::RichText::new("Open a note to see its details").color(weak)));
                return;
            };
            let vault = self.vault.clone().unwrap_or_default();
            let meta = std::fs::metadata(&d.path).ok();
            // ---- links: the notes linking here, and where this note links to
            if !back.is_empty() {
                card(ui, &format!("Backlinks  {}", back.len()), |ui| {
                    for (p, title, lines) in &back {
                        // The line the link is on shows on hover.
                        let tip = lines.first().cloned().unwrap_or_default();
                        if link_row(ui, title, true).on_hover_text(tip).clicked() {
                            go = Some(p.clone());
                        }
                    }
                });
            }
            let mut outgoing: Vec<String> = Vec::new();
            for l in crate::links::outgoing(&d.body) {
                if !outgoing.iter().any(|o| o.eq_ignore_ascii_case(&l.target)) {
                    outgoing.push(l.target);
                }
            }
            if !outgoing.is_empty() {
            card(ui, &format!("Links  {}", outgoing.len()), |ui| {
                for t in &outgoing {
                    match vault::resolve(&self.notes, &d.path, t) {
                        Some(n) => {
                            if link_row(ui, &n.title, true).on_hover_text("Open this note").clicked() {
                                go = Some(n.path.clone());
                            }
                        }
                        None => {
                            if link_row(ui, t, false).on_hover_text("Not made yet").clicked() {
                                make = Some(t.clone());
                            }
                        }
                    }
                }
            });
            }
            // ---- the file
            card(ui, "File", |ui| {
                let name = d.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let folder = d.path.parent().and_then(|p| p.strip_prefix(&vault).ok()).map(|p| p.to_string_lossy().replace('\\', "/")).filter(|p| !p.is_empty()).unwrap_or_else(|| "Top of the vault".into());
                let words = d.body.split_whitespace().count();
                let chars = d.body.chars().count();
                let lines = d.body.lines().count();
                let minutes = words.div_ceil(220).max(1);
                rows(ui, &[
                    ("Name", name),
                    ("Folder", folder),
                    ("Size", meta.as_ref().map(|m| human_size(m.len())).unwrap_or_else(|| "—".into())),
                    ("Words", format!("{words}")),
                    ("Characters", format!("{chars}")),
                    ("Lines", format!("{lines}")),
                    ("Reading time", format!("About {minutes} min")),
                    ("Location", d.path.parent().map(|p| p.display().to_string()).unwrap_or_default()),
                ]);
            });
            // ---- dates
            card(ui, "Dates", |ui| {
                let edited = meta.as_ref().and_then(|m| m.modified().ok()).map(when).unwrap_or_else(|| "—".into());
                let created = d
                    .fm
                    .as_ref()
                    .and_then(|f| f.get("created").or_else(|| f.get("date")))
                    .map(|x| x.as_text())
                    .filter(|t| !t.is_empty())
                    .or_else(|| meta.as_ref().and_then(|m| m.created().ok()).map(when))
                    .unwrap_or_else(|| "—".into());
                let saved = if d.dirty { "Not yet (saving…)".to_string() } else { d.saved_at.clone().map(|t| format!("Today at {t}")).unwrap_or_else(|| "Unchanged since opened".into()) };
                rows(ui, &[("Edited", edited), ("Created", created), ("Saved", saved)]);
            });
            // ---- properties (the frontmatter), read-only here
            card(ui, "Properties", |ui| match d.fm.as_ref().filter(|f| f.fields.iter().any(|(k, _)| !crate::props::hidden(k))) {
                None => {
                    ui.label(egui::RichText::new("No properties").color(weak));
                }
                Some(fm) => {
                    for (k, val) in fm.fields.iter().filter(|(k, _)| !crate::props::hidden(k)) {
                        let is_tags = k.eq_ignore_ascii_case("tags") || k.eq_ignore_ascii_case("tag");
                        match val {
                            Value::List(items) if is_tags => {
                                ui.label(egui::RichText::new(k).color(weak));
                                if items.is_empty() {
                                    ui.label(egui::RichText::new("none").color(weak));
                                }
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                                    for t in items {
                                        chip(ui, &format!("#{t}"), palette::mix(v.faint_bg_color, v.selection.bg_fill, 0.35), v.text_color());
                                    }
                                });
                            }
                            _ => rows(ui, &[(k.as_str(), val.as_text())]),
                        }
                    }
                }
            });
        });
        if let Some(p) = go {
            self.open_note(&p);
        }
        if let Some(name) = make {
            let folder = self.doc.as_ref().and_then(|d| d.path.parent()).and_then(|p| self.vault.as_ref().and_then(|v| p.strip_prefix(v).ok())).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
            self.new_note(&folder, &name);
        }
    }
}

/// A note's name in Peek's link lists: link-colored when it's there, faint when it isn't yet.
fn link_row(ui: &mut egui::Ui, name: &str, exists: bool) -> egui::Response {
    let v = ui.visuals().clone();
    let col = if exists { v.hyperlink_color } else { v.weak_text_color() };
    let text = name.to_string();
    let r = ui.add(egui::Label::new(egui::RichText::new(text).font(fonts::ui_font(12.5)).color(col)).truncate().sense(Sense::click()));
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A rounded section with a small heading.
fn card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    let v = ui.visuals().clone();
    egui::Frame::NONE.fill(v.extreme_bg_color).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(title.to_uppercase()).font(fonts::ui_font(11.0)).strong().color(v.weak_text_color()));
        ui.add_space(2.0);
        body(ui);
    });
    ui.add_space(4.0);
}

/// Name / value lines, the names in a fixed column and the values wrapped beside them.
fn rows(ui: &mut egui::Ui, items: &[(&str, String)]) {
    let weak = ui.visuals().weak_text_color();
    let fg = ui.visuals().text_color();
    for (k, val) in items {
        ui.horizontal_top(|ui| {
            let (lr, _) = ui.allocate_exact_size(Vec2::new(92.0, 18.0), Sense::hover());
            ui.painter().with_clip_rect(lr).text(lr.left_center(), Align2::LEFT_CENTER, *k, fonts::ui_font(12.5), weak);
            ui.add(egui::Label::new(egui::RichText::new(val).font(fonts::ui_font(12.5)).color(fg)).wrap());
        });
    }
}

/// One half of a two-way switch.
/// One half of a two-way switch, as an icon (its name in the tooltip).
fn icon_segment(ui: &mut egui::Ui, value: &mut bool, this: bool, icon: &Option<egui::TextureHandle>, name: &str) {
    let on = *value == this;
    let v = ui.visuals().clone();
    let fg = if on { v.strong_text_color() } else { v.text_color() };
    let b = match icon {
        Some(t) => egui::Button::image(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(16.0)).tint(fg)),
        None => egui::Button::new(name),
    };
    // The chosen one sits on Rivet's selected tile.
    let under = ui.painter().add(egui::Shape::Noop);
    let r = ui.add(b.frame_when_inactive(false).min_size(Vec2::new(34.0, 28.0)).corner_radius(rivet_look::theme::r_sm()));
    if on {
        let (fill, stroke) = rivet_look::theme::selected_tile(&v, v.extreme_bg_color);
        ui.painter().set(under, egui::Shape::Vec(vec![egui::Shape::rect_filled(r.rect, rivet_look::theme::r_sm(), fill), egui::Shape::rect_stroke(r.rect, rivet_look::theme::r_sm(), stroke, egui::StrokeKind::Inside)]));
    }
    if r.on_hover_text(name).clicked() {
        *value = this;
    }
}

#[allow(dead_code)]
fn segment(ui: &mut egui::Ui, value: &mut bool, this: bool, label: &str) -> egui::Response {
    let r = ui.selectable_label(*value == this, label);
    if r.clicked() {
        *value = this;
    }
    r
}

fn chip(ui: &mut egui::Ui, text: &str, fill: Color32, fg: Color32) {
    let g = ui.painter().layout_no_wrap(text.to_string(), fonts::ui_font(12.0), fg);
    let (r, _) = ui.allocate_exact_size(g.size() + Vec2::new(12.0, 6.0), Sense::hover());
    ui.painter().rect_filled(r, r_xs(), fill);
    ui.painter().galley(r.min + Vec2::new(6.0, 3.0), g, fg);
}

fn when(t: SystemTime) -> String {
    let t: chrono::DateTime<chrono::Local> = t.into();
    let today = chrono::Local::now().date_naive();
    if t.date_naive() == today {
        format!("Today at {}", t.format("%H:%M"))
    } else if Some(t.date_naive()) == today.pred_opt() {
        format!("Yesterday at {}", t.format("%H:%M"))
    } else {
        t.format("%Y-%m-%d %H:%M").to_string()
    }
}

fn human_size(n: u64) -> String {
    match n {
        0..1024 => format!("{n} bytes"),
        1024..1_048_576 => format!("{:.1} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1_048_576.0),
    }
}
