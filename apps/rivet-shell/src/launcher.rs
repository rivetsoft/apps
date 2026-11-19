//! The launcher: a search field over the apps, as a grid of icons or a list (Settings,
//! Launcher), with sums answered as they're typed. On its own so it can grow (files, commands,
//! plugins' results…): each kind of result is found from the query, then drawn by the view.

use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::quick;
use rivet_look::theme::r_md;
use rivet_xdg::launch::DesktopApp;

use crate::config::{LaunchSize, LaunchView, Settings};
use crate::ui::{Act, Kit, Model, calc, calc_text, hover_fill, ui_font};

/// The launcher's size on a screen.
pub fn size(s: &Settings, full: Rect) -> Vec2 {
    let (w, h) = match (s.launch_view, s.launch_size) {
        (LaunchView::Grid, LaunchSize::Small) => (500.0, 400.0),
        (LaunchView::Grid, LaunchSize::Medium) => (620.0, 480.0),
        (LaunchView::Grid, LaunchSize::Large) => (800.0, 600.0),
        (LaunchView::List, LaunchSize::Small) => (400.0, 400.0),
        (LaunchView::List, LaunchSize::Medium) => (480.0, 480.0),
        (LaunchView::List, LaunchSize::Large) => (580.0, 600.0),
    };
    let h = f32::min(h, full.height() - 120.0);
    // As tall as whole rows take (no strip left under the last one): the card's padding, the
    // search field and the room under it, then the rows.
    const CHROME: f32 = 14.0 * 2.0 + 60.0;
    let (pitch, gap) = pitch(s);
    let rows = ((h - CHROME + gap) / pitch).floor().max(1.0);
    Vec2::new(f32::min(w, full.width() - 40.0), CHROME + rows * pitch - gap)
}

/// How far apart rows start, and the room between them.
fn pitch(s: &Settings) -> (f32, f32) {
    let icon = s.launch_icon.clamp(24, 64) as f32;
    if s.launch_view == LaunchView::List { (list_icon(icon) + 14.0 + 2.0, 2.0) } else { (icon + 50.0 + 6.0, 6.0) }
}

/// The panel the launcher opens from when it's opened by a key (the first on this screen):
/// the panel, its edge and how far from it to open, whether it joins the panel, and where
/// along it (the end the panel's Apps button is at, else the middle: NaN).
pub fn home(s: &Settings, output: &str) -> Option<(usize, (crate::config::Edge, f32), bool, f32)> {
    let idx = s.panels.iter().position(|p| p.on(output))?;
    let c = &s.panels[idx];
    let look = s.panel_look(idx);
    let reach = crate::panels::reach(&s.panels, idx, output);
    // As the panel's own popups open (bar.rs).
    let from = if look.attach { reach - c.gap() } else { reach + 4.0 };
    let at = match c.zones.iter().position(|z| z.iter().any(|i| i == "launcher")) {
        Some(0) => 0.0,
        Some(2) => f32::MAX,
        _ => f32::NAN,
    };
    Some((idx, (c.edge, from), look.attach, at))
}

/// A list row's icon for the grid's icon size.
fn list_icon(icon: f32) -> f32 {
    (icon * 0.67).round().max(18.0)
}

/// Something the launcher lists: an app, or an entry a plugin gave.
enum Entry<'a> {
    App(&'a DesktopApp),
    Plugin { pid: String, id: String, name: String, sub: String, icon: String, query: String },
    /// A command's word (`@ftp`), for the plugin called `sub`: choosing it types it.
    Keyword { word: String, name: String, sub: String, icon: String },
}

