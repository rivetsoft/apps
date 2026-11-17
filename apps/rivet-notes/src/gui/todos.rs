//! Super todos (Preferences → Modules): a task tagged with a todo tag gets a small button in
//! the editor that opens a popover for its due date, priority and tags, and every super todo in
//! the vault is rolled up on the Todos page, which can be sorted and filtered.

use chrono::{Datelike, NaiveDate};
use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
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

fn due_label(d: NaiveDate) -> String {
    let t = today();
    match (d - t).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        -1 => "Yesterday".into(),
        n if (2..7).contains(&n) => d.format("%A").to_string(),
        _ if d.year() == t.year() => d.format("%-d %b").to_string(),
        _ => d.format("%-d %b %Y").to_string(),
    }
}

fn priority_color(ctx: &egui::Context, p: Priority, weak: Color32) -> Color32 {
    let s = status(ctx);
    match p {
        Priority::High => s.danger,
        Priority::Medium => s.warning,
        Priority::Low => s.info,
        Priority::None => weak,
    }
}

/// A small flag for a priority.
fn flag(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.4, col);
    p.line_segment([c + Vec2::new(-3.5, -5.5), c + Vec2::new(-3.5, 6.0)], s);
    p.add(egui::Shape::convex_polygon(vec![c + Vec2::new(-3.5, -5.5), c + Vec2::new(4.5, -3.0), c + Vec2::new(-3.5, -0.5)], col, Stroke::NONE));
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
                tasks::write_line(&line, it.done, &it.tags, it.due, it.priority)
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
            for dx in [-5.0f32, 0.0, 5.0] {
                p.circle_filled(r.center() + Vec2::new(dx, 0.0), 1.5, if pointer_on { v.text_color() } else { look_weak });
            }
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
        let new = tasks::write_line(line, it.done, &it.tags, it.due, it.priority);
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
        let screen = ctx.content_rect();
        let pos = Pos2::new(pop.at.x.clamp(screen.left() + 8.0, (screen.right() - 300.0).max(screen.left())), pop.at.y);
        let r = egui::Area::new(egui::Id::unique("task-popover")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).corner_radius(r_md()).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                ui.set_width(268.0);
                ui.spacing_mut().item_spacing.y = 6.0;
                let title = if it.title.is_empty() { "Task".to_string() } else { it.title.clone() };
                ui.add(egui::Label::new(egui::RichText::new(title).strong().color(v.strong_text_color())).truncate());
                ui.add_space(2.0);
                // ---- due date
                ui.label(egui::RichText::new("DUE").font(fonts::ui_font(11.0)).color(weak));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let t = today();
                    for (label, d) in [("Today", t), ("Tomorrow", t + chrono::Days::new(1)), ("Next week", t + chrono::Days::new(7))] {
                        if ui.selectable_label(it.due == Some(d), label).clicked() {
                            change = Some(Box::new(move |i: &mut Item| i.due = Some(d)));
                        }
                    }
                    let shown = it.due.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default();
                    let btn = egui::Button::new(if shown.is_empty() { "Pick…".to_string() } else { shown.clone() }).corner_radius(r_sm());
                    egui::containers::menu::MenuButton::from_button(btn).ui(ui, |ui| {
                        if let Some(day) = super::props::calendar(ui, &shown) {
                            change = Some(Box::new(move |i: &mut Item| i.due = Some(day)));
                            ui.close();
                        }
                    });
                    if it.due.is_some() && ui.small_button("×").on_hover_text("No due date").clicked() {
                        change = Some(Box::new(|i: &mut Item| i.due = None));
                    }
                });
                // ---- priority
                ui.add_space(2.0);
                ui.label(egui::RichText::new("PRIORITY").font(fonts::ui_font(11.0)).color(weak));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for p in Priority::ALL {
                        let r = ui.selectable_label(it.priority == p, p.label());
                        if p != Priority::None {
                            flag(ui.painter(), Pos2::new(r.rect.left() - 1.0, r.rect.center().y), Color32::TRANSPARENT);
                        }
                        if r.clicked() {
                            change = Some(Box::new(move |i: &mut Item| i.priority = p));
                        }
                    }
                });
                // ---- tags (the todo tags stay; they make it a super todo)
                ui.add_space(2.0);
                ui.label(egui::RichText::new("TAGS").font(fonts::ui_font(11.0)).color(weak));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                    for t in &it.tags {
                        let keep = triggers.contains(&t.to_lowercase());
                        let label = if keep { format!("#{t}") } else { format!("#{t}  ×") };
                        let b = egui::Button::new(egui::RichText::new(label).size(12.5)).fill(palette::mix(v.faint_bg_color, v.selection.bg_fill, 0.35)).corner_radius(r_xs());
                        let r = ui.add_enabled(!keep, b);
                        if !keep && r.on_hover_text("Remove this tag").clicked() {
                            let t = t.clone();
                            change = Some(Box::new(move |i: &mut Item| i.tags.retain(|x| *x != t)));
                        }
                    }
                    let r = ui.add(egui::TextEdit::singleline(&mut pop.tag_input).hint_text("add tag").desired_width(90.0));
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
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut it.done.clone(), "Done").clicked() {
                        change = Some(Box::new(|i: &mut Item| i.done = !i.done));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                });
            });
        });
        // A click outside closes it.
        let outside = ctx.input(|i| i.pointer.any_pressed()) && ctx.pointer_interact_pos().is_some_and(|p| !r.response.rect.contains(p)) && !egui::Popup::is_any_open(ctx);
        if outside && !pop.fresh {
            close = true;
        }
        if let Some(f) = change {
            self.edit_task(pop.line, f);
        }
        pop.fresh = false;
        if !close {
            self.task_pop = Some(pop);
        }
    }

    /// Tick or untick a task in any note (the open one is changed in place).
    fn toggle_task(&mut self, path: &Path, it: &Item) {
        if self.doc.as_ref().is_some_and(|d| d.path == path) {
            self.edit_task(it.line, |i| i.done = !i.done);
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
        let new = tasks::write_line(&it.raw, !it.done, &it.tags, it.due, it.priority);
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
        // ---- header and controls
        let open_n = all.iter().filter(|(_, _, i)| !i.done).count();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Todos").size(25.0 * fonts::scale()).strong().color(v.strong_text_color()));
            ui.add_space(6.0);
            ui.label(egui::RichText::new(format!("{open_n} open")).color(weak));
        });
        ui.add_space(10.0);
        let tv = &mut self.todo_view;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 6.0);
            ui.add(egui::TextEdit::singleline(&mut tv.query).hint_text("Filter").desired_width(160.0));
            ui.add_space(8.0);
            for (s, l) in [(Show::Open, "Open"), (Show::Done, "Done"), (Show::All, "All")] {
                ui.selectable_value(&mut tv.show, s, l);
            }
            ui.add_space(8.0);
            egui::ComboBox::from_id_salt("todo-when")
                .selected_text(match tv.when {
                    When::Any => "Any time",
                    When::Overdue => "Overdue",
                    When::Today => "Due today",
                    When::Week => "Next 7 days",
                    When::NoDate => "No date",
                })
                .show_ui(ui, |ui| {
                    for (w, l) in [(When::Any, "Any time"), (When::Overdue, "Overdue"), (When::Today, "Due today"), (When::Week, "Next 7 days"), (When::NoDate, "No date")] {
                        ui.selectable_value(&mut tv.when, w, l);
                    }
                });
            egui::ComboBox::from_id_salt("todo-sort")
                .selected_text(match tv.sort {
                    Sort::Due => "Sort: Due date",
                    Sort::Priority => "Sort: Priority",
                    Sort::Note => "Sort: Note",
                })
                .show_ui(ui, |ui| {
                    for (s, l) in [(Sort::Due, "Due date"), (Sort::Priority, "Priority"), (Sort::Note, "Note")] {
                        ui.selectable_value(&mut tv.sort, s, l);
                    }
                });
        });
        if !tags.is_empty() {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                for t in &tags {
                    let on = tv.tag.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(t));
                    if ui.selectable_label(on, format!("#{t}")).clicked() {
                        tv.tag = if on { None } else { Some(t.clone()) };
                    }
                }
            });
        }
        ui.add_space(10.0);
        // ---- filter and sort
        let q = tv.query.trim().to_lowercase();
        let mut rows: Vec<&(PathBuf, String, Item)> = all
            .iter()
            .filter(|(_, _, i)| match tv.show {
                Show::Open => !i.done,
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
        egui::ScrollArea::vertical().id_salt("todos").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (path, note, it) in rows {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(rect, r_sm(), v.widgets.hovered.weak_bg_fill.gamma_multiply(0.6));
                }
                let p = ui.painter();
                // Check box.
                let b = Rect::from_center_size(Pos2::new(rect.left() + 18.0, rect.center().y), Vec2::splat(16.0));
                let on_box = ctx.pointer_hover_pos().is_some_and(|q| b.expand(4.0).contains(q));
                if it.done {
                    p.rect_filled(b, 3.5, v.selection.bg_fill);
                    let c = b.center();
                    let s = Stroke::new(1.8, v.selection.stroke.color);
                    p.line_segment([c + Vec2::new(-4.0, 0.0), c + Vec2::new(-1.2, 3.2)], s);
                    p.line_segment([c + Vec2::new(-1.2, 3.2), c + Vec2::new(4.2, -3.2)], s);
                } else {
                    p.rect_stroke(b, 3.5, Stroke::new(if on_box { 1.8 } else { 1.3 }, if on_box { v.selection.bg_fill } else { weak }), egui::StrokeKind::Inside);
                }
                // Title, then the note and tags.
                let x = rect.left() + 38.0;
                let right_w = 150.0;
                let maxw = (rect.right() - x - right_w).max(40.0);
                let fg = if it.done { weak } else { v.text_color() };
                let mut job = egui::text::LayoutJob::simple_singleline(if it.title.is_empty() { "(no text)".into() } else { it.title.clone() }, fonts::ui_font(14.0), fg);
                job.wrap = egui::text::TextWrapping::truncate_at_width(maxw);
                if it.done
                    && let Some(s) = job.sections.first_mut()
                {
                    s.format.strikethrough = Stroke::new(1.0, weak);
                }
                let g = p.layout_job(job);
                p.galley(Pos2::new(x, rect.top() + 6.0), g, fg);
                let extra: Vec<String> = it.tags.iter().filter(|t| !triggers.contains(&t.to_lowercase())).map(|t| format!("#{t}")).collect();
                let sub = if extra.is_empty() { note.clone() } else { format!("{note}   {}", extra.join(" ")) };
                let mut job = egui::text::LayoutJob::simple_singleline(sub, fonts::ui_font(11.5), weak);
                job.wrap = egui::text::TextWrapping::truncate_at_width(maxw);
                let g = p.layout_job(job);
                p.galley(Pos2::new(x, rect.bottom() - 7.0 - g.size().y), g, weak);
                // Due date and priority on the right.
                let mut rx = rect.right() - 12.0;
                if it.priority != Priority::None {
                    flag(p, Pos2::new(rx - 4.0, rect.center().y), priority_color(&ctx, it.priority, weak));
                    rx -= 22.0;
                }
                if let Some(d) = it.due {
                    let col = if it.done {
                        weak
                    } else if d < t0 {
                        status(&ctx).danger
                    } else if d == t0 {
                        v.selection.bg_fill
                    } else {
                        weak
                    };
                    p.text(Pos2::new(rx, rect.center().y), Align2::RIGHT_CENTER, due_label(d), fonts::ui_font(12.5), col);
                }
                let resp = resp.on_hover_text(format!("{}\nIn {note}{}", it.title, if it.priority != Priority::None { format!(" · {} priority", it.priority.label()) } else { String::new() }));
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
        let _ = palette::mix;
    }
}
