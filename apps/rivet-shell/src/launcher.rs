//! The launcher: a search field over the apps, as a grid of icons or a list (Settings,
//! Launcher), with sums answered as they're typed. On its own so it can grow (files, commands,
//! plugins' results…): each kind of result is found from the query, then drawn by the view.

use rivet_look::a11y::{Access, Role};
use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::quick;
use rivet_look::theme::r_md;
use rivet_xdg::launch::DesktopApp;

use crate::config::{LaunchSize, LaunchView, Settings};
use crate::ui::{Act, Kit, Model, calc, calc_text, find_app, hover_fill, ui_font};

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
    // A grid as wide as the columns asked for take, each a tile (the icon over its name) with
    // the room between, inside the card's padding.
    let w = if s.launch_view == LaunchView::Grid && s.launch_cols > 0 { s.launch_cols as f32 * (s.launch_icon.clamp(24, 64) as f32 + 56.0 + 6.0) - 6.0 + 14.0 * 2.0 + 0.5 } else { w };
    let tallest = full.height() - 120.0;
    let h = f32::min(h, tallest);
    // As tall as whole rows take (no strip left under the last one): the card's padding, the
    // search field and the room under it, then the rows.
    const CHROME: f32 = 14.0 * 2.0 + 60.0;
    let (pitch, gap) = pitch(s);
    // The favourites' row with the line under it, and the directions.
    let extra = if s.launch_favorites { s.launch_icon.clamp(24, 64) as f32 + 50.0 + FAV_GAP + 4.0 + FOOT + 8.0 } else { 0.0 };
    let fit = |h: f32| ((h - CHROME - extra + gap) / pitch).floor().max(1.0);
    // The rows asked for, as far as the screen has room; else as many as the size holds.
    let rows = if s.launch_rows > 0 { (s.launch_rows as f32).min(fit(tallest)) } else { fit(h) };
    Vec2::new(f32::min(w, full.width() - 40.0), CHROME + extra + rows * pitch - gap)
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
    /// A copy in the clipboard history (`@clip`): choosing it puts it back on the clipboard.
    Clip { id: u64, name: String, sub: String, pic: Option<std::sync::Arc<crate::clip::Picture>> },
}

/// The clipboard history's command, shown while there is a history.
const CLIP_WORD: &str = "clip";
const CLIP_NAME: &str = "Clipboard History";
const CLIP_ICON: &str = "edit-paste";

/// The commands there are, as their word, name and icon: the clipboard history's, then the
/// plugins'.
fn commands(m: &Model) -> Vec<(String, String, String)> {
    let clip = m.extra.clip.is_some().then(|| (CLIP_WORD.to_string(), CLIP_NAME.to_string(), CLIP_ICON.to_string()));
    clip.into_iter().chain(m.extra.plugins.iter().filter(|(_, it)| !it.keyword.is_empty()).map(|(_, it)| (it.keyword.clone(), it.name.clone(), it.icon.clone()))).collect()
}

impl Entry<'_> {
    fn name(&self) -> &str {
        match self {
            Entry::App(a) => &a.name,
            Entry::Plugin { name, .. } | Entry::Keyword { name, .. } | Entry::Clip { name, .. } => name,
        }
    }
    /// A second, weaker part after the name (a list's rows show it).
    fn sub(&self) -> &str {
        match self {
            Entry::App(_) => "",
            Entry::Plugin { sub, .. } | Entry::Keyword { sub, .. } | Entry::Clip { sub, .. } => sub,
        }
    }
    /// A picture's width over its height, for its thumbnail (None: a square icon).
    fn aspect(&self) -> Option<f32> {
        match self {
            Entry::Clip { pic: Some(p), .. } if p.thumb.1 > 0 => Some(p.thumb.0 as f32 / p.thumb.1 as f32),
            _ => None,
        }
    }
    fn icon(&self, ctx: &egui::Context, kit: &mut Kit, px: f32) -> Option<egui::TextureHandle> {
        match self {
            Entry::App(a) => kit.app_icon(ctx, a, px),
            Entry::Plugin { icon, .. } | Entry::Keyword { icon, .. } => kit.named_icon(ctx, icon, px).or_else(|| kit.named_icon(ctx, "application-x-executable", px)),
            Entry::Clip { id, pic: Some(p), .. } => kit.clip_thumb(ctx, *id, p),
            Entry::Clip { .. } => kit.named_icon(ctx, "text-x-generic", px).or_else(|| kit.named_icon(ctx, CLIP_ICON, px)),
        }
    }
    /// Choosing it: the app starts, or the plugin is told (with what was typed).
    fn acts(&self) -> Vec<Act> {
        match self {
            Entry::App(a) => vec![Act::Launch(a.id.clone())],
            Entry::Keyword { .. } => Vec::new(),
            Entry::Plugin { pid, id, query, .. } => vec![Act::Plugin(pid.clone(), crate::plugins::Event::new("launch", &[("id", id), ("query", query)])), Act::Close],
            Entry::Clip { id, .. } => vec![Act::Clip(crate::clip::Job::Restore(*id)), Act::Close],
        }
    }
}

