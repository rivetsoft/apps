//! Super todos (Preferences → Modules): a task tagged with a todo tag gets a small button in
//! the editor that opens a popover for its due date, priority and tags, and every super todo in
//! the vault is rolled up on the Todos page, which can be sorted and filtered.

use chrono::{Datelike, NaiveDate};
use eframe::egui::{self, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use eframe::egui::text::CCursor;
use rivet_look::theme::{r_md, r_sm, r_xs, status};
use rivet_look::{fonts, palette};

use super::*;
use crate::tasks::{self, Item, Priority};

/// A change to a task, from its popover.
type TaskChange = Box<dyn FnOnce(&mut Item)>;

/// The popover for one task in the open note.
pub(super) struct TaskPop {
    pub line: usize,
    /// Where it hangs from (under the task's button).
    pub at: Pos2,
    /// Opened this frame (the click that opened it doesn't close it).
    pub fresh: bool,
    /// What's typed in its "add tag" box.
    pub tag_input: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) enum Show {
    #[default]
    Open,
    Doing,
    Done,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) enum When {
    #[default]
    Any,
    Overdue,
    Today,
    Week,
    NoDate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) enum Sort {
    #[default]
    Due,
    Priority,
    Note,
}

/// How the Todos page is sorted and filtered.
#[derive(Clone, Debug, Default)]
pub(super) struct TodoView {
    pub query: String,
    pub show: Show,
    pub when: When,
    pub sort: Sort,
    pub tag: Option<String>,
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// A due date in words: Today, Tomorrow, Friday, 3 days late, 20 Nov…
pub(super) fn due_label(d: NaiveDate) -> String {
    let t = today();
    match (d - t).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        -1 => "Yesterday".into(),
        n if n < -1 => format!("{} days late", -n),
        n if (2..7).contains(&n) => d.format("%A").to_string(),
        _ if d.year() == t.year() => d.format("%-d %b").to_string(),
        _ => d.format("%-d %b %Y").to_string(),
    }
}

/// How a due date is colored: late, today, soon, later (or done).
fn due_color(ctx: &egui::Context, d: NaiveDate, done: bool) -> Color32 {
    let v = ctx.global_style().visuals.clone();
    let n = (d - today()).num_days();
    if done {
        v.weak_text_color()
    } else if n < 0 {
        status(ctx).danger
    } else if n == 0 {
        v.selection.bg_fill
    } else if n <= 2 {
        status(ctx).warning
    } else {
        v.text_color()
    }
}

/// A small calendar.
fn calendar_glyph(p: &egui::Painter, c: Pos2, s: f32, col: Color32) {
    let st = Stroke::new(1.2, col);
    let r = Rect::from_center_size(c + Vec2::new(0.0, s * 0.06), Vec2::new(s, s * 0.85));
    p.rect_stroke(r, 2.0, st, egui::StrokeKind::Inside);
    p.line_segment([Pos2::new(r.left(), r.top() + s * 0.28), Pos2::new(r.right(), r.top() + s * 0.28)], st);
    for x in [r.left() + s * 0.28, r.right() - s * 0.28] {
        p.line_segment([Pos2::new(x, r.top() - s * 0.14), Pos2::new(x, r.top() + s * 0.12)], st);
    }
}

/// A due date as a pill (calendar and words), its left edge's middle at `left`. Returns its
/// rectangle.
pub(super) fn due_pill(p: &egui::Painter, left: Pos2, d: NaiveDate, done: bool, size: f32) -> Rect {
    let ctx = p.ctx().clone();
    let col = due_color(&ctx, d, done);
    let font = egui::FontId::proportional((size * 0.8).clamp(11.0, 14.0));
    let g = p.layout_no_wrap(due_label(d), font, col);
    let h = g.size().y + 6.0;
    let r = Rect::from_min_size(Pos2::new(left.x, left.y - h / 2.0), Vec2::new(g.size().x + 30.0, h));
    p.rect_filled(r, h / 2.0, col.gamma_multiply(0.14));
    calendar_glyph(p, Pos2::new(r.left() + 13.0, r.center().y), 10.0, col);
    p.galley(Pos2::new(r.left() + 22.0, r.center().y - g.size().y / 2.0), g, col);
    r
}

pub(super) fn priority_color(ctx: &egui::Context, p: Priority, weak: Color32) -> Color32 {
    let s = status(ctx);
    match p {
        Priority::High => s.danger,
        Priority::Medium => s.warning,
        Priority::Low => s.info,
        Priority::None => weak,
    }
}

/// A small flag (a hollow one for no priority).
pub(super) fn flag(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.4, col);
    p.line_segment([c + Vec2::new(-3.5, -6.0), c + Vec2::new(-3.5, 6.5)], s);
    let pts = vec![c + Vec2::new(-3.5, -6.0), c + Vec2::new(5.0, -3.3), c + Vec2::new(-3.5, -0.6)];
    p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
}

