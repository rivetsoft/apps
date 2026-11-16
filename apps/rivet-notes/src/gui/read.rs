//! The reading view (Ctrl+Space): the open note as a rendered page over the window. Esc or
//! Ctrl+Space closes it. Links to other notes (`[[Name]]` or a relative `.md` path) open them
//! in place; Backspace goes back.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::theme::{r_lg, r_md, r_sm, r_xs};
use rivet_look::{builtin_icons, fonts, highlight, icons, palette};

use super::*;
use crate::render::{self, Block, Inline, Style};

pub(super) struct Reading {
    /// Notes opened by following links, to go back to.
    back: Vec<PathBuf>,
    scroll_to: Option<f32>,
    offset: f32,
    viewport_h: f32,
    /// How far the page can scroll.
    max_offset: f32,
    images: HashMap<PathBuf, Option<egui::TextureHandle>>,
}

/// What a click in the page asks for.
enum Act {
    Url(String),
    Note(String),
    Tag(String),
    Copy(String),
}

/// How the page is drawn.
struct Pen<'a> {
    size: f32,
    /// Extra room between wrapped lines.
    gap: f32,
    fg: Color32,
    strong: Color32,
    weak: Color32,
    accent: Color32,
    code_bg: Color32,
    colors: highlight::Colors,
    dir: PathBuf,
    images: &'a mut HashMap<PathBuf, Option<egui::TextureHandle>>,
    act: Option<Act>,
}