pub fn launcher(ui: &mut egui::Ui, query: &mut String, sel: &mut usize, focus: bool, m: &mut Model, kit: &mut Kit) {
    // A steady cursor: a blinking one redraws the whole launcher twice a second.
    ui.visuals_mut().text_cursor.blink = false;
    let v = ui.visuals().clone();
    let id = egui::Id::unique("launcher-search");
    // A command's word, typed in full (or with a space after it), is locked in: it becomes a
    // badge before the field, and the field holds only the words that search its entries. The
    // query keeps both, as `@word words`.
    let mut badge = locked(query, m);
    if let Some((word, ..)) = &badge
        && !query.trim_start().contains(char::is_whitespace)
    {
        *query = format!("@{word} ");
        cursor_to_end(ui.ctx(), id, 0);
    }
    // A command: `@word` shows only that plugin's entries, and the words after it search them
    // (`@ftp work`); `@` alone, or the start of a word, lists the words there are.
    let command = query.trim_start().strip_prefix('@').map(|q| {
        let (word, rest) = q.split_once(char::is_whitespace).unwrap_or((q, ""));
        (word.to_lowercase(), rest.trim().to_string())
    });
    // Commands' entries have a second line: always a list.
    let list = m.s.launch_view == LaunchView::List || command.is_some();
    let icon = m.s.launch_icon.clamp(24, 64) as f32;
    let has_words = !commands(m).is_empty();
    let hint = match (m.s.launch_calc, has_words) {
        (true, true) => "Search apps, type a sum, or @ for commands…",
        (true, false) => "Search apps, or type a sum…",
        (false, true) => "Search apps, or @ for commands…",
        (false, false) => "Search apps…",
    };
    // Backspace in an empty field takes the badge off, back to the list of commands.
    if let Some((word, ..)) = &badge
        && query.trim_start().strip_prefix(&format!("@{word}")).is_some_and(|r| r.trim().is_empty())
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Backspace))
    {
        *query = "@".into();
        *sel = 0;
        badge = None;
        cursor_to_end(ui.ctx(), id, 1);
    }
    let field = egui::Frame::NONE.fill(mix(v.panel_fill, v.text_color(), 0.07)).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(12, 9)).show(ui, |ui| match &badge {
        Some((word, name, icon)) => ui
            .horizontal(|ui| {
                let accent = kit.accent(m.s);
                egui::Frame::NONE.fill(accent.gamma_multiply(0.22)).stroke(Stroke::new(1.0, accent.gamma_multiply(0.6))).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(8, 2)).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.horizontal(|ui| {
                        if let Some(t) = kit.named_icon(ui.ctx(), icon, 16.0) {
                            ui.image((t.id(), Vec2::splat(16.0)));
                        }
                        ui.label(egui::RichText::new(name).font(ui_font(14.0)).color(v.strong_text_color()));
                    });
                });
                let mut words = query.trim_start().strip_prefix(&format!("@{word}")).unwrap_or("").trim_start().to_string();
                let resp = ui.add(egui::TextEdit::singleline(&mut words).id(id).frame(egui::Frame::NONE).font(ui_font(16.0)).hint_text(format!("Search {name}…")).desired_width(f32::INFINITY));
                *query = format!("@{word} {words}");
                resp
            })
            .inner,
        None => ui.add(egui::TextEdit::singleline(query).id(id).frame(egui::Frame::NONE).font(ui_font(16.0)).hint_text(hint).desired_width(f32::INFINITY)),
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
        let resp = resp.role(Role::Button, format!("= {text}"));
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
        // The clipboard history: newest first under the pinned copies, or the ones the words find.
        Some((word, rest)) if word == CLIP_WORD && m.extra.clip.is_some() => {
            let list = m.extra.clip.as_deref().unwrap_or_default();
            for (k, c) in crate::clip::shown(list).into_iter().enumerate() {
                let score = if rest.is_empty() { Some(-(k as i32)) } else { quick::fuzzy_score(&c.line(200), rest) };
                let Some(score) = score else { continue };
                let when = chrono::DateTime::from_timestamp(c.at, 0).map(|t| crate::ui::ago(t.with_timezone(&chrono::Local))).unwrap_or_default();
                let pic = match &c.body {
                    crate::clip::Body::Image(p) => Some(p.clone()),
                    crate::clip::Body::Text(_) => None,
                };
                ranked.push((score, Entry::Clip { id: c.id, name: c.line(120), sub: if c.pinned { format!("Pinned · {when}") } else { when }, pic }));
            }
            ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
        }
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
                for (w, name, icon) in commands(m).into_iter().filter(|(w, ..)| w.starts_with(word.as_str())) {
                    ranked.push((0, Entry::Keyword { name: format!("@{w}"), word: w, sub: name, icon }));
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
    // Favourites: a row of their own above the rest while nothing is typed, which Left and
    // Right move along. Selection counts them first, then the entries below.
    let faving = m.s.launch_favorites;
    let all_favs: Vec<&DesktopApp> = if faving { m.s.launch_favorite_apps.iter().filter_map(|id| find_app(m.apps, id)).collect() } else { Vec::new() };
    let fav_row = faving && command.is_none() && typed.is_empty();
    let favs: &[&DesktopApp] = if fav_row { &all_favs } else { &[] };
    let nf = favs.len();
    let is_fav = |a: &DesktopApp| all_favs.iter().any(|f| f.id == a.id);
    let n = ranked.len();
    *sel = (*sel).min((nf + n).saturating_sub(1));
    let in_favs = *sel < nf;
    let (up, down, left, right) = ui.input_mut(|i| {
        let grab = in_favs || !list;
        let side = in_favs || query.is_empty();
        (
            i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
            grab && i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft) && side,
            grab && i.consume_key(egui::Modifiers::NONE, Key::ArrowRight) && side,
        )
    });
    if in_favs {
        if right {
            *sel = (*sel + 1).min(nf - 1);
        }
        if left {
            *sel = sel.saturating_sub(1);
        }
        // Down to the entry under it, as near as the first row goes.
        if down && n > 0 {
            *sel = nf + (*sel).min(cols.min(n) - 1);
        }
    } else if n > 0 {
        let mut k = (*sel - nf).min(n - 1);
        if up && k < cols && nf > 0 {
            *sel = k.min(nf - 1);
        } else {
            if down {
                k = (k + cols).min(n - 1);
            }
            if up {
                k = k.saturating_sub(cols);
            }
            if right {
                k = (k + 1).min(n - 1);
            }
            if left {
                k = k.saturating_sub(1);
            }
            *sel = nf + k;
        }
    }
    if enter && *sel < nf + n {
        picked = Some(*sel);
    }
    // Ctrl+D adds the selected app to the favourites or takes it off, as a browser's bookmark key.
    let mut toggle: Option<String> = None;
    if faving && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::D)) {
        let app = if *sel < nf {
            Some(favs[*sel])
        } else {
            match ranked.get(*sel - nf) {
                Some((_, Entry::App(a))) => Some(*a),
                _ => None,
            }
        };
        toggle = app.map(|a| a.id.trim_end_matches(".desktop").to_string());
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
    // The directions along the bottom, kept clear of the rows.
    let foot = faving.then(|| {
        let r = ui.max_rect();
        Rect::from_min_max(Pos2::new(r.left(), r.bottom() - FOOT), r.max)
    });
    if fav_row {
        // The row and the line under it, in a block exactly as tall as size() leaves for it.
        let (block, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), tile.y + FAV_GAP), Sense::hover());
        ui.painter().hline(block.x_range(), block.bottom() - FAV_GAP / 2.0, Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.1)));
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(block.min, Vec2::new(block.width(), tile.y))));
        let ui = &mut ui;
        if favs.is_empty() {
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), tile.y), Sense::hover());
            ui.painter().rect_stroke(r.shrink(1.0), r_md(), Stroke::new(1.0, v.weak_text_color().gamma_multiply(0.35)), egui::StrokeKind::Inside);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "Favorites go here: right-click an app below to add it.", ui_font(13.0), v.weak_text_color());
        } else {
            egui::ScrollArea::horizontal().id_salt("launcher-favs").auto_shrink([false, true]).scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (j, a) in favs.iter().enumerate() {
                        let (r, resp) = ui.allocate_exact_size(tile, Sense::click());
                        let resp = resp.role(Role::ListBoxOption, a.name.as_str()).selected(j == *sel).described("Favorite");
                        if j == *sel {
                            ui.ctx().accesskit_node_builder(egui::Id::unique("launcher-search"), |n| n.set_active_descendant(resp.id.accesskit_id()));
                        }
                        mark(ui.painter(), r, j == *sel, resp.hovered());
                        let tex = kit.app_icon(ui.ctx(), a, icon);
                        if let (Some(at), Some(label)) = (draw_tile(ui.painter(), r, &a.name, tex, icon, &v, source_of(m.s, a)), source_of(m.s, a)) {
                            package_tip(ui, at, &format!("fav-{}", a.id), label);
                        }
                        if ui.rect_contains_pointer(r) && star_button(ui, Pos2::new(r.right() - 13.0, r.top() + 13.0), true, accent, &v, egui::Id::unique(("launcher-fav-star", &a.id))) {
                            toggle = Some(a.id.trim_end_matches(".desktop").to_string());
                        }
                        fav_menu(&resp, a, true, &mut toggle);
                        if j == *sel && moved {
                            ui.scroll_to_rect(r, None);
                        }
                        if resp.clicked() {
                            picked = Some(j);
                        }
                    }
                });
            });
        }
    }
    // Whole rows only: the list is as tall as the rows that fit, and while it scrolls a row
    // that's partly out of it isn't drawn (rather than cut through).
    let (pitch, gap) = if list { (list_icon(icon) + 14.0 + 2.0, 2.0) } else { pitch(m.s) };
    let room = ui.available_height() - if foot.is_some() { FOOT + 8.0 } else { 0.0 };
    let rows = ((room + gap) / pitch).floor().max(1.0);
    let whole = |ui: &egui::Ui, r: Rect| ui.clip_rect().expand(0.5).contains_rect(r);
    egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(rows * pitch - gap).show(ui, |ui| {
        if list {
            let li = list_icon(icon);
            ui.spacing_mut().item_spacing.y = 2.0;
            for (k, (_, a)) in ranked.iter().enumerate() {
                let chosen = nf + k == *sel;
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), li + 14.0), Sense::click());
                let resp = resp.role(Role::ListBoxOption, a.name().to_string()).selected(chosen);
                if chosen {
                    ui.ctx().accesskit_node_builder(egui::Id::unique("launcher-search"), |n| n.set_active_descendant(resp.id.accesskit_id()));
                }
                // Only what's in view is drawn (and only its icons made).
                let shown = whole(ui, r);
                // An app's star, at the end of its row: always on a favourite, else under the pointer.
                let star = match a {
                    Entry::App(app) if faving && shown => {
                        let on = is_fav(app);
                        (on || ui.rect_contains_pointer(r)).then_some((*app, on))
                    }
                    _ => None,
                };
                if shown {
                    mark(ui.painter(), r, chosen, resp.hovered());
                    let ir = Rect::from_center_size(Pos2::new(r.left() + 10.0 + li / 2.0, r.center().y), Vec2::splat(li));
                    if let Some(t) = a.icon(ui.ctx(), kit, li) {
                        // A picture's thumbnail keeps its shape inside the icon's square.
                        let tr = match a.aspect() {
                            Some(k) if k >= 1.0 => Rect::from_center_size(ir.center(), Vec2::new(li, li / k)),
                            Some(k) => Rect::from_center_size(ir.center(), Vec2::new(li * k, li)),
                            None => ir,
                        };
                        ui.painter().image(t.id(), tr, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    let end = r.right() - if star.is_some() { 30.0 } else { 0.0 };
                    let room = end - ir.right() - 24.0 - if chosen { 44.0 } else { 0.0 };
                    let g = crate::ui::one_line(ui.painter(), a.name(), ui_font(14.0), v.text_color(), room);
                    let after = ir.right() + 12.0 + g.size().x + 12.0;
                    ui.painter().galley(Pos2::new(ir.right() + 12.0, r.center().y - g.size().y / 2.0), g, v.text_color());
                    // What a plugin's entry says of itself, after its name.
                    let sub_room = ir.right() + 12.0 + room - after;
                    if !a.sub().is_empty() && sub_room > 40.0 {
                        let g = crate::ui::one_line(ui.painter(), a.sub(), ui_font(12.0), v.weak_text_color(), sub_room);
                        ui.painter().galley(Pos2::new(after, r.center().y - g.size().y / 2.0 + 0.5), g, v.weak_text_color());
                    }
                    // An app from a Flatpak, Snap or AppImage: a package after its name.
                    if let Entry::App(app) = a
                        && let Some(label) = source_of(m.s, app)
                        && sub_room > 20.0
                    {
                        let at = Rect::from_center_size(Pos2::new(after + 4.0, r.center().y), Vec2::splat(16.0));
                        crate::glyph::package(ui.painter(), at.center(), 6.0, v.weak_text_color());
                        package_tip(ui, at, &app.id, label);
                    }
                    if chosen {
                        ui.painter().text(Pos2::new(end - 12.0, r.center().y), Align2::RIGHT_CENTER, "Enter", ui_font(11.5), v.weak_text_color());
                    }
                }
                if let Some((app, on)) = star
                    && star_button(ui, Pos2::new(r.right() - 18.0, r.center().y), on, accent, &v, egui::Id::unique(("launcher-star", &app.id)))
                {
                    toggle = Some(app.id.trim_end_matches(".desktop").to_string());
                }
                if let Entry::App(app) = a
                    && faving
                {
                    fav_menu(&resp, app, is_fav(app), &mut toggle);
                }
                if chosen && moved {
                    ui.scroll_to_rect(r, None);
                }
                if shown && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    picked = Some(nf + k);
                }
            }
        } else {
            egui::Grid::new("launcher-grid").spacing([6.0, 6.0]).show(ui, |ui| {
                for (k, (_, a)) in ranked.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(tile, Sense::click());
                    let resp = resp.role(Role::ListBoxOption, a.name().to_string()).selected(nf + k == *sel);
                    if nf + k == *sel {
                        ui.ctx().accesskit_node_builder(egui::Id::unique("launcher-search"), |n| n.set_active_descendant(resp.id.accesskit_id()));
                    }
                    let shown = whole(ui, r);
                    if shown {
                        mark(ui.painter(), r, nf + k == *sel, resp.hovered());
                        let tex = a.icon(ui.ctx(), kit, icon);
                        let source = if let Entry::App(app) = a { source_of(m.s, app) } else { None };
                        if let (Some(at), Some(label), Entry::App(app)) = (draw_tile(ui.painter(), r, a.name(), tex, icon, &v, source), source, a) {
                            package_tip(ui, at, &app.id, label);
                        }
                        // An app's star, in its corner: always on a favourite, else under the pointer.
                        if let Entry::App(app) = a
                            && faving
                        {
                            let on = is_fav(app);
                            if (on || ui.rect_contains_pointer(r)) && star_button(ui, Pos2::new(r.right() - 13.0, r.top() + 13.0), on, accent, &v, egui::Id::unique(("launcher-star", &app.id))) {
                                toggle = Some(app.id.trim_end_matches(".desktop").to_string());
                            }
                            fav_menu(&resp, app, on, &mut toggle);
                        }
                    }
                    if nf + k == *sel && moved {
                        ui.scroll_to_rect(r, None);
                    }
                    if shown && resp.clicked() {
                        picked = Some(nf + k);
                    }
                    if (k + 1) % cols == 0 {
                        ui.end_row();
                    }
                }
            });
        }
        if ranked.is_empty() {
            let text = match &command {
                Some((word, _)) if word == CLIP_WORD && m.extra.clip.as_ref().is_some_and(|l| l.is_empty()) => "Text and pictures you copy appear here",
                Some((word, _)) if commands(m).iter().any(|(w, ..)| !word.is_empty() && w == word) => "No matches",
                Some(_) if !has_words => "No commands yet: plugins add them",
                Some(_) => "No command by that name",
                None => "No apps match",
            };
            ui.label(egui::RichText::new(text).color(v.weak_text_color()));
        }
    });
    if let Some(r) = foot {
        // Each with how much it matters: what doesn't fit goes from the least, so the
        // favourite key stays when the launcher is narrow.
        let keys: &[(&str, &str, u8)] = if in_favs {
            &[("← →", "Favorites", 2), ("↓", "All apps", 1), ("Enter", "Open", 0), ("Ctrl+D or right-click", "Remove", 3)]
        } else if nf > 0 {
            &[("↑", "Favorites", 2), ("Arrows", "Move", 1), ("Enter", "Open", 0), ("Ctrl+D or right-click", "Add to favorites", 3)]
        } else {
            &[("Arrows", "Move", 1), ("Enter", "Open", 0), ("Ctrl+D or right-click", "Add to favorites", 3)]
        };
        directions(ui.painter(), r, keys, &v);
    }
    if let Some(id) = toggle {
        m.acts.push(Act::Favorite(id));
    }
    // What was chosen: an app or a plugin's entry is told; a command's word is typed in, for
    // its entries to show.
    let mut fill = None;
    match picked {
        Some(k) if k < nf => m.acts.push(Act::Launch(favs[k].id.clone())),
        Some(k) => match ranked.get(k - nf).map(|(_, e)| e) {
            Some(Entry::Keyword { word, .. }) => fill = Some(format!("@{word} ")),
            Some(e) => m.acts.extend(e.acts()),
            None => {}
        },
        None => {}
    }
    if let Some(text) = fill {
        // It shows as a badge, with the field empty after it.
        *query = text;
        *sel = 0;
        cursor_to_end(ui.ctx(), id, 0);
    }
}