fn hollow_flag(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.2, col);
    p.line_segment([c + Vec2::new(-3.5, -6.0), c + Vec2::new(-3.5, 6.5)], s);
    p.add(egui::Shape::closed_line(vec![c + Vec2::new(-3.5, -6.0), c + Vec2::new(5.0, -3.3), c + Vec2::new(-3.5, -0.6)], s));
}

/// A task's box: empty, half (in progress) or ticked.
fn state_box(p: &egui::Painter, r: Rect, it: &Item, v: &egui::Visuals, hot: bool) {
    if it.done {
        p.rect_filled(r, 3.5, v.selection.bg_fill);
        let c = r.center();
        let s = Stroke::new(1.8, v.selection.stroke.color);
        p.line_segment([c + Vec2::new(-4.0, 0.0), c + Vec2::new(-1.2, 3.2)], s);
        p.line_segment([c + Vec2::new(-1.2, 3.2), c + Vec2::new(4.2, -3.2)], s);
    } else if it.doing {
        p.rect_stroke(r, 3.5, Stroke::new(1.5, v.selection.bg_fill), egui::StrokeKind::Inside);
        p.rect_filled(Rect::from_min_max(r.min, Pos2::new(r.center().x, r.max.y)).shrink(3.0), 1.5, v.selection.bg_fill);
    } else {
        p.rect_stroke(r, 3.5, Stroke::new(if hot { 1.8 } else { 1.3 }, if hot { v.selection.bg_fill } else { v.weak_text_color() }), egui::StrokeKind::Inside);
    }
}

/// The page's search box: a rounded field with a magnifier.
fn search_box(ui: &mut egui::Ui, query: &mut String) {
    let v = ui.visuals().clone();
    let id = egui::Id::unique("todo-search");
    let focused = ui.ctx().memory(|m| m.has_focus(id));
    let fill = palette::mix(v.extreme_bg_color, v.text_color(), 0.05);
    let frame = egui::Frame::NONE.fill(fill).corner_radius(r_md()).inner_margin(egui::Margin { left: 28, right: 10, top: 5, bottom: 5 });
    let r = frame
        .show(ui, |ui| {
            ui.add(egui::TextEdit::singleline(query).id(id).frame(egui::Frame::NONE).margin(egui::Margin::ZERO).hint_text("Search todos").font(fonts::ui_font(13.0)).desired_width(170.0));
        })
        .response
        .rect;
    let c = Pos2::new(r.left() + 15.0, r.center().y - 1.0);
    let st = Stroke::new(1.4, v.weak_text_color());
    ui.painter().circle_stroke(c, 4.5, st);
    ui.painter().line_segment([c + Vec2::new(3.3, 3.3), c + Vec2::new(6.5, 6.5)], st);
    if focused {
        ui.painter().rect_stroke(r, r_md(), Stroke::new(1.5, v.selection.bg_fill), egui::StrokeKind::Outside);
    }
}

/// One choice out of a few, as a single rounded control.
fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str)]) {
    let v = ui.visuals().clone();
    let font = fonts::ui_font(13.0);
    let sizes: Vec<Vec2> = options.iter().map(|(_, l)| ui.painter().layout_no_wrap(l.to_string(), font.clone(), v.text_color()).size() + Vec2::new(22.0, 10.0)).collect();
    let total = Vec2::new(sizes.iter().map(|s| s.x).sum::<f32>() + 6.0, sizes.iter().map(|s| s.y).fold(0.0, f32::max) + 6.0);
    let (rect, _) = ui.allocate_exact_size(total, Sense::hover());
    ui.painter().rect_filled(rect, r_md(), palette::mix(v.extreme_bg_color, v.text_color(), 0.06));
    let mut x = rect.left() + 3.0;
    for ((val, label), size) in options.iter().zip(sizes) {
        let r = Rect::from_min_size(Pos2::new(x, rect.top() + 3.0), Vec2::new(size.x, rect.height() - 6.0));
        let resp = ui.interact(r, egui::Id::unique(("seg", *label)), Sense::click());
        let on = *value == *val;
        if on {
            ui.painter().rect_filled(r, r_sm(), v.window_fill);
            ui.painter().rect_stroke(r, r_sm(), Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color), egui::StrokeKind::Inside);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill.gamma_multiply(0.5));
        }
        let fg = if on { v.strong_text_color() } else { v.weak_text_color() };
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, *label, font.clone(), fg);
        if resp.clicked() {
            *value = *val;
        }
        x += size.x;
    }
}

