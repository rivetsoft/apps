//! Widgets, on the two boards that hold them: the desktop, under the windows, and the HUD,
//! which opens over them from a key or a corner of the screen. Both are the same list of placed
//! widgets, drawn the same way; each board can be edited, where a widget is dragged to move it,
//! dragged by its corner to resize it, put on every display or only this one, or taken away.

use rivet_look::a11y::{Access, Role};
use chrono::Local;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{r_lg, r_md};

use crate::config::{Board, Edge, Placed, Side, WidgetKind};
use crate::ui::{Act, Kit, Model, card, find_app, hover_fill, one_line, ui_font};

/// A card's see-through, over the wallpaper or the HUD's backdrop.
const CARD: f32 = 0.82;
/// The corner of a widget that resizes it, in edit mode.
const GRIP: f32 = 22.0;
/// The grid widgets lock to while they're arranged: this many columns across the room, rows as
/// near to square as fit, and this much room left between two widgets side by side.
const GRID_COLS: f32 = 24.0;
const GRID_GAP: f32 = 12.0;
/// The room between the stacked desktop widgets that haven't been moved.
const STACK_GAP: f32 = 14.0;

/// The room widgets have on a screen: the screen less its panels, with a margin.
pub fn area(m: &Model, full: Rect) -> Rect {
    let side = |e: Edge| crate::panels::room(&m.s.panels, m.output, e);
    let r = Rect::from_min_max(Pos2::new(full.left() + side(Edge::Left) + 28.0, full.top() + side(Edge::Top) + 24.0), Pos2::new(full.right() - side(Edge::Right) - 28.0, full.bottom() - side(Edge::Bottom) - 24.0));
    if r.is_positive() { r } else { full }
}

/// The clock's two lines at a size, and the room they take.
fn clock_lines(p: &egui::Painter, scale: f32, fg: Color32) -> (std::sync::Arc<egui::Galley>, std::sync::Arc<egui::Galley>, Vec2) {
    let now = Local::now();
    let t = p.layout_no_wrap(now.format("%H:%M").to_string(), FontId::proportional(64.0 * scale), fg);
    let d = p.layout_no_wrap(now.format("%A, %B %-d").to_string(), ui_font(17.0 * scale), fg.gamma_multiply(0.88));
    let size = Vec2::new(t.size().x.max(d.size().x + 3.0 * scale), t.size().y + d.size().y - 4.0 * scale);
    (t, d, size)
}

/// The system card's height: three rows, the last with a bar under it only for the battery.
fn system_height(m: &Model) -> f32 {
    16.0 + 8.0 + 2.0 * 36.0 + if m.status.battery.is_some() { 15.0 } else { 8.0 } + 16.0
}

/// A widget's own size, which it has until it's resized.
fn natural(kind: WidgetKind, p: &egui::Painter, m: &Model) -> Vec2 {
    match kind {
        WidgetKind::Clock => clock_lines(p, 1.0, Color32::WHITE).2,
        WidgetKind::System => Vec2::new(280.0, system_height(m)),
        WidgetKind::Weather => Vec2::new(280.0, crate::weather_ui::card_height(&m.extra.weather)),
        WidgetKind::Media => Vec2::new(crate::media_pop::WIDTH, crate::media_pop::height(m).max(140.0)),
        WidgetKind::Calendar => Vec2::new(300.0, 300.0),
        WidgetKind::Events => Vec2::new(300.0, 260.0),
        WidgetKind::Apps => Vec2::new(540.0, 420.0),
    }
}

/// The smallest a widget can be made.
fn smallest(kind: WidgetKind) -> Vec2 {
    match kind {
        WidgetKind::Clock => Vec2::new(90.0, 44.0),
        WidgetKind::Apps => Vec2::new(200.0, 160.0),
        WidgetKind::Calendar => Vec2::new(240.0, 240.0),
        _ => Vec2::new(200.0, 110.0),
    }
}

/// Where a widget is in `area`: its size (its own, or the fraction of the room it was given)
/// and its place (the fraction of the room left over it was put at). One that hasn't been moved
/// is next in the stack on the desktop (`stack`: the next free place), or in the HUD's middle.
fn rect_of(w: &Placed, own: Vec2, area: Rect, side: Side, stack: &mut f32) -> Rect {
    let size = w.size.map(|(fw, fh)| Vec2::new(area.width() * fw, area.height() * fh)).unwrap_or(own).max(smallest(w.kind)).min(area.size());
    let room = (area.size() - size).max(Vec2::ZERO);
    let min = match (w.at, w.board) {
        (Some((fx, fy)), _) => area.min + Vec2::new(room.x * fx, room.y * fy),
        (None, Board::Desktop) => {
            let y = (*stack).min(area.top() + room.y);
            *stack += size.y + STACK_GAP;
            Pos2::new(if side == Side::Left { area.left() } else { area.right() - size.x }, y)
        }
        (None, Board::Hud) => area.min + room / 2.0,
    };
    Rect::from_min_size(min, size)
}