/// The height of the directions along the launcher's bottom.
const FOOT: f32 = 22.0;

/// The room under the favourites' row, with a line across its middle.
const FAV_GAP: f32 = 12.0;

/// Where an app came from, to mark it by, when the launcher marks them.
fn source_of(s: &Settings, app: &DesktopApp) -> Option<&'static str> {
    s.launch_source.then(|| app.source()).flatten().map(|x| x.label())
}

/// A grid tile's icon over two lines of its name, and, for an app from a Flatpak, Snap or
/// AppImage, a package on the icon's corner. Gives where that package is, to say which on hover.
pub(crate) fn draw_tile(p: &egui::Painter, r: Rect, name: &str, tex: Option<egui::TextureHandle>, icon: f32, v: &egui::Visuals, source: Option<&str>) -> Option<Rect> {
    let ir = Rect::from_center_size(Pos2::new(r.center().x, r.top() + 9.0 + icon / 2.0), Vec2::splat(icon));
    if let Some(t) = tex {
        p.image(t.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    }
    let badge = source.map(|_| {
        // Over the icon's lower right corner, kept inside the tile.
        let d = (icon * 0.42).clamp(15.0, 22.0);
        let c = Pos2::new((ir.right() + 2.0).min(r.right() - 3.0 - d / 2.0), ir.bottom() - 1.0);
        p.circle_filled(c, d / 2.0, v.panel_fill);
        p.circle_stroke(c, d / 2.0, Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.25)));
        crate::glyph::package(p, c, d * 0.3, v.text_color());
        Rect::from_center_size(c, Vec2::splat(d))
    });
    let mut job = egui::text::LayoutJob::single_section(name.to_string(), egui::TextFormat { font_id: ui_font(12.0), color: v.text_color(), ..Default::default() });
    job.wrap = egui::text::TextWrapping { max_width: r.width() - 8.0, max_rows: 2, break_anywhere: false, overflow_character: Some('…') };
    job.halign = egui::Align::Center;
    let g = p.layout_job(job);
    p.galley(Pos2::new(r.center().x, r.top() + icon + 16.0), g, v.text_color());
    badge
}