/// A quiet drop-down: the choice's name and a chevron.
fn dropdown<T: PartialEq + Copy>(ui: &mut egui::Ui, id: &str, value: &mut T, options: &[(T, &str)]) {
    let v = ui.visuals().clone();
    let current = options.iter().find(|(o, _)| *o == *value).map(|(_, l)| *l).unwrap_or("");
    let btn = egui::Button::new(egui::RichText::new(format!("{current}     ")).font(fonts::ui_font(13.0)).color(v.text_color())).frame_when_inactive(false).corner_radius(r_sm()).min_size(Vec2::new(0.0, 28.0));
    let (resp, _) = egui::containers::menu::MenuButton::from_button(btn).ui(ui, |ui| {
        ui.set_min_width(150.0);
        for (o, l) in options {
            if ui.add(egui::Button::selectable(*o == *value, *l)).clicked() {
                *value = *o;
                ui.close();
            }
        }
    });
    let c = Pos2::new(resp.rect.right() - 13.0, resp.rect.center().y);
    let st = Stroke::new(1.4, v.weak_text_color());
    ui.painter().line_segment([c + Vec2::new(-3.5, -1.5), c + Vec2::new(0.0, 2.0)], st);
    ui.painter().line_segment([c + Vec2::new(3.5, -1.5), c + Vec2::new(0.0, 2.0)], st);
    let _ = id;
}

/// A section title in the popover.
fn caption(ui: &mut egui::Ui, icon: Caption, text: &str) {
    ui.add_space(4.0);
    let weak = ui.visuals().weak_text_color();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let (r, _) = ui.allocate_exact_size(Vec2::splat(13.0), Sense::hover());
        let p = ui.painter();
        let c = r.center();
        match icon {
            Caption::Progress => mini_state(p, r, 1, weak, weak),
            Caption::Due => calendar_glyph(p, c, 11.0, weak),
            Caption::Priority => flag(p, c + Vec2::new(0.0, 0.5), weak),
            Caption::Tags => {
                p.text(c, egui::Align2::CENTER_CENTER, "#", fonts::ui_font(12.5), weak);
            }
        }
        ui.label(egui::RichText::new(text).font(fonts::ui_font(11.0)).color(weak));
    });
}

#[derive(Clone, Copy)]
enum Caption {
    Progress,
    Due,
    Priority,
    Tags,
}

/// A small box for a progress state (0 to do, 1 in progress, 2 done).
fn mini_state(p: &egui::Painter, r: Rect, k: usize, col: Color32, fill: Color32) {
    let r = r.shrink(0.5);
    match k {
        2 => {
            p.rect_filled(r, 2.5, fill);
            let c = r.center();
            let s = Stroke::new(1.4, Color32::WHITE);
            p.line_segment([c + Vec2::new(-3.0, 0.0), c + Vec2::new(-0.8, 2.4)], s);
            p.line_segment([c + Vec2::new(-0.8, 2.4), c + Vec2::new(3.2, -2.4)], s);
        }
        1 => {
            p.rect_stroke(r, 2.5, Stroke::new(1.2, col), egui::StrokeKind::Inside);
            p.rect_filled(Rect::from_min_max(r.min, Pos2::new(r.center().x, r.max.y)).shrink(2.0), 1.0, col);
        }
        _ => {
            p.rect_stroke(r, 2.5, Stroke::new(1.2, col), egui::StrokeKind::Inside);
        }
    }
}