/// A rectangle in `area` as the fractions a widget is kept as: its place in the room left over,
/// and its size.
fn fractions(r: Rect, area: Rect) -> ((f32, f32), (f32, f32)) {
    let room = (area.size() - r.size()).max(Vec2::ZERO);
    let part = |d: f32, of: f32| if of > 0.5 { (d / of).clamp(0.0, 1.0) } else { 0.0 };
    ((part(r.left() - area.left(), room.x), part(r.top() - area.top(), room.y)), (part(r.width(), area.width()), part(r.height(), area.height())))
}

/// A grid cell's size in `area`, the gap after it counted in (so the last column ends at the
/// room's edge with no gap past it).
fn grid_cell(area: Rect) -> Vec2 {
    let w = (area.width() + GRID_GAP) / GRID_COLS;
    let rows = ((area.height() + GRID_GAP) / w).round().max(1.0);
    Vec2::new(w, (area.height() + GRID_GAP) / rows)
}

/// A rectangle locked to the grid: its top left on a grid line, and, when it's being sized
/// (`size`), as wide and tall as a whole number of cells less the gap. Kept inside the room.
fn snapped(r: Rect, area: Rect, size: bool, small: Vec2) -> Rect {
    let cell = grid_cell(area);
    let dims = if size {
        let n = ((r.size() + Vec2::splat(GRID_GAP)) / cell).round().max(((small + Vec2::splat(GRID_GAP)) / cell).ceil());
        (n * cell - Vec2::splat(GRID_GAP)).min(area.size())
    } else {
        r.size()
    };
    // The furthest line it can start on and still fit.
    let last = ((area.size() - dims) / cell + Vec2::splat(0.001)).floor().max(Vec2::ZERO);
    let at = ((r.min - area.min) / cell).round().clamp(Vec2::ZERO, last);
    Rect::from_min_size(area.min + at * cell, dims)
}

// ------------------------------------------------------------------ what each widget draws

fn clock(p: &egui::Painter, r: Rect) {
    let fg = Color32::WHITE;
    let own = clock_lines(p, 1.0, fg).2;
    // As large as its room lets it be, in its own proportions.
    let scale = (r.width() / own.x).min(r.height() / own.y).clamp(0.2, 6.0);
    let (t, d, _) = clock_lines(p, scale, fg);
    // A soft shadow behind the text, so it reads on any picture.
    for (g, at) in [(t.clone(), r.min), (d, r.min + Vec2::new(3.0 * scale, t.size().y - 4.0 * scale))] {
        p.galley(at + Vec2::new(0.0, 1.0), g.clone(), Color32::from_black_alpha(70));
        p.galley(at, g, fg);
    }
}

fn system(p: &egui::Painter, r: Rect, m: &Model, kit: &Kit, v: &egui::Visuals) {
    let st = m.status;
    let accent = kit.accent(m.s);
    card(p, r, v, CARD, r_lg());
    let p = p.with_clip_rect(r.intersect(p.clip_rect()));
    let pad = 16.0;
    let first = pad + 8.0;
    // The rows spread over a taller card.
    let step = ((r.height() - first - pad - if st.battery.is_some() { 15.0 } else { 8.0 }) / 2.0).max(36.0);
    let (txt, weak) = (v.text_color(), v.weak_text_color());
    let (bar2, bar3) = (kit.tone(m.s, crate::style::Slot::Bar2), kit.tone(m.s, crate::style::Slot::Bar3));
    let row = |label: &str, value: String, frac: f32, yy: f32, accent: Color32| {
        p.text(Pos2::new(r.left() + pad, yy), Align2::LEFT_CENTER, label, ui_font(12.5), weak);
        p.text(Pos2::new(r.right() - pad, yy), Align2::RIGHT_CENTER, value, ui_font(12.5), txt);
        let bar = Rect::from_min_size(Pos2::new(r.left() + pad, yy + 10.0), Vec2::new(r.width() - 2.0 * pad, 5.0));
        p.rect_filled(bar, 3.0, mix(v.panel_fill, txt, 0.12));
        p.rect_filled(Rect::from_min_size(bar.min, Vec2::new(bar.width() * frac.clamp(0.0, 1.0), bar.height())), 3.0, accent);
    };
    let ys = [r.top() + first, r.top() + first + step, r.top() + first + 2.0 * step];
    row("Processor", format!("{:.0}%", st.cpu), st.cpu / 100.0, ys[0], accent);
    let (used, total) = st.mem;
    let gb = |b: u64| b as f32 / (1u64 << 30) as f32;
    row("Memory", format!("{:.1} of {:.0} GB", gb(used), gb(total)), if total > 0 { used as f32 / total as f32 } else { 0.0 }, ys[1], bar2);
    match st.battery {
        Some((pct, charging)) => row("Battery", format!("{pct}%{}", if charging { ", charging" } else { "" }), pct as f32 / 100.0, ys[2], bar3),
        None => {
            let up = crate::sys::uptime();
            p.text(Pos2::new(r.left() + pad, ys[2]), Align2::LEFT_CENTER, "Up for", ui_font(12.5), weak);
            p.text(Pos2::new(r.right() - pad, ys[2]), Align2::RIGHT_CENTER, format!("{} h {} min", up / 3600, up / 60 % 60), ui_font(12.5), txt);
        }
    }
}