/// Which of Flatpak, Snap and AppImage a package mark is, said when the pointer is on it.
fn package_tip(ui: &egui::Ui, at: Rect, id: &str, label: &str) {
    ui.interact(at, egui::Id::unique(("launcher-source", id)), Sense::hover()).on_hover_text(label);
}

/// The star that adds an app to the favourites or takes it off: filled on a favourite. True
/// when it's clicked.
fn star_button(ui: &mut egui::Ui, at: Pos2, on: bool, accent: Color32, v: &egui::Visuals, id: egui::Id) -> bool {
    let r = Rect::from_center_size(at, Vec2::splat(22.0));
    let resp = ui.interact(r, id, Sense::click()).role(Role::Button, if on { "Remove from favorites" } else { "Add to favorites" }).toggled(on);
    if resp.hovered() {
        hover_fill(ui.painter(), r, v);
    }
    star(ui.painter(), r.center(), 7.0, on, if on { accent } else { v.text_color() });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(if on { "Remove from favorites (Ctrl+D)" } else { "Add to favorites (Ctrl+D)" }).clicked()
}

/// A five-pointed star, filled or outlined.
fn star(p: &egui::Painter, c: Pos2, r: f32, filled: bool, color: Color32) {
    use std::f32::consts::{FRAC_PI_2, PI};
    let pts: Vec<Pos2> = (0..10).map(|i| c + Vec2::angled(-FRAC_PI_2 + i as f32 * PI / 5.0) * if i % 2 == 0 { r } else { r * 0.45 }).collect();
    if filled {
        // A star isn't convex: its middle, then each point on it.
        p.add(egui::Shape::convex_polygon(pts.iter().skip(1).step_by(2).copied().collect(), color, Stroke::NONE));
        for i in (0..10).step_by(2) {
            p.add(egui::Shape::convex_polygon(vec![pts[(i + 9) % 10], pts[i], pts[i + 1]], color, Stroke::NONE));
        }
    }
    p.add(egui::Shape::closed_line(pts, Stroke::new(1.3, color)));
}