impl Entry<'_> {
    fn name(&self) -> &str {
        match self {
            Entry::App(a) => &a.name,
            Entry::Plugin { name, .. } | Entry::Keyword { name, .. } => name,
        }
    }
    /// A second, weaker part after the name (a list's rows show it).
    fn sub(&self) -> &str {
        match self {
            Entry::App(_) => "",
            Entry::Plugin { sub, .. } | Entry::Keyword { sub, .. } => sub,
        }
    }
    fn icon(&self, ctx: &egui::Context, kit: &mut Kit, px: f32) -> Option<egui::TextureHandle> {
        match self {
            Entry::App(a) => kit.app_icon(ctx, a, px),
            Entry::Plugin { icon, .. } | Entry::Keyword { icon, .. } => kit.named_icon(ctx, icon, px).or_else(|| kit.named_icon(ctx, "application-x-executable", px)),
        }
    }
    /// Choosing it: the app starts, or the plugin is told (with what was typed).
    fn acts(&self) -> Vec<Act> {
        match self {
            Entry::App(a) => vec![Act::Launch(a.id.clone())],
            Entry::Keyword { .. } => Vec::new(),
            Entry::Plugin { pid, id, query, .. } => vec![Act::Plugin(pid.clone(), crate::plugins::Event::new("launch", &[("id", id), ("query", query)])), Act::Close],
        }
    }
}