impl NotesApp {
    /// Shift+Ctrl+T: the line at the cursor becomes a super todo (a task with the first todo
    /// tag), and its popover opens.
    pub(super) fn make_super_todo(&mut self) {
        let tag = self.todo_tags().into_iter().next().unwrap_or_else(|| "task".into());
        let ctx_id = egui::Id::unique("note-body");
        let Some(d) = self.doc.as_mut() else { return };
        let Some(state) = self.caret_state.as_ref().filter(|(id, _)| *id == ctx_id).map(|(_, c)| *c) else { return };
        let caret = d.body.char_indices().nth(state).map(|(b, _)| b).unwrap_or(d.body.len());
        let n = d.body[..caret].matches('\n').count();
        let line = tasks::line_of(&d.body, n).unwrap_or("").to_string();
        let new = match tasks::parse_line(n, &line) {
            Some(mut it) => {
                if !it.tags.iter().any(|t| t.eq_ignore_ascii_case(&tag)) {
                    it.tags.insert(0, tag.clone());
                }
                tasks::write_line(&line, it.mark(), &it.tags, it.due, it.priority)
            }
            None => {
                let indent = &line[..line.len() - line.trim_start().len()];
                let t = line.trim_start();
                let t = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).unwrap_or(t);
                format!("{indent}- [ ] {} #{tag}", t.trim_end()).replacen("[ ]  #", "[ ] #", 1)
            }
        };
        let body = if d.body.is_empty() { Some(new.clone()) } else { tasks::replace_line(&d.body, n, &new) };
        if let Some(b) = body {
            d.body = b;
            d.dirty = true;
        }
        self.task_pop_line = Some(n);
    }

    /// The todo tags without `#`, lower case.
    pub(super) fn todo_tags(&self) -> Vec<String> {
        self.s.todo_tags.iter().map(|t| t.trim().trim_start_matches('#').to_lowercase()).filter(|t| !t.is_empty()).collect()
    }

    /// Buttons beside the super todos in the open note's text: on the cursor's line and the
    /// one under the pointer. Each opens the task's popover.
    pub(super) fn task_buttons(ui: &egui::Ui, text: &str, out: &egui::text_edit::TextEditOutput, triggers: &[String], caret: Option<usize>, look_weak: Color32) -> Vec<(Rect, super::live_edit::Hit)> {
        let mut hits = Vec::new();
        if triggers.is_empty() {
            return hits;
        }
        let hover_line = ui.ctx().pointer_hover_pos().filter(|p| out.response.response.rect.contains(*p)).map(|p| {
            let cc = out.galley.cursor_from_pos(p - out.galley_pos);
            let b = text.char_indices().nth(cc.index.0).map(|(b, _)| b).unwrap_or(text.len());
            text[..b].matches('\n').count()
        });
        let caret_line = caret.map(|b| text[..b.min(text.len())].matches('\n').count());
        let mut start = 0;
        for (i, l) in text.split_inclusive('\n').enumerate() {
            let here = start;
            start += l.len();
            if Some(i) != hover_line && Some(i) != caret_line {
                continue;
            }
            let Some(it) = tasks::parse_line(i, l) else { continue };
            if !it.is_super(triggers) {
                continue;
            }
            let row = out.galley.pos_from_cursor(CCursor::new(text[..here].chars().count())).translate(out.galley_pos.to_vec2());
            let right = out.response.response.rect.right();
            let r = Rect::from_center_size(Pos2::new(right - 16.0, row.center().y), Vec2::new(26.0, 20.0));
            let pointer_on = ui.ctx().pointer_hover_pos().is_some_and(|p| r.contains(p));
            let v = ui.visuals();
            let p = ui.painter();
            p.rect_filled(r, r_sm(), if pointer_on { v.widgets.hovered.weak_bg_fill } else { v.faint_bg_color });
            p.rect_stroke(r, r_sm(), Stroke::new(1.0, look_weak.gamma_multiply(0.35)), egui::StrokeKind::Inside);
            // An arrow: open the task's panel.
            let c = r.center();
            let col = if pointer_on { v.text_color() } else { look_weak };
            let st = Stroke::new(1.5, col);
            p.line_segment([c + Vec2::new(-4.5, 0.0), c + Vec2::new(4.5, 0.0)], st);
            p.line_segment([c + Vec2::new(1.0, -3.5), c + Vec2::new(4.5, 0.0)], st);
            p.line_segment([c + Vec2::new(1.0, 3.5), c + Vec2::new(4.5, 0.0)], st);
            if pointer_on {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            hits.push((r, super::live_edit::Hit::Task(i, r.left_bottom() + Vec2::new(-210.0, 6.0))));
        }
        hits
    }

    /// Change the open note's task on line `n`: done, tags, due date, priority.
    fn edit_task(&mut self, n: usize, f: impl FnOnce(&mut Item)) {
        let Some(d) = self.doc.as_mut() else { return };
        let Some(line) = tasks::line_of(&d.body, n) else { return };
        let Some(mut it) = tasks::parse_line(n, line) else { return };
        f(&mut it);
        let new = tasks::write_line(line, it.mark(), &it.tags, it.due, it.priority);
        if let Some(b) = tasks::replace_line(&d.body, n, &new)
            && b != d.body
        {
            d.body = b;
            d.dirty = true;
            d.edited_at = 0.0;
        }
    }

    /// The popover for the open note's task, while it's up.
    pub(super) fn task_popover(&mut self, ctx: &egui::Context) {
        let Some(mut pop) = self.task_pop.take() else { return };
        let Some(it) = self.doc.as_ref().and_then(|d| tasks::line_of(&d.body, pop.line)).and_then(|l| tasks::parse_line(pop.line, l)) else { return };
        let triggers = self.todo_tags();
        let v = ctx.global_style().visuals.clone();
        let weak = v.weak_text_color();
        let mut change: Option<TaskChange> = None;
        let mut close = ctx.input(|i| i.key_pressed(Key::Escape));
        let mut edit_text = false;
        let screen = ctx.content_rect();
        let w = 288.0;
        // Below the task, or above it when there's no room below (its height from last frame).
        let h = ctx.data(|m| m.get_temp::<f32>(egui::Id::unique("task-popover-h"))).unwrap_or(330.0);
        let y = if pop.at.y + h > screen.bottom() - 8.0 { (pop.at.y - h - 34.0).max(screen.top() + 8.0) } else { pop.at.y };
        let pos = Pos2::new(pop.at.x.clamp(screen.left() + 8.0, (screen.right() - w - 36.0).max(screen.left())), y);
        let r = egui::Area::new(egui::Id::unique("task-popover")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).corner_radius(r_md()).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
                ui.set_width(w);
                ui.spacing_mut().item_spacing.y = 6.0;
                // ---- the task: its box (click to finish) and its words
                ui.horizontal(|ui| {
                    let (br, bresp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
                    state_box(ui.painter(), br, &it, &v, bresp.hovered());
                    if bresp.on_hover_text(if it.done { "Mark not done" } else { "Mark done" }).clicked() {
                        change = Some(Box::new(|i: &mut Item| {
                            i.done = !i.done;
                            i.doing = false;
                        }));
                    }
                    ui.add_space(4.0);
                    let title = if it.title.is_empty() { "Task".to_string() } else { it.title.clone() };
                    let tw = ui.available_width() - 26.0;
                    ui.allocate_ui(Vec2::new(tw, 20.0), |ui| {
                        ui.add(egui::Label::new(egui::RichText::new(title).font(fonts::ui_font(14.5)).strong().color(v.strong_text_color())).truncate());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(egui::RichText::new("×").size(15.0).color(weak)).frame_when_inactive(false).min_size(Vec2::splat(22.0))).on_hover_text("Close (Esc)").clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(2.0);
                // ---- progress
                caption(ui, Caption::Progress, "PROGRESS");
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let cur = if it.done { 2 } else if it.doing { 1 } else { 0 };
                    for (k, label) in ["To do", "In progress", "Done"].into_iter().enumerate() {
                        let resp = ui.add(egui::Button::new(format!("       {label}")).selected(cur == k).corner_radius(r_sm()).min_size(Vec2::new(0.0, 26.0)));
                        let col = if cur == k { v.selection.stroke.color } else { v.text_color() };
                        let fill = if cur == k { v.selection.stroke.color.gamma_multiply(0.85) } else { v.selection.bg_fill };
                        mini_state(ui.painter(), Rect::from_center_size(Pos2::new(resp.rect.left() + 14.0, resp.rect.center().y), Vec2::splat(12.0)), k, col, fill);
                        if resp.clicked() && cur != k {
                            change = Some(Box::new(move |i: &mut Item| {
                                i.done = k == 2;
                                i.doing = k == 1;
                            }));
                        }
                    }
                });
                // ---- due date
                caption(ui, Caption::Due, "DUE");
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                    if let Some(d) = it.due {
                        let (pr, _) = ui.allocate_exact_size(Vec2::new(0.0, 24.0), Sense::hover());
                        let pill = due_pill(ui.painter(), Pos2::new(pr.left(), pr.center().y), d, it.done, 14.0);
                        ui.allocate_exact_size(Vec2::new(pill.width(), 24.0), Sense::hover()).1.on_hover_text(d.format("%A %-d %B %Y").to_string());
                        if ui.add(egui::Button::new(egui::RichText::new("×").color(weak)).frame_when_inactive(false).min_size(Vec2::splat(22.0))).on_hover_text("No due date").clicked() {
                            change = Some(Box::new(|i: &mut Item| i.due = None));
                        }
                        ui.end_row();
                    }
                    let t = today();
                    for (label, d) in [("Today", t), ("Tomorrow", t + chrono::Days::new(1)), ("Next week", t + chrono::Days::new(7))] {
                        if ui.add(egui::Button::new(label).selected(it.due == Some(d)).corner_radius(r_sm())).clicked() {
                            change = Some(Box::new(move |i: &mut Item| i.due = Some(d)));
                        }
                    }
                    let shown = it.due.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default();
                    let btn = egui::Button::new("    Date…").corner_radius(r_sm());
                    let (resp, _) = egui::containers::menu::MenuButton::from_button(btn).ui(ui, |ui| {
                        if let Some(day) = super::props::calendar(ui, &shown) {
                            change = Some(Box::new(move |i: &mut Item| i.due = Some(day)));
                            ui.close();
                        }
                    });
                    calendar_glyph(ui.painter(), Pos2::new(resp.rect.left() + 13.0, resp.rect.center().y), 10.0, v.text_color());
                });
                // ---- priority: flags
                caption(ui, Caption::Priority, "PRIORITY");
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for p in Priority::ALL {
                        let on = it.priority == p;
                        let resp = ui.add(egui::Button::new(format!("     {}", p.label())).selected(on).corner_radius(r_sm()).min_size(Vec2::new(0.0, 26.0)));
                        let c = Pos2::new(resp.rect.left() + 13.0, resp.rect.center().y);
                        if p == Priority::None {
                            hollow_flag(ui.painter(), c, weak);
                        } else {
                            flag(ui.painter(), c, priority_color(ctx, p, weak));
                        }
                        if resp.clicked() && !on {
                            change = Some(Box::new(move |i: &mut Item| i.priority = p));
                        }
                    }
                });
                // ---- tags (the todo tag itself stays out of sight)
                caption(ui, Caption::Tags, "TAGS");
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                    for t in it.tags.iter().filter(|t| !triggers.contains(&t.to_lowercase())) {
                        let b = egui::Button::new(egui::RichText::new(format!("#{t}  ×")).size(12.5)).fill(palette::mix(v.faint_bg_color, v.selection.bg_fill, 0.35)).corner_radius(r_xs());
                        if ui.add(b).on_hover_text("Remove this tag").clicked() {
                            let t = t.clone();
                            change = Some(Box::new(move |i: &mut Item| i.tags.retain(|x| *x != t)));
                        }
                    }
                    let r = ui.add(egui::TextEdit::singleline(&mut pop.tag_input).hint_text("add tag").desired_width(100.0));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        let add: Vec<String> = pop.tag_input.split([',', ' ']).map(|t| t.trim().trim_start_matches('#').to_string()).filter(|t| !t.is_empty()).collect();
                        pop.tag_input.clear();
                        change = Some(Box::new(move |i: &mut Item| {
                            for t in add {
                                if !i.tags.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
                                    i.tags.push(t);
                                }
                            }
                        }));
                        r.request_focus();
                    }
                });
                // ---- footer: edit the words, or make it a plain todo again
                ui.add_space(6.0);
                let (sep, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                ui.painter().hline(sep.x_range(), sep.center().y, Stroke::new(1.0, weak.gamma_multiply(0.25)));
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let down = ui.add(egui::Button::new("     Plain todo").frame_when_inactive(false).corner_radius(r_sm()).min_size(Vec2::new(0.0, 26.0)));
                    // A down arrow.
                    let c = Pos2::new(down.rect.left() + 13.0, down.rect.center().y);
                    let st = Stroke::new(1.4, weak);
                    ui.painter().line_segment([c + Vec2::new(0.0, -5.0), c + Vec2::new(0.0, 5.0)], st);
                    ui.painter().line_segment([c + Vec2::new(-4.0, 1.0), c + Vec2::new(0.0, 5.0)], st);
                    ui.painter().line_segment([c + Vec2::new(4.0, 1.0), c + Vec2::new(0.0, 5.0)], st);
                    if down.on_hover_text("Make it a plain todo again: drops its todo tag, due date and priority").clicked() {
                        let triggers = triggers.clone();
                        change = Some(Box::new(move |i: &mut Item| tasks::downgrade(i, &triggers)));
                        close = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new("Edit text").corner_radius(r_sm()).min_size(Vec2::new(0.0, 26.0))).on_hover_text("Put the cursor at the end of the task's line").clicked() {
                            edit_text = true;
                            close = true;
                        }
                    });
                });
            });
        });
        ctx.data_mut(|m| m.insert_temp(egui::Id::unique("task-popover-h"), r.response.rect.height()));
        // A click outside closes it.
        let outside = ctx.input(|i| i.pointer.any_pressed()) && ctx.pointer_interact_pos().is_some_and(|p| !r.response.rect.contains(p)) && !egui::Popup::is_any_open(ctx);
        if outside && !pop.fresh {
            close = true;
        }
        if let Some(f) = change {
            self.edit_task(pop.line, f);
        }
        if edit_text && let Some(d) = self.doc.as_ref() {
            // The cursor at the end of the task's line, in the text.
            let id = egui::Id::unique("note-body");
            let before: usize = d.body.split_inclusive('\n').take(pop.line).map(|l| l.chars().count()).sum();
            let line_end = before + tasks::line_of(&d.body, pop.line).map(|l| l.trim_end_matches('\r').chars().count()).unwrap_or(0);
            let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(line_end))));
            st.store(ctx, id);
            ctx.memory_mut(|m| m.request_focus(id));
        }
        pop.fresh = false;
        if !close {
            self.task_pop = Some(pop);
        }
    }

    /// Tick or untick a task in any note (the open one is changed in place).
    fn toggle_task(&mut self, path: &Path, it: &Item) {
        if self.doc.as_ref().is_some_and(|d| d.path == path) {
            self.edit_task(it.line, |i| {
                i.done = !i.done;
                i.doing = false;
            });
            self.save_now();
            return;
        }
        // Checks never write files (unless asked to, on a throwaway vault).
        if crate::settings::NO_SAVE.load(std::sync::atomic::Ordering::Relaxed) && std::env::var_os("RIVET_NOTES_WRITE").is_none() {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else { return };
        // Only when the line is still the one shown.
        if tasks::line_of(&text, it.line).map(|l| l.trim_end_matches('\r')) != Some(it.raw.as_str()) {
            self.note_msg = Some(("That task changed; showing it again.".into(), true));
            self.rescan();
            return;
        }
        let new = tasks::write_line(&it.raw, if it.done { ' ' } else { 'x' }, &it.tags, it.due, it.priority);
        if let Some(t) = tasks::replace_line(&text, it.line, &new) {
            let tmp = path.with_extension("md.rivet-tmp");
            if std::fs::write(&tmp, t).and_then(|_| std::fs::rename(&tmp, path)).is_ok()
                && let (Some(v), Some(n)) = (&self.vault, self.notes.iter_mut().find(|n| n.path == path))
                && let Some(fresh) = vault::read_note(v, path)
            {
                *n = fresh;
                self.notes_gen += 1;
            }
        }
    }

    /// The Todos page: every super todo in the vault.
    pub(super) fn todos_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let weak = v.weak_text_color();
        let triggers = self.todo_tags();
        let t0 = today();
        // ---- gather
        let mut all: Vec<(PathBuf, String, Item)> = Vec::new();
        for n in &self.notes {
            for it in n.tasks.iter().filter(|i| i.is_super(&triggers)) {
                all.push((n.path.clone(), n.title.clone(), it.clone()));
            }
        }
        let mut tags: Vec<String> = Vec::new();
        for (_, _, it) in &all {
            for t in &it.tags {
                if !triggers.contains(&t.to_lowercase()) && !tags.iter().any(|x| x.eq_ignore_ascii_case(t)) {
                    tags.push(t.clone());
                }
            }
        }
        tags.sort_by_key(|t| t.to_lowercase());
        // ---- header: title and count, search on the right
        let open_n = all.iter().filter(|(_, _, i)| !i.done).count();
        let tv = &mut self.todo_view;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Todos").size(25.0 * fonts::scale()).strong().color(v.strong_text_color()));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(format!("{open_n} open")).color(weak));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                search_box(ui, &mut tv.query);
            });
        });
        ui.add_space(12.0);
        // ---- status, then when and sort
        ui.horizontal(|ui| {
            segmented(ui, &mut tv.show, &[(Show::Open, "Open"), (Show::Doing, "In progress"), (Show::Done, "Done"), (Show::All, "All")]);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                dropdown(ui, "todo-sort", &mut tv.sort, &[(Sort::Due, "By due date"), (Sort::Priority, "By priority"), (Sort::Note, "By note")]);
                dropdown(ui, "todo-when", &mut tv.when, &[(When::Any, "Any time"), (When::Overdue, "Overdue"), (When::Today, "Due today"), (When::Week, "Next 7 days"), (When::NoDate, "No date")]);
            });
        });
        // ---- tags
        if !tags.is_empty() {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                for t in &tags {
                    let on = tv.tag.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(t));
                    let (fg, fill) = if on { (v.selection.stroke.color, v.selection.bg_fill) } else { (weak, palette::mix(v.extreme_bg_color, v.text_color(), 0.06)) };
                    let b = egui::Button::new(egui::RichText::new(format!("#{t}")).font(fonts::ui_font(12.0)).color(fg)).fill(fill).corner_radius(10.0).min_size(Vec2::new(0.0, 22.0));
                    if ui.add(b).on_hover_text(if on { "Show every tag" } else { "Only this tag" }).clicked() {
                        tv.tag = if on { None } else { Some(t.clone()) };
                    }
                }
            });
        }
        ui.add_space(6.0);
        // ---- filter and sort
        let q = tv.query.trim().to_lowercase();
        let mut rows: Vec<&(PathBuf, String, Item)> = all
            .iter()
            .filter(|(_, _, i)| match tv.show {
                Show::Open => !i.done,
                Show::Doing => i.doing && !i.done,
                Show::Done => i.done,
                Show::All => true,
            })
            .filter(|(_, _, i)| match tv.when {
                When::Any => true,
                When::Overdue => i.due.is_some_and(|d| d < t0) && !i.done,
                When::Today => i.due == Some(t0),
                When::Week => i.due.is_some_and(|d| d >= t0 && d <= t0 + chrono::Days::new(7)),
                When::NoDate => i.due.is_none(),
            })
            .filter(|(_, _, i)| tv.tag.as_ref().is_none_or(|t| i.tags.iter().any(|x| x.eq_ignore_ascii_case(t))))
            .filter(|(_, note, i)| q.is_empty() || i.title.to_lowercase().contains(&q) || note.to_lowercase().contains(&q))
            .collect();
        let far = NaiveDate::MAX;
        match tv.sort {
            Sort::Due => rows.sort_by(|a, b| a.2.done.cmp(&b.2.done).then(a.2.due.unwrap_or(far).cmp(&b.2.due.unwrap_or(far))).then(b.2.priority.cmp(&a.2.priority))),
            Sort::Priority => rows.sort_by(|a, b| a.2.done.cmp(&b.2.done).then(b.2.priority.cmp(&a.2.priority)).then(a.2.due.unwrap_or(far).cmp(&b.2.due.unwrap_or(far)))),
            Sort::Note => rows.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()).then(a.2.line.cmp(&b.2.line))),
        }
        // ---- the list
        let mut toggle: Option<(PathBuf, Item)> = None;
        let mut open: Option<PathBuf> = None;
        if rows.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                let msg = if all.is_empty() { format!("No todos yet. Tag a task with #{} to add it here", triggers.first().cloned().unwrap_or_else(|| "task".into())) } else { "Nothing matches".to_string() };
                ui.label(egui::RichText::new(msg).color(weak));
            });
        }
        // Sections: when it's due, its priority, or its note (following the sort).
        let sort = tv.sort;
        let group = |note: &str, it: &Item| -> String {
            if it.done && sort != Sort::Note {
                return "Done".into();
            }
            match sort {
                Sort::Due => match it.due {
                    Some(d) if d < t0 => "Overdue".into(),
                    Some(d) if d == t0 => "Today".into(),
                    Some(d) if d <= t0 + chrono::Days::new(7) => "Next 7 days".into(),
                    Some(_) => "Later".into(),
                    None => "No date".into(),
                },
                Sort::Priority => match it.priority {
                    Priority::None => "No priority".into(),
                    p => format!("{} priority", p.label()),
                },
                Sort::Note => note.to_string(),
            }
        };
        let hair = palette::mix(v.extreme_bg_color, v.text_color(), 0.07);
        egui::ScrollArea::vertical().id_salt("todos").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut last_group: Option<String> = None;
            for (k, (path, note, it)) in rows.iter().map(|r| (&r.0, &r.1, &r.2)).enumerate() {
                let g = group(note, it);
                if last_group.as_ref() != Some(&g) {
                    let count = rows[k..].iter().take_while(|r| group(&r.1, &r.2) == g).count();
                    ui.add_space(if last_group.is_none() { 8.0 } else { 18.0 });
                    ui.horizontal(|ui| {
                        let col = if g == "Overdue" { status(&ctx).danger } else { weak };
                        ui.label(egui::RichText::new(g.to_uppercase()).font(fonts::ui_font(11.0)).strong().color(col));
                        ui.label(egui::RichText::new(count.to_string()).font(fonts::ui_font(11.0)).color(weak.gamma_multiply(0.7)));
                    });
                    ui.add_space(4.0);
                    last_group = Some(g);
                } else {
                    let (sep, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                    ui.painter().hline(Rect::from_min_max(Pos2::new(sep.left() + 38.0, sep.top()), sep.max).x_range(), sep.center().y, Stroke::new(1.0, hair));
                }
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(rect, r_sm(), v.widgets.hovered.weak_bg_fill.gamma_multiply(0.45));
                }
                let p = ui.painter();
                // Check box.
                let b = Rect::from_center_size(Pos2::new(rect.left() + 16.0, rect.center().y), Vec2::splat(16.0));
                let on_box = ctx.pointer_hover_pos().is_some_and(|q| b.expand(4.0).contains(q));
                state_box(p, b, it, &v, on_box);
                // Right side first (due, progress, flag), so the words know their room.
                let mut rx = rect.right() - 10.0;
                if it.priority != Priority::None {
                    flag(p, Pos2::new(rx - 4.0, rect.center().y), priority_color(&ctx, it.priority, weak));
                    rx -= 20.0;
                }
                if let Some(d) = it.due {
                    let probe = due_pill(&p.with_clip_rect(Rect::NOTHING), Pos2::ZERO, d, it.done, 14.0);
                    due_pill(p, Pos2::new(rx - probe.width(), rect.center().y), d, it.done, 14.0);
                    rx -= probe.width() + 6.0;
                }
                if it.doing && !it.done {
                    let g = p.layout_no_wrap("In progress".into(), fonts::ui_font(11.5), v.selection.bg_fill);
                    let pr = Rect::from_min_size(Pos2::new(rx - g.size().x - 14.0, rect.center().y - g.size().y / 2.0 - 3.0), g.size() + Vec2::new(14.0, 6.0));
                    p.rect_filled(pr, pr.height() / 2.0, v.selection.bg_fill.gamma_multiply(0.15));
                    p.galley(pr.min + Vec2::new(7.0, 3.0), g, v.selection.bg_fill);
                    rx = pr.left() - 6.0;
                }
                // The words, then where it is and its tags.
                let x = rect.left() + 36.0;
                let maxw = (rx - x - 10.0).max(40.0);
                let fg = if it.done { weak } else { v.text_color() };
                let mut job = egui::text::LayoutJob::simple_singleline(if it.title.is_empty() { "(no text)".into() } else { it.title.clone() }, fonts::ui_font(14.0), fg);
                job.wrap = egui::text::TextWrapping::truncate_at_width(maxw);
                if it.done
                    && let Some(s) = job.sections.first_mut()
                {
                    s.format.strikethrough = Stroke::new(1.0, weak);
                }
                let g = p.layout_job(job);
                p.galley(Pos2::new(x, rect.top() + 5.0), g, fg);
                let extra: Vec<String> = it.tags.iter().filter(|t| !triggers.contains(&t.to_lowercase())).map(|t| format!("#{t}")).collect();
                let sub = match (sort == Sort::Note, extra.is_empty()) {
                    (true, true) => String::new(),
                    (true, false) => extra.join(" "),
                    (false, true) => note.clone(),
                    (false, false) => format!("{note}  ·  {}", extra.join(" ")),
                };
                let mut job = egui::text::LayoutJob::simple_singleline(sub, fonts::ui_font(11.5), weak);
                job.wrap = egui::text::TextWrapping::truncate_at_width(maxw);
                let g = p.layout_job(job);
                p.galley(Pos2::new(x, rect.bottom() - 6.0 - g.size().y), g, weak);
                let resp = resp.on_hover_text(format!("Open {note}"));
                if resp.clicked() {
                    if on_box {
                        toggle = Some((path.clone(), it.clone()));
                    } else {
                        open = Some(path.clone());
                    }
                }
            }
        });
        if let Some((p, it)) = toggle {
            self.toggle_task(&p, &it);
        }
        if let Some(p) = open {
            self.open_note(&p);
        }
    }
}