/// A card's inside as a place to lay things out, cut off at the card's edge.
fn inside(ui: &mut egui::Ui, r: Rect, pad: f32) -> egui::Ui {
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink(pad)).layout(egui::Layout::top_down(egui::Align::Min)));
    c.set_clip_rect(r.shrink(1.0).intersect(ui.clip_rect()));
    c
}

/// The events coming up, soonest first, each day under its name: from the plugins that give
/// the calendar its events. A click opens one in its app.
fn events(ui: &mut egui::Ui, r: Rect, m: &mut Model, accent: Color32) {
    let v = ui.visuals().clone();
    let mut c = inside(ui, r, 14.0);
    c.spacing_mut().item_spacing.y = 0.0;
    let w = c.available_width();
    let (hr, _) = c.allocate_exact_size(Vec2::new(w, 26.0), Sense::hover());
    c.painter().text(Pos2::new(hr.left() + 2.0, hr.center().y - 2.0), Align2::LEFT_CENTER, "Coming up", ui_font(14.5), v.strong_text_color());
    let today = Local::now().date_naive();
    let mut list: Vec<(String, crate::plugins::DayEvent)> = m.extra.plugins.iter().flat_map(|(id, it)| it.events.iter().filter(|e| e.day.is_some_and(|d| d >= today)).map(move |e| (id.clone(), e.clone()))).collect();
    list.sort_by(|a, b| (a.1.day, !a.1.time.is_empty(), &a.1.time).cmp(&(b.1.day, !b.1.time.is_empty(), &b.1.time)));
    if list.is_empty() {
        let none = if crate::ui::has_calendar(m) { "Nothing coming up." } else { "Turn on a calendar plugin to see events here." };
        let g = crate::ui::wrapped(c.painter(), none, ui_font(13.0), v.weak_text_color(), w - 4.0, 3);
        c.painter().galley(Pos2::new(hr.left() + 2.0, hr.bottom() + 8.0), g, v.weak_text_color());
        return;
    }
    let mut open: Option<(String, String)> = None;
    let mut day = None;
    for (pid, e) in &list {
        // No part of a row past the card's foot.
        if c.available_height() < if day != e.day { 54.0 } else { 30.0 } {
            break;
        }
        if day != e.day {
            day = e.day;
            let name = match e.day {
                Some(d) if d == today => "Today".to_string(),
                Some(d) if d == today + chrono::Duration::days(1) => "Tomorrow".to_string(),
                Some(d) => d.format("%A %-d %B").to_string(),
                None => String::new(),
            };
            let (dr, _) = c.allocate_exact_size(Vec2::new(w, 24.0), Sense::hover());
            c.painter().text(Pos2::new(dr.left() + 2.0, dr.center().y + 2.0), Align2::LEFT_CENTER, name, ui_font(11.5), v.weak_text_color());
        }
        let (row, resp) = c.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
        let resp = resp.role(Role::Button, e.title.as_str()).described(e.time.as_str());
        if resp.hovered() {
            hover_fill(c.painter(), row, &v);
        }
        let col = e.color.map(|[r, g, b]| Color32::from_rgb(r, g, b)).unwrap_or(accent);
        c.painter().rect_filled(Rect::from_min_size(Pos2::new(row.left() + 4.0, row.top() + 7.0), Vec2::new(3.0, row.height() - 14.0)), 1.5, col);
        let time = if e.time.is_empty() { "All day" } else { e.time.as_str() };
        let tg = c.painter().layout_no_wrap(time.to_string(), ui_font(11.5), v.weak_text_color());
        c.painter().galley(Pos2::new(row.left() + 14.0, row.center().y - tg.size().y / 2.0), tg, v.weak_text_color());
        let g = one_line(c.painter(), &e.title, ui_font(13.0), v.text_color(), row.width() - 84.0);
        c.painter().galley(Pos2::new(row.left() + 72.0, row.center().y - g.size().y / 2.0), g, v.text_color());
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            open = Some((pid.clone(), e.id.clone()));
        }
    }
    if let Some((pid, id)) = open {
        m.acts.push(Act::Close);
        m.acts.push(Act::Plugin(pid, crate::plugins::Event::new("open", &[("id", &id)])));
    }
}

/// The apps `query` finds (all of them when it's empty), the launcher's favorites first.
fn found<'a>(m: &'a Model, query: &str) -> Vec<&'a rivet_xdg::launch::DesktopApp> {
    let q = query.trim().to_lowercase();
    let favs: Vec<&rivet_xdg::launch::DesktopApp> = m.s.launch_favorite_apps.iter().filter_map(|id| find_app(m.apps, id)).collect();
    let rest = m.apps.iter().filter(|a| !favs.iter().any(|f| f.id == a.id));
    favs.iter().copied().chain(rest).filter(|a| q.is_empty() || a.name.to_lowercase().contains(&q)).collect()
}