pub fn launcher(ui: &mut egui::Ui, query: &mut String, sel: &mut usize, focus: bool, m: &mut Model, kit: &mut Kit) {
    // A steady cursor: a blinking one redraws the whole launcher twice a second.
    ui.visuals_mut().text_cursor.blink = false;
    let v = ui.visuals().clone();
    // A command: `@word` shows only that plugin's entries, and the words after it search them
    // (`@ftp work`); `@` alone, or the start of a word, lists the words there are.
    let command = query.trim_start().strip_prefix('@').map(|q| {
        let (word, rest) = q.split_once(char::is_whitespace).unwrap_or((q, ""));
        (word.to_lowercase(), rest.trim().to_string())
    });
    // Commands' entries have a second line: always a list.
    let list = m.s.launch_view == LaunchView::List || command.is_some();
    let icon = m.s.launch_icon.clamp(24, 64) as f32;
    let id = egui::Id::unique("launcher-search");
    let has_words = m.extra.plugins.iter().any(|(_, it)| !it.keyword.is_empty());
    let hint = match (m.s.launch_calc, has_words) {
        (true, true) => "Search apps, type a sum, or @ for commands…",
        (true, false) => "Search apps, or type a sum…",
        (false, true) => "Search apps, or @ for commands…",
        (false, false) => "Search apps…",
    };
    let field = egui::Frame::NONE.fill(mix(v.panel_fill, v.text_color(), 0.07)).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(12, 9)).show(ui, |ui| {
        ui.add(egui::TextEdit::singleline(query).id(id).frame(egui::Frame::NONE).font(ui_font(16.0)).hint_text(hint).desired_width(f32::INFINITY))
    });
    let resp = field.inner;
    if focus || !resp.has_focus() {
        resp.request_focus();
    }
    if resp.changed() {
        *sel = 0;
    }
    ui.add_space(12.0);
    let accent = kit.accent(m.s);
    let enter = ui.input(|i| i.key_pressed(Key::Enter));
    // A sum: its answer, and Enter copies it.
    if m.s.launch_calc
        && command.is_none()
        && let Some(answer) = calc(query)
    {
        let text = calc_text(answer);
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::click());
        ui.painter().rect_filled(r, r_md(), accent.gamma_multiply(0.22));
        ui.painter().rect_stroke(r, r_md(), Stroke::new(1.0, accent.gamma_multiply(0.6)), egui::StrokeKind::Inside);
        ui.painter().text(Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, format!("= {text}"), ui_font(20.0), v.strong_text_color());
        ui.painter().text(Pos2::new(r.right() - 16.0, r.center().y), Align2::RIGHT_CENTER, "Enter to copy", ui_font(12.0), v.weak_text_color());
        if enter || resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            m.acts.push(Act::Copy(text));
        }
        return;
    }
    // The apps: best matches first when searching, else by name.
    let mut ranked: Vec<(i32, Entry)> = Vec::new();
    let typed = query.trim().to_string();
    let entry = |pid: &str, row: &crate::plugins::LaunchRow, words: &str| Entry::Plugin { pid: pid.to_string(), id: row.id.clone(), name: if words.is_empty() { row.label.clone() } else { format!("{} “{words}”", row.label) }, sub: row.sub.clone(), icon: row.icon.clone(), query: words.to_string() };
    match &command {
        Some((word, rest)) => match m.extra.plugins.iter().find(|(_, it)| !word.is_empty() && &it.keyword == word) {
            // One plugin's entries: all of them, or the ones the words find; an entry that
            // takes words comes last with them.
            Some((pid, it)) => {
                for (k, row) in it.launcher.iter().enumerate() {
                    let found = quick::fuzzy_score(&row.label, rest).or_else(|| quick::fuzzy_score(&row.sub, rest).map(|s| s - 1000));
                    match found {
                        Some(s) => ranked.push((if rest.is_empty() { -(k as i32) } else { s }, entry(pid, row, ""))),
                        None if row.query => ranked.push((i32::MIN, entry(pid, row, rest))),
                        None => {}
                    }
                }
                ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
            }
            // The words there are, as far as it's typed.
            None => {
                for (_, it) in m.extra.plugins.iter().filter(|(_, it)| !it.keyword.is_empty() && it.keyword.starts_with(word.as_str())) {
                    ranked.push((0, Entry::Keyword { word: it.keyword.clone(), name: format!("@{}", it.keyword), sub: it.name.clone(), icon: it.icon.clone() }));
                }
            }
        },
        None => {
            // The apps: best matches first when searching, else by name.
            ranked.extend(m.apps.iter().filter_map(|a| quick::fuzzy_score(&a.name, query).map(|s| (s, Entry::App(a)))));
            // What plugins add, once something is typed: entries whose name matches, and after
            // everything else the ones that take the words typed (New Note “words”).
            if !typed.is_empty() {
                for (pid, it) in &m.extra.plugins {
                    for row in it.launcher.iter().filter(|r| !r.scoped) {
                        match quick::fuzzy_score(&row.label, query) {
                            Some(s) => ranked.push((s, entry(pid, row, ""))),
                            None if row.query => ranked.push((i32::MIN, entry(pid, row, &typed))),
                            None => {}
                        }
                    }
                }
                ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
            }
        }
    }
    let mut picked: Option<usize> = None;
    // A grid's tile holds the icon over two lines of name.
    let tile = Vec2::new(icon + 56.0, icon + 50.0);
    let cols = if list { 1 } else { ((ui.available_width() + 6.0) / (tile.x + 6.0)).floor().max(1.0) as usize };
    let (up, down, left, right) = ui.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
            !list && i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft) && query.is_empty(),
            !list && i.consume_key(egui::Modifiers::NONE, Key::ArrowRight) && query.is_empty(),
        )
    });
    let n = ranked.len();
    if n > 0 {
        if down {
            *sel = (*sel + cols).min(n - 1);
        }
        if up {
            *sel = sel.saturating_sub(cols);
        }
        if right {
            *sel = (*sel + 1).min(n - 1);
        }
        if left {
            *sel = sel.saturating_sub(1);
        }
        *sel = (*sel).min(n - 1);
    }
    if enter && *sel < ranked.len() {
        picked = Some(*sel);
    }
    let moved = up || down || left || right;
    let mark = |p: &egui::Painter, r: Rect, chosen: bool, hovered: bool| {
        if chosen {
            p.rect_filled(r, r_md(), accent.gamma_multiply(0.22));
            p.rect_stroke(r, r_md(), Stroke::new(1.0, accent.gamma_multiply(0.6)), egui::StrokeKind::Inside);
        } else if hovered {
            hover_fill(p, r, &v);
        }
    };
    // Whole rows only: the list is as tall as the rows that fit, and while it scrolls a row
    // that's partly out of it isn't drawn (rather than cut through).
    let (pitch, gap) = if list { (list_icon(icon) + 14.0 + 2.0, 2.0) } else { pitch(m.s) };
    let rows = ((ui.available_height() + gap) / pitch).floor().max(1.0);
    let whole = |ui: &egui::Ui, r: Rect| ui.clip_rect().expand(0.5).contains_rect(r);
    egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(rows * pitch - gap).show(ui, |ui| {
        if list {
            let li = list_icon(icon);
            ui.spacing_mut().item_spacing.y = 2.0;
            for (k, (_, a)) in ranked.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), li + 14.0), Sense::click());
                // Only what's in view is drawn (and only its icons made).
                let shown = whole(ui, r);
                if shown {
                    mark(ui.painter(), r, k == *sel, resp.hovered());
                    let ir = Rect::from_center_size(Pos2::new(r.left() + 10.0 + li / 2.0, r.center().y), Vec2::splat(li));
                    if let Some(t) = a.icon(ui.ctx(), kit, li) {
                        ui.painter().image(t.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    let room = r.right() - ir.right() - 24.0 - if k == *sel { 44.0 } else { 0.0 };
                    let g = crate::ui::one_line(ui.painter(), a.name(), ui_font(14.0), v.text_color(), room);
                    let after = ir.right() + 12.0 + g.size().x + 12.0;
                    ui.painter().galley(Pos2::new(ir.right() + 12.0, r.center().y - g.size().y / 2.0), g, v.text_color());
                    // What a plugin's entry says of itself, after its name.
                    let sub_room = ir.right() + 12.0 + room - after;
                    if !a.sub().is_empty() && sub_room > 40.0 {
                        let g = crate::ui::one_line(ui.painter(), a.sub(), ui_font(12.0), v.weak_text_color(), sub_room);
                        ui.painter().galley(Pos2::new(after, r.center().y - g.size().y / 2.0 + 0.5), g, v.weak_text_color());
                    }
                    if k == *sel {
                        ui.painter().text(Pos2::new(r.right() - 12.0, r.center().y), Align2::RIGHT_CENTER, "Enter", ui_font(11.5), v.weak_text_color());
                    }
                }
                if k == *sel && moved {
                    ui.scroll_to_rect(r, None);
                }
                if shown && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    picked = Some(k);
                }
            }
        } else {
            egui::Grid::new("launcher-grid").spacing([6.0, 6.0]).show(ui, |ui| {
                for (k, (_, a)) in ranked.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(tile, Sense::click());
                    let shown = whole(ui, r);
                    if shown {
                        mark(ui.painter(), r, k == *sel, resp.hovered());
                        let ir = Rect::from_center_size(Pos2::new(r.center().x, r.top() + 9.0 + icon / 2.0), Vec2::splat(icon));
                        if let Some(t) = a.icon(ui.ctx(), kit, icon) {
                            ui.painter().image(t.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                        }
                        let mut job = egui::text::LayoutJob::single_section(a.name().to_string(), egui::TextFormat { font_id: ui_font(12.0), color: v.text_color(), ..Default::default() });
                        job.wrap = egui::text::TextWrapping { max_width: r.width() - 8.0, max_rows: 2, break_anywhere: false, overflow_character: Some('…') };
                        job.halign = egui::Align::Center;
                        let g = ui.painter().layout_job(job);
                        ui.painter().galley(Pos2::new(r.center().x, r.top() + icon + 16.0), g, v.text_color());
                    }
                    if k == *sel && moved {
                        ui.scroll_to_rect(r, None);
                    }
                    if shown && resp.clicked() {
                        picked = Some(k);
                    }
                    if (k + 1) % cols == 0 {
                        ui.end_row();
                    }
                }
            });
        }
        if ranked.is_empty() {
            let text = match &command {
                Some((word, _)) if m.extra.plugins.iter().any(|(_, it)| !word.is_empty() && &it.keyword == word) => "No matches",
                Some(_) if !has_words => "No commands yet: plugins add them",
                Some(_) => "No command by that name",
                None => "No apps match",
            };
            ui.label(egui::RichText::new(text).color(v.weak_text_color()));
        }
    });
    // What was chosen: an app or a plugin's entry is told; a command's word is typed in, for
    // its entries to show.
    let mut fill = None;
    match picked.and_then(|k| ranked.get(k)).map(|(_, e)| e) {
        Some(Entry::Keyword { word, .. }) => fill = Some(format!("@{word} ")),
        Some(e) => m.acts.extend(e.acts()),
        None => {}
    }
    if let Some(text) = fill {
        *query = text;
        *sel = 0;
        // The cursor after it.
        let mut st = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
        st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(query.chars().count()))));
        st.store(ui.ctx(), id);
        ui.ctx().request_repaint();
    }
}