/// An app's right-click menu in the launcher: add it to the favourites or take it off.
fn fav_menu(resp: &egui::Response, app: &DesktopApp, on: bool, toggle: &mut Option<String>) {
    resp.context_menu(|ui| {
        if ui.button(if on { "Remove from Favorites" } else { "Add to Favorites" }).clicked() {
            *toggle = Some(app.id.trim_end_matches(".desktop").to_string());
            ui.close();
        }
    });
}

/// How to get around, along the bottom: each key in a chip, then what it does, centred. When
/// they don't all fit, the ones that matter least are left out.
fn directions(p: &egui::Painter, r: Rect, keys: &[(&str, &str, u8)], v: &egui::Visuals) {
    let chip = mix(v.panel_fill, v.text_color(), 0.1);
    let mut pieces: Vec<_> = keys.iter().map(|(k, what, rank)| (p.layout_no_wrap(k.to_string(), ui_font(11.0), v.text_color()), p.layout_no_wrap(what.to_string(), ui_font(11.5), v.weak_text_color()), *rank)).collect();
    let width = |(k, w, _): &(std::sync::Arc<egui::Galley>, std::sync::Arc<egui::Galley>, u8)| k.size().x + 10.0 + 5.0 + w.size().x;
    let total = |pieces: &[(std::sync::Arc<egui::Galley>, std::sync::Arc<egui::Galley>, u8)]| pieces.iter().map(width).sum::<f32>() + 16.0 * pieces.len().saturating_sub(1) as f32;
    while pieces.len() > 1 && total(&pieces) > r.width() {
        let least = pieces.iter().enumerate().min_by_key(|(_, (.., rank))| *rank).map(|(i, _)| i).unwrap_or(0);
        pieces.remove(least);
    }
    let mut x = r.center().x - total(&pieces) / 2.0;
    let y = r.center().y;
    for (k, w, _) in pieces {
        let cr = Rect::from_min_size(Pos2::new(x, y - 9.0), Vec2::new(k.size().x + 10.0, 18.0));
        p.rect_filled(cr, 4.0, chip);
        p.galley(Pos2::new(cr.left() + 5.0, y - k.size().y / 2.0), k, v.text_color());
        x = cr.right() + 5.0;
        let ww = w.size().x;
        p.galley(Pos2::new(x, y - w.size().y / 2.0), w, v.weak_text_color());
        x += ww + 16.0;
    }
}

/// The command locked in by the query, as its word, its plugin's name and its icon: `@word`
/// followed by a space, or a whole word that no other command's word goes on from.
fn locked(query: &str, m: &Model) -> Option<(String, String, String)> {
    let q = query.trim_start().strip_prefix('@')?;
    let (word, spaced) = match q.split_once(char::is_whitespace) {
        Some((w, _)) => (w.to_lowercase(), true),
        None => (q.to_lowercase(), false),
    };
    let cmds = commands(m);
    let (_, name, icon) = cmds.iter().find(|(w, ..)| !word.is_empty() && *w == word)?;
    (spaced || cmds.iter().all(|(k, ..)| *k == word || !k.starts_with(word.as_str()))).then(|| (word.clone(), name.clone(), icon.clone()))
}

/// Puts the search field's cursor after its first `chars` characters, next frame.
fn cursor_to_end(ctx: &egui::Context, id: egui::Id, chars: usize) {
    let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(chars))));
    st.store(ctx, id);
    ctx.request_repaint();
}