/// A grid of apps, as many across as fit, scrolling when there are more. `query`: what has
/// been typed on the HUD, which narrows them.
fn apps(ui: &mut egui::Ui, r: Rect, m: &mut Model, kit: &mut Kit, query: &str, idx: usize) {
    let v = ui.visuals().clone();
    let mut c = inside(ui, r, 12.0);
    let icon = 48.0;
    let (tile, gap) = (Vec2::new(icon + 52.0, icon + 50.0), 6.0);
    if !query.trim().is_empty() {
        let (qr, _) = c.allocate_exact_size(Vec2::new(c.available_width(), 26.0), Sense::hover());
        c.painter().text(Pos2::new(qr.left() + 4.0, qr.center().y - 2.0), Align2::LEFT_CENTER, query, ui_font(15.0), v.strong_text_color());
    }
    let list = found(m, query);
    if list.is_empty() {
        let at = c.available_rect_before_wrap();
        c.painter().text(Pos2::new(at.left() + 4.0, at.top() + 14.0), Align2::LEFT_CENTER, "No app has that in its name.", ui_font(13.0), v.weak_text_color());
        return;
    }
    let w = c.available_width();
    let cols = (((w + gap) / (tile.x + gap)).floor() as usize).max(1);
    let rows = list.len().div_ceil(cols);
    // The grid in the middle of the card's width.
    let left = ((w - (cols as f32 * (tile.x + gap) - gap)) / 2.0).max(0.0);
    let mut launch = None;
    egui::ScrollArea::vertical().id_salt(("board-apps", idx)).auto_shrink([false, false]).show(&mut c, |ui| {
        let (grid, _) = ui.allocate_exact_size(Vec2::new(w, rows as f32 * (tile.y + gap) - gap), Sense::hover());
        for (k, a) in list.iter().enumerate() {
            let tr = Rect::from_min_size(grid.min + Vec2::new(left + (k % cols) as f32 * (tile.x + gap), (k / cols) as f32 * (tile.y + gap)), tile);
            if !ui.is_rect_visible(tr) {
                continue;
            }
            let resp = ui.interact(tr, egui::Id::unique(("board-app", idx, &a.id)), Sense::click()).role(Role::Button, a.name.as_str());
            if resp.hovered() {
                ui.painter().rect_filled(tr, r_md(), mix(v.panel_fill, v.text_color(), 0.08));
            }
            let tex = kit.app_icon(ui.ctx(), a, icon);
            crate::launcher::draw_tile(ui.painter(), tr, &a.name, tex, icon, &v, None);
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                launch = Some(a.id.clone());
            }
        }
    });
    if let Some(id) = launch {
        m.acts.push(Act::Close);
        m.acts.push(Act::Launch(id));
    }
}

/// One widget, in its rectangle.
fn draw(ui: &mut egui::Ui, kind: WidgetKind, r: Rect, m: &mut Model, kit: &mut Kit, query: &str, idx: usize) {
    let v = ui.visuals().clone();
    let accent = kit.accent(m.s);
    let p = ui.painter().clone();
    match kind {
        WidgetKind::Clock => clock(&p, r),
        WidgetKind::System => system(&p, r, m, kit, &v),
        WidgetKind::Weather => {
            card(&p, r, &v, CARD, r_lg());
            crate::weather_ui::card(&p.with_clip_rect(r.intersect(p.clip_rect())), r, &m.extra.weather, m.s.weather_units, crate::weather_ui::Tones::of(&v, accent));
        }
        WidgetKind::Media => {
            card(&p, r, &v, CARD, r_lg());
            let mut c = inside(ui, r, crate::media_pop::PAD);
            crate::media_pop::popup(&mut c, m, kit);
        }
        WidgetKind::Calendar => {
            card(&p, r, &v, CARD, r_lg());
            let mut c = inside(ui, r, 14.0);
            // The month it shows and the day picked are the widget's own, while it's up.
            let id = egui::Id::unique(("board-calendar", idx));
            let (mut month, mut day) = ui.ctx().data(|d| d.get_temp::<(i32, Option<chrono::NaiveDate>)>(id)).unwrap_or((0, None));
            crate::ui::calendar(&mut c, &mut month, &mut day, accent, m);
            ui.ctx().data_mut(|d| d.insert_temp(id, (month, day)));
        }
        WidgetKind::Events => {
            card(&p, r, &v, CARD, r_lg());
            events(ui, r, m, accent);
        }
        WidgetKind::Apps => {
            card(&p, r, &v, CARD, r_lg());
            apps(ui, r, m, kit, query, idx);
        }
    }
}

// ------------------------------------------------------------------ a board, and editing it

/// A widget held in edit mode: which, the rectangle it had when it was taken, and whether it's
/// being resized (else moved).
#[derive(Clone, Copy)]
struct Held {
    idx: usize,
    from: Rect,
    resize: bool,
    /// Where the pointer pressed it: kept here, since by the frame the button is let go egui
    /// no longer says, and the place to save is worked out on that frame.
    origin: Pos2,
}

fn held_id() -> egui::Id {
    egui::Id::unique("board-held")
}