impl Pen<'_> {
    fn font(&self, scale: f32) -> egui::FontId {
        egui::FontId::new(self.size * scale, rivet_look::live::editor_family())
    }
    fn mono(&self, scale: f32) -> egui::FontId {
        egui::FontId::monospace(self.size * 0.9 * scale)
    }

    fn rich(&self, text: &str, st: &Style, scale: f32) -> egui::RichText {
        let mut t = if st.code {
            egui::RichText::new(text).font(self.mono(scale)).background_color(self.code_bg)
        } else if st.bold {
            egui::RichText::new(text).font(egui::FontId::new(self.size * scale, rivet_look::live::editor_bold_family()))
        } else {
            egui::RichText::new(text).font(self.font(scale))
        };
        t = t.color(if st.bold { self.strong } else { self.fg });
        if st.bold {
            t = t.strong();
        }
        if st.italic {
            t = t.italics();
        }
        if st.strike {
            t = t.strikethrough();
        }
        t
    }

    /// One run of text, word by word so it wraps.
    fn words(&mut self, ui: &mut egui::Ui, text: &str, st: &Style, scale: f32, link: Option<&str>) {
        for w in text.split_inclusive(' ') {
            let rt = self.rich(w, st, scale);
            match link {
                Some(target) => {
                    let rt = rt.color(self.accent).underline();
                    let r = ui.add(egui::Label::new(rt).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand);
                    let r = if target.starts_with("[[") { r.on_hover_text(format!("Open {}", target.trim_matches(['[', ']']))) } else { r.on_hover_text(target) };
                    if r.clicked() {
                        self.act = Some(if let Some(n) = target.strip_prefix("[[").and_then(|t| t.strip_suffix("]]")) {
                            Act::Note(n.to_string())
                        } else if target.contains("://") || target.starts_with("mailto:") {
                            Act::Url(target.to_string())
                        } else {
                            Act::Note(target.to_string())
                        });
                    }
                }
                None => {
                    ui.label(rt);
                }
            }
        }
    }

    /// A line (or paragraph) of inline Markdown, wrapping.
    fn line(&mut self, ui: &mut egui::Ui, text: &str, scale: f32) {
        self.line_in(ui, text, scale, true);
    }

    /// `wrap: false` keeps it on one line (table cells).
    fn line_in(&mut self, ui: &mut egui::Ui, text: &str, scale: f32, wrap: bool) {
        let parts = render::inlines(text);
        let gap = self.gap;
        let body = |ui: &mut egui::Ui, pen: &mut Self| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, gap);
            for p in &parts {
                match p {
                    Inline::Text(t, st) => pen.words(ui, t, st, scale, None),
                    Inline::Link(t, to, st) => pen.words(ui, t, st, scale, Some(to)),
                    Inline::Tag(t) => {
                        let rt = egui::RichText::new(format!("#{t}")).font(pen.font(scale * 0.92)).color(pen.accent).background_color(palette::mix(pen.code_bg, pen.accent, 0.18));
                        if ui.add(egui::Label::new(rt).sense(Sense::click())).on_hover_text("Show notes with this tag").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            pen.act = Some(Act::Tag(t.clone()));
                        }
                        ui.label(" ");
                    }
                    Inline::Image { alt, src } => pen.image(ui, alt, src),
                }
            }
        };
        if wrap {
            ui.horizontal_wrapped(|ui| body(ui, self));
        } else {
            ui.horizontal(|ui| body(ui, self));
        }
    }

    /// A picture next to the note (shown), or elsewhere (its description and a link).
    fn image(&mut self, ui: &mut egui::Ui, alt: &str, src: &str) {
        let local = !src.contains("://");
        if local {
            let path = self.dir.join(src.replace("%20", " "));
            let ctx = ui.ctx().clone();
            let max_w = ui.available_width().max(100.0);
            let tex = self
                .images
                .entry(path.clone())
                .or_insert_with(|| icons::load_picture(&path, 1600).map(|img| ctx.load_texture(format!("read:{}", path.display()), img, egui::TextureOptions::LINEAR)))
                .clone();
            if let Some(t) = tex {
                let size = t.size_vec2() / ctx.pixels_per_point();
                let scale = (max_w / size.x).min(1.0);
                ui.add(egui::Image::from_texture(&t).fit_to_exact_size(size * scale).corner_radius(r_sm())).on_hover_text(alt);
                return;
            }
        }
        let label = if alt.is_empty() { src.to_string() } else { alt.to_string() };
        let rt = egui::RichText::new(format!("[picture: {label}]")).font(self.font(0.9)).color(self.weak).background_color(self.code_bg);
        if ui.add(egui::Label::new(rt).sense(Sense::click())).on_hover_text(src).clicked() && !local {
            self.act = Some(Act::Url(src.to_string()));
        }
    }

    fn blocks(&mut self, ui: &mut egui::Ui, blocks: &[Block]) {
        for b in blocks {
            self.block(ui, b);
        }
    }

    fn block(&mut self, ui: &mut egui::Ui, b: &Block) {
        let width = ui.available_width();
        match b {
            Block::Heading(n, t) => {
                let scale = [1.9, 1.5, 1.25, 1.1, 1.0, 0.92][(*n as usize - 1).min(5)];
                ui.add_space(if *n <= 2 { 14.0 } else { 8.0 });
                let (fg, strong) = (self.fg, self.strong);
                self.fg = strong;
                self.line(ui, &format!("**{t}**"), scale);
                self.fg = fg;
                if *n <= 2 {
                    ui.add_space(2.0);
                    let r = ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover()).0;
                    ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(1.0, palette::mix(self.code_bg, self.weak, 0.35)));
                }
                ui.add_space(6.0);
            }
            Block::Para(lines) => {
                // Soft line breaks join with a space; hard ones start a new line.
                let mut text = String::new();
                let mut groups = Vec::new();
                for (l, hard) in lines {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(l);
                    if *hard {
                        groups.push(std::mem::take(&mut text));
                    }
                }
                if !text.is_empty() {
                    groups.push(text);
                }
                for g in groups {
                    self.line(ui, &g, 1.0);
                }
                ui.add_space(10.0);
            }
            Block::Code { lang, text } => {
                let name = format!("code.{}", if lang.is_empty() { "txt" } else { lang.as_str() });
                let job = highlight::layout(text, &name, &self.colors, self.mono(1.0));
                let copy = text.clone();
                egui::Frame::NONE.fill(self.code_bg).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_width(width - 28.0);
                    ui.horizontal(|ui| {
                        if !lang.is_empty() {
                            ui.label(egui::RichText::new(lang).small().color(self.weak));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Copy").on_hover_text("Copy the code").clicked() {
                                self.act = Some(Act::Copy(copy.clone()));
                            }
                        });
                    });
                    egui::ScrollArea::horizontal().id_salt(("code", text.len(), text.as_ptr() as usize)).show(ui, |ui| {
                        ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
                    });
                });
                ui.add_space(10.0);
            }
            Block::Quote(inner) => {
                let r = egui::Frame::NONE.inner_margin(egui::Margin { left: 16, right: 0, top: 2, bottom: 2 }).show(ui, |ui| {
                    ui.set_width(width - 16.0);
                    let fg = self.fg;
                    self.fg = self.weak;
                    self.blocks(ui, inner);
                    self.fg = fg;
                });
                let rr = r.response.rect;
                ui.painter().rect_filled(Rect::from_min_size(rr.min, Vec2::new(3.0, rr.height() - 8.0)), r_xs(), self.accent.gamma_multiply(0.7));
                ui.add_space(4.0);
            }
            Block::List(items) => {
                for it in items {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(6.0 + it.depth as f32 * 22.0);
                        let h = self.size * 1.3;
                        let (r, _) = ui.allocate_exact_size(Vec2::new(24.0, h), Sense::hover());
                        let c = Pos2::new(r.left() + 8.0, r.center().y + 2.0);
                        match (it.task, it.number) {
                            (Some(done), _) => {
                                let b = Rect::from_center_size(c, Vec2::splat(13.0));
                                if done {
                                    ui.painter().rect_filled(b, 3.0, self.accent);
                                    let s = Stroke::new(1.6, Color32::WHITE);
                                    ui.painter().line_segment([c + Vec2::new(-3.5, 0.0), c + Vec2::new(-1.0, 2.8)], s);
                                    ui.painter().line_segment([c + Vec2::new(-1.0, 2.8), c + Vec2::new(3.8, -2.8)], s);
                                } else {
                                    ui.painter().rect_stroke(b, 3.0, Stroke::new(1.2, self.weak), egui::StrokeKind::Inside);
                                }
                            }
                            (None, Some(n)) => {
                                ui.painter().text(Pos2::new(r.right() - 4.0, r.center().y), Align2::RIGHT_CENTER, format!("{n}."), self.font(0.95), self.weak);
                            }
                            (None, None) => {
                                let filled = it.depth % 2 == 0;
                                if filled {
                                    ui.painter().circle_filled(c, 2.8, self.weak);
                                } else {
                                    ui.painter().circle_stroke(c, 2.6, Stroke::new(1.2, self.weak));
                                }
                            }
                        }
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            let done = it.task == Some(true);
                            let fg = self.fg;
                            if done {
                                self.fg = self.weak;
                            }
                            let text = if done { format!("~~{}~~", it.text) } else { it.text.clone() };
                            self.line(ui, &text, 1.0);
                            self.fg = fg;
                        });
                    });
                    ui.add_space(2.0);
                }
                ui.add_space(8.0);
            }
            Block::Table { head, rows } => {
                let line = palette::mix(self.code_bg, self.weak, 0.3);
                egui::Frame::NONE.stroke(Stroke::new(1.0, line)).corner_radius(r_sm()).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                    egui::ScrollArea::horizontal().id_salt(("table", head.join("|"))).show(ui, |ui| {
                        egui::Grid::new(("table-grid", head.join("|"))).striped(true).spacing([24.0, 8.0]).show(ui, |ui| {
                            for h in head {
                                ui.vertical(|ui| {
                                    let fg = self.fg;
                                    self.fg = self.strong;
                                    self.line_in(ui, &format!("**{h}**"), 1.0, false);
                                    self.fg = fg;
                                });
                            }
                            ui.end_row();
                            for row in rows {
                                for k in 0..head.len() {
                                    ui.vertical(|ui| self.line_in(ui, row.get(k).map(String::as_str).unwrap_or(""), 1.0, false));
                                }
                                ui.end_row();
                            }
                        });
                    });
                });
                ui.add_space(12.0);
            }
            Block::Rule => {
                ui.add_space(8.0);
                let r = ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover()).0;
                ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(1.0, palette::mix(self.code_bg, self.weak, 0.4)));
                ui.add_space(12.0);
            }
        }
    }
}