fn picked_id() -> egui::Id {
    egui::Id::unique("board-picked")
}

/// A small button drawn on a board: its label on a pill. Gives its rectangle and whether it
/// was clicked. `strong`: on the accent.
fn chip(ui: &mut egui::Ui, at: Pos2, label: &str, id: egui::Id, strong: Option<Color32>) -> (Rect, bool) {
    let v = ui.visuals().clone();
    let g = ui.painter().layout_no_wrap(label.to_string(), ui_font(12.5), v.text_color());
    let r = Rect::from_min_size(at, Vec2::new(g.size().x + 22.0, 28.0));
    let resp = ui.interact(r, id, Sense::click()).role(Role::Button, label);
    let fill = match strong {
        Some(a) => if resp.hovered() { mix(a, v.text_color(), 0.12) } else { a },
        None => mix(v.panel_fill, v.text_color(), if resp.hovered() { 0.16 } else { 0.08 }),
    };
    ui.painter().rect_filled(r, 14.0, fill);
    let col = strong.map(crate::ui::on).unwrap_or(v.text_color());
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, ui_font(12.5), col);
    (r, resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked())
}

/// How wide a chip with this label is.
fn chip_width(ui: &egui::Ui, label: &str) -> f32 {
    ui.painter().layout_no_wrap(label.to_string(), ui_font(12.5), Color32::WHITE).size().x + 22.0
}

/// The widgets of a board on this screen, each in its place. `query`: what's typed on the HUD.
/// In edit mode (`edit`) nothing in them answers the pointer: they are moved, resized, set to
/// every display or this one, and taken away.
fn widgets(ui: &mut egui::Ui, board: Board, edit: bool, query: &str, m: &mut Model, kit: &mut Kit) {
    let full = ui.max_rect();
    let area = area(m, full);
    let p = ui.painter().clone();
    let accent = kit.accent(m.s);
    let v = ui.visuals().clone();
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let held: Option<Held> = if edit { ui.ctx().data(|d| d.get_temp(held_id())) } else { None };
    let mut picked: Option<usize> = if edit { ui.ctx().data(|d| d.get_temp(picked_id())) } else { None };
    let grid = edit && if board == Board::Hud { m.s.hud_grid } else { m.s.desktop_grid };
    // The grid shows while something is held: a dot where its lines cross.
    if grid && held.is_some() {
        let cell = grid_cell(area);
        let (cols, rows) = ((area.width() / cell.x).round() as usize + 1, (area.height() / cell.y).round() as usize + 1);
        for i in 0..cols {
            for j in 0..rows {
                p.circle_filled(area.min + Vec2::new(i as f32 * cell.x, j as f32 * cell.y), 1.5, Color32::from_white_alpha(70));
            }
        }
    }
    let mut stack = area.top();
    let mut list: Vec<(usize, Placed)> = m.s.widgets.iter().cloned().enumerate().filter(|(_, w)| w.board == board && w.on(m.output)).collect();
    // The one held is drawn last, over the rest.
    list.sort_by_key(|(i, _)| held.is_some_and(|h| h.idx == *i));
    let mut bar: Option<(usize, Placed, Rect)> = None;
    for (idx, w) in &list {
        let (idx, small) = (*idx, smallest(w.kind));
        let mut r = rect_of(w, natural(w.kind, &p, m), area, m.s.widget_side, &mut stack);
        // Held: where the pointer has taken it, or the size it has been dragged to.
        let mine = held.filter(|h| h.idx == idx);
        if let (Some(h), Some(now)) = (mine, pointer) {
            let by = now - h.origin;
            r = if h.resize {
                let max = area.max - h.from.min;
                Rect::from_min_size(h.from.min, (h.from.size() + by).max(small).min(max.max(small)))
            } else {
                let room = (area.size() - h.from.size()).max(Vec2::ZERO);
                Rect::from_min_size((h.from.min + by).clamp(area.min, area.min + room), h.from.size())
            };
            if grid {
                // Sized once, it keeps to whole cells; at its own size only its corner locks.
                r = snapped(r, area, h.resize || w.size.is_some(), small);
            }
        }
        draw(ui, w.kind, r, m, kit, query, idx);
        if !edit {
            continue;
        }
        // Over what it drew, so the widget itself takes the pointer and nothing in it does.
        let body = ui.interact(r, egui::Id::unique(("board-move", idx)), Sense::click_and_drag()).role(Role::GenericContainer, "Widget");
        let grip = Rect::from_min_max(r.max - Vec2::splat(GRIP), r.max);
        let corner = ui.interact(grip, egui::Id::unique(("board-size", idx)), Sense::click_and_drag()).role(Role::Button, "Resize widget");
        for (resp, resize) in [(&body, false), (&corner, true)] {
            if resp.drag_started() || resp.clicked() {
                picked = Some(idx);
            }
            if resp.drag_started()
                && let Some(origin) = ui.input(|i| i.pointer.press_origin()).or(pointer)
            {
                ui.ctx().data_mut(|d| d.insert_temp(held_id(), Held { idx, from: r, resize, origin }));
            }
        }
        if mine.is_some() && (body.drag_stopped() || corner.drag_stopped()) {
            ui.ctx().data_mut(|d| d.remove::<Held>(held_id()));
            let (at, size) = fractions(r, area);
            // Moved only, it keeps its own size; resized once, it keeps the size it was given.
            let size = (mine.is_some_and(|h| h.resize) || w.size.is_some()).then_some(size);
            m.acts.push(Act::WidgetPut(idx, Placed { at: Some(at), size, ..w.clone() }));
        }
        let chosen = picked == Some(idx);
        let line = if chosen { Stroke::new(2.0, accent) } else { Stroke::new(1.0, Color32::from_white_alpha(110)) };
        p.rect_stroke(r.expand(3.0), r_lg(), line, egui::StrokeKind::Outside);
        // The corner to drag: three short lines across it.
        let gc = if chosen || corner.hovered() { accent } else { Color32::from_white_alpha(170) };
        for k in 1..=3 {
            let d = k as f32 * 4.5;
            p.line_segment([Pos2::new(r.right() - d - 2.0, r.bottom() - 3.0), Pos2::new(r.right() - 3.0, r.bottom() - d - 2.0)], Stroke::new(1.5, gc));
        }
        if corner.hovered() || mine.is_some_and(|h| h.resize) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
        } else if mine.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if body.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if chosen && held.is_none() {
            bar = Some((idx, w.clone(), r));
        }
    }
    if !edit {
        return;
    }
    // The chosen widget's own row: which displays it's on, its own size again, and away.
    if let Some((idx, w, r)) = bar {
        let here = !m.output.is_empty();
        let on = if w.output.is_empty() { "On Every Display".to_string() } else { format!("Only on {}", w.output) };
        let mut labels = vec![on, "Remove".to_string()];
        if w.size.is_some() {
            labels.insert(1, "Its Own Size".to_string());
        }
        let width: f32 = labels.iter().map(|l| chip_width(ui, l) + 6.0).sum::<f32>() + 6.0;
        // Under it; over it at the screen's foot; inside its top when it's as tall as the
        // screen (clear of the bar along the top either way).
        let y = if r.bottom() + 46.0 <= full.bottom() {
            r.bottom() + 9.0
        } else if r.top() - 49.0 >= full.top() + 60.0 {
            r.top() - 49.0
        } else {
            r.top() + 10.0
        };
        let back = Rect::from_min_size(Pos2::new(r.left().clamp(full.left() + 4.0, (full.right() - width - 4.0).max(full.left() + 4.0)), y), Vec2::new(width, 40.0));
        card(&p, back, &v, 0.98, 20.0);
        let mut x = back.left() + 6.0;
        for l in labels {
            let (cr, clicked) = chip(ui, Pos2::new(x, back.top() + 6.0), &l, egui::Id::unique(("board-chip", idx, &l)), None);
            x = cr.right() + 6.0;
            if !clicked {
                continue;
            }
            match l.as_str() {
                "Remove" => {
                    picked = None;
                    m.acts.push(Act::WidgetRemove(idx));
                }
                "Its Own Size" => m.acts.push(Act::WidgetPut(idx, Placed { size: None, ..w.clone() })),
                // Every display, or the one this is being edited on (when it's known).
                _ if w.output.is_empty() && here => m.acts.push(Act::WidgetPut(idx, Placed { output: m.output.to_string(), ..w.clone() })),
                _ => m.acts.push(Act::WidgetPut(idx, Placed { output: String::new(), ..w.clone() })),
            }
        }
    }
    // The board's own bar, at the top: what is being edited, what can be added, and Done.
    let title = if board == Board::Hud { "Editing the HUD" } else { "Editing the desktop's widgets" };
    // One of a kind shows on a screen at a time.
    let addable: Vec<WidgetKind> = WidgetKind::ALL.into_iter().filter(|k| !list.iter().any(|(_, w)| w.kind == *k)).collect();
    let tg = p.layout_no_wrap(title.to_string(), ui_font(14.0), v.strong_text_color());
    let adds: f32 = addable.iter().map(|k| chip_width(ui, &format!("Add {}", k.label())) + 6.0).sum();
    let done = chip_width(ui, "Done");
    let lock = chip_width(ui, "Lock to Grid") + 6.0;
    let one_row = 16.0 + tg.size().x + 16.0 + adds + lock + done + 10.0;
    let wide = one_row.min(full.width() - 16.0);
    let two = one_row > full.width() - 16.0;
    let back = Rect::from_min_size(Pos2::new(full.center().x - wide / 2.0, full.top() + 10.0), Vec2::new(wide, if two { 76.0 } else { 44.0 }));
    card(&p, back, &v, 0.98, 22.0);
    p.galley(Pos2::new(back.left() + 16.0, back.top() + 22.0 - tg.size().y / 2.0), tg.clone(), v.strong_text_color());
    let (mut x, y) = if two { (back.left() + 10.0, back.top() + 42.0) } else { (back.left() + 16.0 + tg.size().x + 16.0, back.top() + 8.0) };
    for k in addable {
        let (cr, clicked) = chip(ui, Pos2::new(x, y), &format!("Add {}", k.label()), egui::Id::unique(("board-add", k)), None);
        x = cr.right() + 6.0;
        if clicked {
            // In the middle, at its own size, to be dragged from there.
            m.acts.push(Act::WidgetAdd(Placed { at: Some((0.5, 0.5)), ..Placed::new(k, board) }));
        }
    }
    // On: in the accent's tint. Dragged widgets then keep to the grid's lines.
    let (_, clicked) = chip(ui, Pos2::new(back.right() - done - 8.0 - lock, back.top() + 8.0), "Lock to Grid", egui::Id::unique("board-grid"), grid.then(|| mix(v.panel_fill, accent, 0.55)));
    if clicked {
        m.acts.push(Act::BoardGrid(board, !grid));
    }
    let (_, clicked) = chip(ui, Pos2::new(back.right() - done - 8.0, back.top() + 8.0), "Done", egui::Id::unique("board-done"), Some(accent));
    if clicked {
        m.acts.push(Act::BoardDone);
    }
    ui.ctx().data_mut(|d| match picked {
        Some(i) => {
            d.insert_temp(picked_id(), i);
        }
        None => d.remove::<usize>(picked_id()),
    });
}