impl NotesApp {
    /// Open or close the reading view (Ctrl+Space).
    pub(super) fn toggle_reading(&mut self, ctx: &egui::Context) {
        if self.reading.is_some() {
            self.reading = None;
            return;
        }
        if self.doc.is_none() {
            return;
        }
        self.save_now();
        // The editor gives up the keyboard while the page is up.
        if let Some(id) = ctx.memory(|m| m.focused()) {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
        self.reading = Some(Reading { back: Vec::new(), scroll_to: Some(0.0), offset: 0.0, viewport_h: 400.0, max_offset: 0.0, images: HashMap::new() });
    }

    /// Find a note from a link: `Name` (a title or file name) or a path relative to `dir`.
    fn link_target(&self, target: &str, dir: &Path) -> Option<PathBuf> {
        let t = target.split('#').next().unwrap_or(target).replace("%20", " ");
        if t.is_empty() {
            return None;
        }
        // Other files beside the note (pictures, PDFs) open too.
        let p = dir.join(&t);
        if p.is_file() {
            return Some(p);
        }
        vault::resolve(&self.notes, &dir.join("_"), &t).map(|n| n.path.clone())
    }

    /// Draw the page (if open) and handle its keys.
    pub(super) fn reading_view(&mut self, ctx: &egui::Context) {
        let Some(mut rd) = self.reading.take() else { return };
        let Some(d) = self.doc.as_ref() else { return };
        // Keys: Esc and Ctrl+Space close; Backspace goes back; arrows and pages scroll.
        let (esc, back, up, down, pgup, pgdn, home, end) = ctx.input_mut(|i| {
            let none = egui::Modifiers::NONE;
            (
                i.consume_key(none, Key::Escape),
                i.consume_key(none, Key::Backspace) || i.consume_key(egui::Modifiers::ALT, Key::ArrowLeft),
                i.consume_key(none, Key::ArrowUp),
                i.consume_key(none, Key::ArrowDown),
                i.consume_key(none, Key::PageUp),
                i.consume_key(none, Key::PageDown) || i.consume_key(none, Key::Space),
                i.consume_key(none, Key::Home),
                i.consume_key(none, Key::End),
            )
        });
        if esc {
            return;
        }
        let step = 48.0;
        let page = (rd.viewport_h - 60.0).max(step);
        let delta = if up { -step } else if down { step } else if pgup { -page } else if pgdn { page } else { 0.0 };
        if delta != 0.0 {
            rd.scroll_to = Some((rd.offset + delta).clamp(0.0, rd.max_offset));
        }
        if home {
            rd.scroll_to = Some(0.0);
        }
        if end {
            rd.scroll_to = Some(rd.max_offset);
        }
        let path = d.path.clone();
        let fm = d.fm.clone();
        let body = d.body.clone();
        let v = ctx.global_style().visuals.clone();
        let accent = self.look.accent(&self.choice());
        let code_bg = palette::mix(v.extreme_bg_color, v.text_color(), 0.06);
        let mut pen = Pen {
            size: self.s.editor_size * fonts::scale(),
            gap: (self.s.editor_size * fonts::scale() * (self.s.line_spacing - 1.2)).max(2.0),
            fg: v.text_color(),
            strong: v.strong_text_color(),
            weak: v.weak_text_color(),
            accent,
            code_bg,
            colors: highlight::colors(v.text_color(), code_bg, accent, builtin_icons::derive_secondary(accent), builtin_icons::derive_tertiary(accent)),
            dir: path.parent().map(Path::to_path_buf).unwrap_or_default(),
            images: &mut rd.images,
            act: None,
        };
        let blocks = render::parse(&body);
        let title = fm.as_ref().and_then(|f| f.get("title")).map(|x| x.as_text()).filter(|t| !t.is_empty()).unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
        let tags: Vec<String> = fm.as_ref().map(|f| f.tags()).unwrap_or_default();
        // Skip a first heading that only repeats the title.
        let blocks: &[Block] = match blocks.first() {
            Some(Block::Heading(1, t)) if *t == title => &blocks[1..],
            _ => &blocks,
        };
        let screen = ctx.content_rect();
        let mut close = false;
        let mut go_back = back && !rd.back.is_empty();
        egui::Area::new(egui::Id::unique("reading-view")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            ui.set_min_size(screen.size());
            let bg = ui.interact(screen, egui::Id::unique("reading-bg"), Sense::click());
            ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha(if v.dark_mode { 170 } else { 110 }));
            if bg.clicked() {
                close = true;
            }
            let w = (screen.width() - 80.0).min(860.0);
            let card = Rect::from_center_size(screen.center(), Vec2::new(w, screen.height() - 36.0));
            ui.painter().rect_filled(card, r_lg(), v.extreme_bg_color);
            let _ = ui.interact(card, egui::Id::unique("reading-card"), Sense::click());
            let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(card.shrink2(Vec2::new(0.0, 14.0))).layout(egui::Layout::top_down(egui::Align::Min)));
            // Top bar: back, where it is, close.
            cui.horizontal(|ui| {
                ui.add_space(18.0);
                if !rd.back.is_empty() && ui.small_button("Back").on_hover_text("Back (Backspace)").clicked() {
                    go_back = true;
                }
                let rel = self.vault.as_ref().and_then(|vt| path.strip_prefix(vt).ok()).map(|r| r.to_string_lossy().replace('/', "  ›  ")).unwrap_or_default();
                ui.label(egui::RichText::new(rel).small().color(pen.weak));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(14.0);
                    if ui.add(egui::Button::new("Close").corner_radius(r_sm())).on_hover_text("Close (Esc or Ctrl+Space)").clicked() {
                        close = true;
                    }
                });
            });
            let mut sa = egui::ScrollArea::vertical().id_salt(("reading", path.to_string_lossy().into_owned())).auto_shrink([false, false]);
            if let Some(y) = rd.scroll_to.take() {
                sa = sa.vertical_scroll_offset(y);
            }
            let out = sa.show(&mut cui, |ui| {
                egui::Frame::NONE.inner_margin(egui::Margin { left: 48, right: 48, top: 10, bottom: 40 }).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(&title).font(egui::FontId::new(pen.size * 2.1, rivet_look::live::editor_bold_family())).color(pen.strong));
                    if !tags.is_empty() {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            for t in &tags {
                                let rt = egui::RichText::new(format!("#{t}")).font(pen.font(0.85)).color(pen.accent).background_color(palette::mix(code_bg, accent, 0.18));
                                if ui.add(egui::Label::new(rt).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    pen.act = Some(Act::Tag(t.clone()));
                                }
                            }
                        });
                    }
                    ui.add_space(14.0);
                    if blocks.is_empty() {
                        ui.label(egui::RichText::new("This note is empty.").color(pen.weak));
                    }
                    pen.blocks(ui, blocks);
                });
            });
            rd.offset = out.state.clamped_offset().y;
            rd.viewport_h = out.inner_rect.height();
            rd.max_offset = (out.content_size.y - out.inner_rect.height()).max(0.0);
        });
        let act = pen.act.take();
        let dir = pen.dir.clone();
        match act {
            Some(Act::Url(u)) => {
                let _ = rivet_xdg::launch::open_url(&u);
            }
            Some(Act::Copy(t)) => {
                ctx.copy_text(t);
                self.note_msg = Some(("Code copied".into(), false));
            }
            Some(Act::Tag(t)) => {
                self.search = format!("#{t}");
                close = true;
            }
            Some(Act::Note(n)) => match self.link_target(&n, &dir) {
                Some(p) if p.extension().is_some_and(|x| x == "md" || x == "markdown") => {
                    rd.back.push(path.clone());
                    self.open_note(&p);
                    rd.scroll_to = Some(0.0);
                }
                Some(p) => {
                    let _ = rivet_xdg::launch::open_url(&p.to_string_lossy());
                }
                None => self.note_msg = Some((format!("There's no note \"{n}\" in this vault."), true)),
            },
            None => {}
        }
        if go_back && let Some(p) = rd.back.pop() {
            self.open_note(&p);
            rd.scroll_to = Some(0.0);
        }
        if !close {
            self.reading = Some(rd);
        }
    }
}