/// The desktop's widgets, on the wallpaper.
pub fn desktop(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    widgets(ui, Board::Desktop, false, "", m, kit);
}

/// The desktop's widgets being edited, over the windows: on the wallpaper again (the shell puts
/// it behind this), a little darker.
pub fn desktop_edit(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, Color32::from_black_alpha(70));
    // A click on nothing lets go of the widget chosen.
    if ui.interact(full, egui::Id::unique("board-back"), Sense::CLICK).role(Role::GenericContainer, "Desktop").clicked() {
        ui.ctx().data_mut(|d| d.remove::<usize>(picked_id()));
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        m.acts.push(Act::Close);
    }
    widgets(ui, Board::Desktop, true, "", m, kit);
}

/// The HUD: its widgets over everything, on a dark, blurred backdrop. Typing narrows its apps
/// (Enter opens the first); Esc, or a click on nothing, closes it.
pub fn hud(ui: &mut egui::Ui, edit: &mut bool, query: &mut String, m: &mut Model, kit: &mut Kit) {
    let full = ui.max_rect();
    let mut v = ui.visuals().clone();
    // The backdrop: what's under it blurred where the compositor can, and darkened.
    v.panel_fill = Color32::from_rgb(8, 8, 10);
    crate::ui::surface(ui.painter(), full, &v, 0.74, 0.0, false);
    let back = ui.interact(full, egui::Id::unique("board-back"), Sense::CLICK).role(Role::GenericContainer, "Desktop");
    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if *edit {
        if back.clicked() {
            ui.ctx().data_mut(|d| d.remove::<usize>(picked_id()));
        }
        if esc {
            m.acts.push(Act::BoardDone);
        }
        widgets(ui, Board::Hud, true, "", m, kit);
        return;
    }
    let has_apps = m.s.widgets.iter().any(|w| w.board == Board::Hud && w.kind == WidgetKind::Apps && w.on(m.output));
    if has_apps {
        // What's typed goes to the apps: there is no box to click first.
        let (mut enter, mut erase) = (false, false);
        ui.input(|i| {
            for e in &i.events {
                match e {
                    egui::Event::Text(t) => query.push_str(t),
                    egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => erase = true,
                    egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } => enter = true,
                    _ => {}
                }
            }
        });
        if erase {
            query.pop();
        }
        let first = found(m, query).first().map(|a| a.id.clone());
        if enter && !query.trim().is_empty()
            && let Some(id) = first
        {
            m.acts.push(Act::Close);
            m.acts.push(Act::Launch(id));
        }
    }
    // Esc clears what was typed first, then closes.
    if esc && !query.is_empty() {
        query.clear();
    } else if esc || back.clicked() || back.secondary_clicked() {
        m.acts.push(Act::Close);
    }
    widgets(ui, Board::Hud, false, query, m, kit);
    let label = "Edit";
    let at = Pos2::new(full.right() - chip_width(ui, label) - 14.0, full.bottom() - 42.0);
    if chip(ui, at, label, egui::Id::unique("board-edit"), None).1 {
        query.clear();
        *edit = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A widget pressed, dragged and let go in edit mode is saved where it was let go (the
    /// frame the button comes up is the one that saves it).
    #[test]
    fn a_dragged_widget_stays_where_it_is_dropped() {
        let s = crate::config::Settings { widgets: vec![Placed::new(WidgetKind::System, Board::Desktop)], panels: Vec::new(), ..Default::default() };
        let (status, extra) = (crate::sys::Status::default(), crate::ui::Extra::default());
        let full = Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0));
        let ctx = egui::Context::default();
        let mut kit = Kit::new(false);
        let mut time = 0.0;
        let mut frame = |events: Vec<egui::Event>| {
            let mut acts = Vec::new();
            time += 0.05;
            let raw = egui::RawInput { screen_rect: Some(full), time: Some(time), events, ..Default::default() };
            let mut out = ctx.run_ui(raw, |ui| {
                kit.begin(ui.ctx(), &s);
                let mut m = Model { s: &s, status: &status, extra: &extra, apps: &[], compositor: crate::sys::Compositor::Other, output: "", acts: &mut acts };
                desktop_edit(ui, &mut m, &mut kit);
            });
            out.textures_delta.clear();
            acts
        };
        let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        // The card starts at the top left of the room: pressed 50 points in, carried 300 by 200.
        let (from, to) = (Pos2::new(78.0, 74.0), Pos2::new(378.0, 274.0));
        frame(vec![egui::Event::PointerMoved(from)]);
        frame(vec![button(from, true)]);
        for k in 1..=5 {
            assert!(frame(vec![egui::Event::PointerMoved(from + (to - from) * (k as f32 / 5.0))]).is_empty());
        }
        let acts = frame(vec![button(to, false)]);
        let [Act::WidgetPut(0, w)] = acts.as_slice() else { panic!("{acts:?}") };
        let mut stack = 0.0;
        let r = rect_of(w, Vec2::new(280.0, system_height(&Model { s: &s, status: &status, extra: &extra, apps: &[], compositor: crate::sys::Compositor::Other, output: "", acts: &mut Vec::new() })), area_of(full), Side::Left, &mut stack);
        assert!((r.left() - 328.0).abs() < 1.0 && (r.top() - 224.0).abs() < 1.0, "{r:?}");
        assert_eq!(w.size, None);
    }

    /// The room with no panels.
    fn area_of(full: Rect) -> Rect {
        Rect::from_min_max(full.min + Vec2::new(28.0, 24.0), full.max - Vec2::new(28.0, 24.0))
    }

    #[test]
    fn a_place_and_size_come_back_as_they_were_kept() {
        let area = Rect::from_min_size(Pos2::new(28.0, 60.0), Vec2::new(1200.0, 700.0));
        let mut stack = area.top();
        let w = Placed { at: Some((0.5, 1.0)), size: Some((0.25, 0.4)), ..Placed::new(WidgetKind::Calendar, Board::Hud) };
        let r = rect_of(&w, Vec2::new(300.0, 300.0), area, Side::Left, &mut stack);
        assert_eq!(r.size(), Vec2::new(300.0, 280.0));
        assert!((r.bottom() - area.bottom()).abs() < 0.01 && (r.center().x - area.center().x).abs() < 0.01);
        let (at, size) = fractions(r, area);
        assert!((at.0 - 0.5).abs() < 1e-4 && (at.1 - 1.0).abs() < 1e-4 && (size.0 - 0.25).abs() < 1e-4 && (size.1 - 0.4).abs() < 1e-4);
        // Not moved: desktop widgets stack down the side chosen, and the HUD's sit in its middle.
        let a = rect_of(&Placed::new(WidgetKind::System, Board::Desktop), Vec2::new(280.0, 120.0), area, Side::Right, &mut stack);
        let b = rect_of(&Placed::new(WidgetKind::Weather, Board::Desktop), Vec2::new(280.0, 200.0), area, Side::Right, &mut stack);
        assert_eq!((a.right(), a.top(), b.top()), (area.right(), area.top(), area.top() + 120.0 + STACK_GAP));
        let c = rect_of(&Placed::new(WidgetKind::Events, Board::Hud), Vec2::new(300.0, 260.0), area, Side::Left, &mut stack);
        assert_eq!(c.center(), area.center());
        // Locked to the grid: its corner on a line, whole cells with the gap between, in the room.
        let cell = grid_cell(area);
        assert!((cell.x - (1200.0 + GRID_GAP) / 24.0).abs() < 1e-3 && (cell.y - cell.x).abs() < cell.x * 0.1);
        let g = snapped(Rect::from_min_size(area.min + Vec2::new(cell.x * 2.4, cell.y * 0.6), Vec2::new(cell.x * 5.1, cell.y * 3.6)), area, true, Vec2::new(90.0, 44.0));
        assert!((g.left() - (area.left() + 2.0 * cell.x)).abs() < 0.01 && (g.top() - (area.top() + cell.y)).abs() < 0.01);
        assert!((g.width() - (5.0 * cell.x - GRID_GAP)).abs() < 0.01 && (g.height() - (4.0 * cell.y - GRID_GAP)).abs() < 0.01);
        // The last column ends at the room's edge, and nothing is pushed past it.
        let far = snapped(Rect::from_min_size(area.max, Vec2::new(cell.x * 3.0 - GRID_GAP, 100.0)), area, false, Vec2::ZERO);
        assert!((far.right() - area.right()).abs() < 0.01 && far.bottom() <= area.bottom() + 0.01);
        // Never smaller than it can be, nor bigger than the room.
        let tiny = Placed { size: Some((0.02, 0.02)), ..Placed::new(WidgetKind::Apps, Board::Hud) };
        assert_eq!(rect_of(&tiny, Vec2::ZERO, area, Side::Left, &mut stack).size(), smallest(WidgetKind::Apps));
    }
}
