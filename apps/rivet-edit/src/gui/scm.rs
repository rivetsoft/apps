//! Git in the window: the sidebar's Git panel, the gutter's change marks, the box a mark
//! opens on a change, and small drawn glyphs for their buttons. VS Code and Zed lay theirs out
//! much like this.
//!
//! The panel shows, from the top:
//! - the branch, with ahead and behind counts, and the sync and more buttons;
//! - the commit message and the Commit button;
//! - conflicts, staged changes, and changes not yet staged.
//!
//! The state and the background work are in `crate::vcs`, and git itself is the shared
//! `rivet-git` crate.

use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_git::{Change, HunkKind};
use rivet_look::a11y::{Access, Role};
use rivet_look::theme::{paint_tex, r_md, r_sm, selected_tile, status};
use rivet_look::{fonts, palette};

use super::*;
use crate::vcs::Op;

const ROW_H: f32 = 26.0;

/// The box opened on one change in the gutter.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Peek {
    pub path: PathBuf,
    pub hunk: usize,
}

#[derive(Clone, Copy)]
pub(crate) enum Glyph {
    Plus,
    Minus,
    Undo,
    Branch,
    Sync,
    More,
    Up,
    Down,
    Close,
}

/// Draw a small glyph (strokes, so it's sharp at any size and in any theme).
pub(crate) fn glyph(p: &egui::Painter, r: Rect, g: Glyph, col: Color32) {
    let c = r.center();
    let s = r.width().min(r.height()) * 0.5;
    let st = Stroke::new(1.5, col);
    let v = |x: f32, y: f32| c + Vec2::new(x * s, y * s);
    let arc = |cx: f32, cy: f32, rad: f32, from: f32, to: f32| -> Vec<Pos2> {
        (0..=12).map(|i| {
            let a = (from + (to - from) * i as f32 / 12.0).to_radians();
            v(cx + rad * a.cos(), cy - rad * a.sin())
        })
        .collect()
    };
    match g {
        Glyph::Plus => {
            p.line_segment([v(-0.55, 0.0), v(0.55, 0.0)], st);
            p.line_segment([v(0.0, -0.55), v(0.0, 0.55)], st);
        }
        Glyph::Minus => {
            p.line_segment([v(-0.55, 0.0), v(0.55, 0.0)], st);
        }
        Glyph::Undo => {
            // An arrow curling back to the left.
            p.add(egui::Shape::line(arc(0.05, 0.1, 0.48, 200.0, -60.0), st));
            let tip = v(0.05 - 0.48 * 200f32.to_radians().cos().abs(), 0.1 + 0.48 * 200f32.to_radians().sin().abs());
            p.line_segment([tip, tip + Vec2::new(0.0, -0.42 * s)], st);
            p.line_segment([tip, tip + Vec2::new(0.42 * s, 0.05 * s)], st);
        }
        Glyph::Sync => {
            // Two arrows round a circle.
            p.add(egui::Shape::line(arc(0.0, 0.0, 0.5, 30.0, 160.0), st));
            p.add(egui::Shape::line(arc(0.0, 0.0, 0.5, 210.0, 340.0), st));
            let a = v(0.5 * 160f32.to_radians().cos(), -0.5 * 160f32.to_radians().sin());
            p.line_segment([a, a + Vec2::new(0.0, -0.3 * s)], st);
            p.line_segment([a, a + Vec2::new(0.3 * s, 0.05 * s)], st);
            let b = v(0.5 * 340f32.to_radians().cos(), -0.5 * 340f32.to_radians().sin());
            p.line_segment([b, b + Vec2::new(0.0, 0.3 * s)], st);
            p.line_segment([b, b + Vec2::new(-0.3 * s, -0.05 * s)], st);
        }
        Glyph::Branch => {
            let (a, b, d) = (v(-0.35, -0.55), v(-0.35, 0.55), v(0.4, -0.3));
            p.line_segment([a + Vec2::new(0.0, 0.16 * s), b - Vec2::new(0.0, 0.16 * s)], st);
            let bend: Vec<Pos2> = (0..=8)
                .map(|i| {
                    let t = i as f32 / 8.0;
                    let from = d + Vec2::new(0.0, 0.16 * s);
                    let to = v(-0.35, 0.25);
                    let ctrl = Pos2::new(from.x, to.y);
                    let q = |a: Pos2, b: Pos2| a + (b - a) * t;
                    q(q(from, ctrl), q(ctrl, to))
                })
                .collect();
            p.add(egui::Shape::line(bend, st));
            for x in [a, b, d] {
                p.circle_stroke(x, 0.16 * s, st);
            }
        }
        Glyph::More => {
            for x in [-0.5, 0.0, 0.5] {
                p.circle_filled(v(x, 0.0), 1.6, col);
            }
        }
        Glyph::Up => {
            p.add(egui::Shape::line(vec![v(-0.45, 0.2), v(0.0, -0.25), v(0.45, 0.2)], st));
        }
        Glyph::Down => {
            p.add(egui::Shape::line(vec![v(-0.45, -0.2), v(0.0, 0.25), v(0.45, -0.2)], st));
        }
        Glyph::Close => {
            p.line_segment([v(-0.4, -0.4), v(0.4, 0.4)], st);
            p.line_segment([v(-0.4, 0.4), v(0.4, -0.4)], st);
        }
    }
}

/// A small square button with a glyph (a light box under the pointer).
pub(crate) fn glyph_button(ui: &mut egui::Ui, g: Glyph, tip: &str, on: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), if on { Sense::click() } else { Sense::hover() });
    let v = ui.visuals().clone();
    if resp.hovered() && on {
        ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
    }
    let col = if !on { v.weak_text_color().gamma_multiply(0.5) } else if resp.hovered() { v.text_color() } else { v.weak_text_color() };
    glyph(ui.painter(), r.shrink(5.0), g, col);
    resp.role(Role::Button, tip).on_hover_text(tip)
}

/// The color for a kind of change: new is green, changed amber, gone or conflicted red.
pub(crate) fn change_color(ctx: &egui::Context, c: Change) -> Color32 {
    let st = status(ctx);
    match c {
        Change::Added | Change::Untracked | Change::Copied => st.success,
        Change::Deleted | Change::Conflicted => st.danger,
        Change::Renamed => st.info,
        Change::Modified | Change::TypeChanged => st.warning,
        Change::Unchanged => ctx.global_style().visuals.weak_text_color(),
    }
}

/// Which list a file is in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum List {
    Conflicts,
    Staged,
    Changes,
}

impl List {
    fn title(self) -> &'static str {
        match self {
            List::Conflicts => "MERGE CHANGES",
            List::Staged => "STAGED CHANGES",
            List::Changes => "CHANGES",
        }
    }
}

/// What a click in the panel asked for.
enum Ask {
    Op(Op),
    Open(String, bool),
    Discard(Vec<String>, Vec<String>),
    Branches,
    Commit,
    Amend(bool),
    Fold(&'static str),
    Copy(String),
}

/// At most this many files are listed in each section (a huge change stays usable).
const MAX_ROWS: usize = 400;

impl EditApp {
    /// The Git panel, in the sidebar.
    pub(crate) fn git_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let weak = v.weak_text_color();
        let small = |s: &str| egui::RichText::new(s.to_string()).color(weak);
        ui.add_space(6.0);
        if self.git.looking {
            ui.label(small("Looking for a repository…"));
            return;
        }
        let Some(repo) = self.git.repo.clone() else {
            ui.label(small("This folder isn't a Git repository yet."));
            ui.add_space(8.0);
            let busy = self.git.running.is_some();
            let b = egui::Button::new(egui::RichText::new("Initialize Repository").color(v.selection.stroke.color)).fill(v.selection.bg_fill).corner_radius(r_sm()).min_size(Vec2::new(ui.available_width(), 28.0));
            if ui.add_enabled(!busy && self.root.is_some(), b).on_hover_text("Runs git init in this folder").clicked()
                && let Some(r) = self.root.clone()
            {
                self.git.run(Op::Init(r));
            }
            return;
        };
        let Some(st) = self.git.status.clone() else {
            ui.label(egui::RichText::new(self.git.error.clone().unwrap_or_else(|| "Reading the status…".into())).color(if self.git.error.is_some() { status(&ctx).danger } else { weak }));
            return;
        };
        let busy = self.git.running.is_some();
        let mut ask: Option<Ask> = None;

        // The branch, how far it is from upstream, sync and more.
        ui.horizontal(|ui| {
            ui.set_min_height(26.0);
            ui.spacing_mut().item_spacing.x = 4.0;
            let label = st.branch.label();
            let ab = match (st.branch.upstream.is_some(), st.branch.ahead, st.branch.behind) {
                (false, ..) => String::new(),
                (true, 0, 0) => String::new(),
                (true, a, b) => format!("  {}{}", if b > 0 { format!("↓{b} ") } else { String::new() }, if a > 0 { format!("↑{a}") } else { String::new() }),
            };
            let tip = match &st.branch.upstream {
                Some(u) => format!("On {label}, tracking {u}. Click to switch branch."),
                None => format!("On {label}, not pushed to a remote yet. Click to switch branch."),
            };
            let (r, resp) = ui.allocate_exact_size(Vec2::new((ui.available_width() - 52.0).max(60.0), 24.0), Sense::click());
            let resp = resp.role(Role::Button, format!("Git branch {label}")).described(tip.as_str());
            if resp.hovered() {
                ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
            }
            glyph(ui.painter(), Rect::from_center_size(Pos2::new(r.left() + 12.0, r.center().y), Vec2::splat(13.0)), Glyph::Branch, v.text_color());
            let mut job = egui::text::LayoutJob::default();
            job.append(&label, 0.0, egui::TextFormat { font_id: fonts::ui_font(13.0), color: v.strong_text_color(), ..Default::default() });
            job.append(&ab, 0.0, egui::TextFormat { font_id: fonts::ui_font(12.0), color: weak, ..Default::default() });
            job.wrap = egui::text::TextWrapping::truncate_at_width(r.width() - 30.0);
            let g = ui.painter().layout_job(job);
            ui.painter().galley(Pos2::new(r.left() + 24.0, r.center().y - g.size().y / 2.0), g, v.text_color());
            if resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                ask = Some(Ask::Branches);
            }
            let sync_tip = if self.git.syncing() {
                "Working…".to_string()
            } else if st.branch.upstream.is_none() {
                "Publish: push this branch to the remote".into()
            } else {
                format!("Sync: pull{}, then push{}", if st.branch.behind > 0 { format!(" {}", st.branch.behind) } else { String::new() }, if st.branch.ahead > 0 { format!(" {}", st.branch.ahead) } else { String::new() })
            };
            if glyph_button(ui, Glyph::Sync, &sync_tip, !busy && st.branch.head.is_some()).clicked() {
                ask = Some(Ask::Op(Op::Sync));
            }
            let more = glyph_button(ui, Glyph::More, "More Git commands", true);
            egui::Popup::menu(&more).show(|ui| {
                super::draw::menu_style(ui);
                let staged = st.staged().count() > 0;
                let unstaged = st.unstaged().count() > 0;
                let has_head = st.branch.oid.is_some();
                if super::draw::item(ui, "Fetch", "", !busy) {
                    ask = Some(Ask::Op(Op::Fetch));
                }
                if super::draw::item(ui, "Pull", "", !busy && st.branch.upstream.is_some()) {
                    ask = Some(Ask::Op(Op::Pull));
                }
                if super::draw::item(ui, if st.branch.upstream.is_some() { "Push" } else { "Publish Branch" }, "", !busy && st.branch.head.is_some()) {
                    ask = Some(Ask::Op(Op::Push));
                }
                ui.separator();
                if super::draw::item(ui, "Switch Branch…", "", !busy) {
                    ask = Some(Ask::Branches);
                }
                ui.separator();
                if super::draw::item(ui, "Stage All Changes", "", !busy && unstaged) {
                    ask = Some(Ask::Op(Op::StageAll));
                }
                if super::draw::item(ui, "Unstage All", "", !busy && staged) {
                    ask = Some(Ask::Op(Op::UnstageAll));
                }
                if super::draw::item(ui, "Discard All Changes…", "", !busy && unstaged) {
                    let (t, u) = split(st.unstaged());
                    ask = Some(Ask::Discard(t, u));
                }
                ui.separator();
                let amend = self.git.amend;
                if super::draw::item(ui, if amend { "✓ Amend Last Commit" } else { "Amend Last Commit" }, "", !busy && has_head) {
                    ask = Some(Ask::Amend(!amend));
                }
                if super::draw::item(ui, "Undo Last Commit", "", !busy && has_head) {
                    ask = Some(Ask::Op(Op::UndoCommit));
                }
            });
        });

        // The message, and Commit.
        ui.add_space(4.0);
        let mid = egui::Id::unique("git-message");
        if ui.memory(|m| m.has_focus(mid)) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Enter)) {
            ask = Some(Ask::Commit);
        }
        let focused = ui.memory(|m| m.has_focus(mid));
        let hint = if self.git.amend { "Message (leave empty to keep the last one)".to_string() } else { format!("Message (Ctrl+Enter to commit on {})", st.branch.label()) };
        egui::Frame::NONE
            .fill(palette::mix(v.extreme_bg_color, v.text_color(), 0.05))
            .stroke(if focused { Stroke::new(1.3, v.selection.bg_fill) } else { Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color) })
            .corner_radius(r_sm())
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.git.message).id(mid).frame(egui::Frame::NONE).margin(egui::Margin::ZERO).font(fonts::ui_font(13.0)).hint_text(hint.clone()).desired_rows(2).desired_width(f32::INFINITY)).accessible_name(hint);
            });
        ui.add_space(6.0);
        let staged_n = st.staged().count();
        let unstaged_n = st.unstaged().count();
        let conflicts = st.conflicted().count();
        let (label, can) = if self.git.amend {
            ("Amend Last Commit", true)
        } else if staged_n > 0 {
            ("Commit", true)
        } else if unstaged_n > 0 {
            ("Commit All", true)
        } else {
            ("Commit", false)
        };
        let can = can && !busy && conflicts == 0 && (self.git.amend || !self.git.message.trim().is_empty());
        let tip = if conflicts > 0 {
            "Resolve the merge conflicts first"
        } else if staged_n == 0 && unstaged_n > 0 && !self.git.amend {
            "Nothing is staged, so this stages every change, then commits"
        } else {
            "Commits what's staged (Ctrl+Enter in the message)"
        };
        let fg = if can { v.selection.stroke.color } else { weak };
        let b = egui::Button::new(egui::RichText::new(label).color(fg).strong()).fill(if can { v.selection.bg_fill } else { palette::mix(v.panel_fill, v.text_color(), 0.06) }).corner_radius(r_sm()).min_size(Vec2::new(ui.available_width(), 28.0));
        if ui.add_enabled(can, b).on_hover_text(tip).on_disabled_hover_text(tip).clicked() {
            ask = Some(Ask::Commit);
        }
        if let Some(op) = &self.git.running {
            ui.add_space(2.0);
            ui.label(small(op.doing()));
        }
        ui.add_space(6.0);

        // The files: conflicts, staged, then changes.
        let mut rows: Vec<(List, rivet_git::Entry)> = Vec::new();
        let mut counts = [0usize; 3];
        for (list, it) in [(List::Conflicts, st.conflicted().collect::<Vec<_>>()), (List::Staged, st.staged().collect()), (List::Changes, st.unstaged().collect())] {
            counts[list as usize] = it.len();
            if !self.git.folded.contains(list.title()) {
                rows.extend(it.into_iter().take(MAX_ROWS).map(|e| (list, e.clone())));
            }
        }
        // Icons for the names shown.
        let mut icons: HashMap<String, Option<egui::TextureHandle>> = HashMap::new();
        for (_, e) in &rows {
            let name = e.path.rsplit('/').next().unwrap_or(&e.path).to_string();
            if let std::collections::hash_map::Entry::Vacant(slot) = icons.entry(name) {
                let t = self.file_icon(&ctx, slot.key(), 16.0);
                slot.insert(t);
            }
        }
        let open_rel = self.tabs.get(self.active).and_then(|t| t.path.as_ref()).and_then(|p| repo.rel(p));
        if counts.iter().all(|n| *n == 0) {
            ui.label(small("No changes."));
        }
        egui::ScrollArea::vertical().id_salt("git-files").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for list in [List::Conflicts, List::Staged, List::Changes] {
                let n = counts[list as usize];
                if n == 0 {
                    continue;
                }
                let folded = self.git.folded.contains(list.title());
                // The section's title, with buttons for all its files on hover.
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
                let resp = resp.role(Role::Button, format!("{}, {n}", list.title())).expanded(!folded);
                let hover = ui.rect_contains_pointer(rect);
                let c = Pos2::new(rect.left() + 8.0, rect.center().y);
                let pts = if !folded { vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)] } else { vec![c + Vec2::new(-2.0, -4.0), c + Vec2::new(-2.0, 4.0), c + Vec2::new(3.0, 0.0)] };
                ui.painter().add(egui::Shape::convex_polygon(pts, weak, Stroke::NONE));
                ui.painter().text(Pos2::new(rect.left() + 20.0, rect.center().y), Align2::LEFT_CENTER, list.title(), fonts::ui_font(11.0), weak);
                // The count, in a little pill.
                let g = ui.painter().layout_no_wrap(n.to_string(), fonts::ui_font(10.5), v.text_color());
                let pill = Rect::from_center_size(Pos2::new(rect.right() - 6.0 - (g.size().x + 10.0) / 2.0, rect.center().y), Vec2::new(g.size().x + 10.0, 16.0));
                ui.painter().rect_filled(pill, 8.0, palette::mix(v.panel_fill, v.text_color(), 0.12));
                ui.painter().galley(pill.center() - g.size() / 2.0, g, v.text_color());
                if resp.clicked() {
                    ask = Some(Ask::Fold(list.title()));
                }
                if hover && !busy {
                    let mut x = pill.left() - 4.0;
                    let mut btn = |ui: &mut egui::Ui, g: Glyph, tip: &str, salt: &str| {
                        x -= 22.0;
                        let r = Rect::from_min_size(Pos2::new(x, rect.center().y - 11.0), Vec2::splat(22.0));
                        let b = ui.interact(r, ui.scope_id().with(("git-sec", list.title(), salt)), Sense::click()).role(Role::Button, tip);
                        if b.hovered() {
                            ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
                        }
                        glyph(ui.painter(), r.shrink(5.0), g, if b.hovered() { v.text_color() } else { weak });
                        b.on_hover_text(tip).clicked()
                    };
                    match list {
                        List::Staged => {
                            if btn(ui, Glyph::Minus, "Unstage all", "u") {
                                ask = Some(Ask::Op(Op::UnstageAll));
                            }
                        }
                        List::Changes => {
                            if btn(ui, Glyph::Plus, "Stage all changes", "s") {
                                ask = Some(Ask::Op(Op::StageAll));
                            }
                            if btn(ui, Glyph::Undo, "Discard all changes…", "d") {
                                let (t, u) = split(st.unstaged());
                                ask = Some(Ask::Discard(t, u));
                            }
                        }
                        List::Conflicts => {
                            if btn(ui, Glyph::Plus, "Mark all resolved (stages them)", "s") {
                                ask = Some(Ask::Op(Op::Stage(st.conflicted().map(|e| e.path.clone()).collect())));
                            }
                        }
                    }
                }
                if folded {
                    continue;
                }
                for (_, e) in rows.iter().filter(|(l, _)| *l == list) {
                    let change = match list {
                        List::Conflicts => Change::Conflicted,
                        List::Staged => e.index,
                        List::Changes => e.worktree,
                    };
                    let (dir, name) = e.path.rsplit_once('/').unwrap_or(("", &e.path));
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
                    let on = open_rel.as_deref() == Some(e.path.as_str());
                    let resp = resp.role(Role::ListBoxOption, format!("{} {}", e.path, change.words())).selected(on);
                    let hover = ui.rect_contains_pointer(rect);
                    if on {
                        let (fill, stroke) = selected_tile(&v, v.panel_fill);
                        ui.painter().rect_filled(rect, r_sm(), fill);
                        ui.painter().rect_stroke(rect, r_sm(), stroke, egui::StrokeKind::Inside);
                    } else if hover {
                        ui.painter().rect_filled(rect, r_sm(), v.widgets.hovered.weak_bg_fill);
                    }
                    let x = rect.left() + 20.0;
                    if let Some(Some(t)) = icons.get(name) {
                        paint_tex(ui.painter(), t, Rect::from_center_size(Pos2::new(x + 8.0, rect.center().y), Vec2::splat(16.0)), Color32::WHITE);
                    }
                    let col = change_color(&ctx, change);
                    // The letter at the right; buttons before it on hover.
                    let letter_x = rect.right() - 12.0;
                    ui.painter().text(Pos2::new(letter_x, rect.center().y), Align2::CENTER_CENTER, change.letter(), fonts::ui_font(12.0), col);
                    let buttons_w = if hover { 70.0 } else { 0.0 };
                    let text_clip = Rect::from_min_max(rect.min, Pos2::new(letter_x - 10.0 - buttons_w, rect.max.y));
                    let mut job = egui::text::LayoutJob::default();
                    let name_col = if change == Change::Deleted { weak } else { palette::mix(v.text_color(), col, 0.35) };
                    job.append(name, 0.0, egui::TextFormat { font_id: fonts::ui_font(13.0), color: name_col, strikethrough: if change == Change::Deleted { Stroke::new(1.0, weak) } else { Stroke::NONE }, ..Default::default() });
                    if !dir.is_empty() {
                        job.append(dir, 8.0, egui::TextFormat { font_id: fonts::ui_font(11.5), color: weak, ..Default::default() });
                    }
                    let g = ui.painter().layout_job(job);
                    ui.painter().with_clip_rect(text_clip).galley(Pos2::new(x + 22.0, rect.center().y - g.size().y / 2.0), g, v.text_color());
                    let words = match &e.from {
                        Some(f) => format!("{}\n{} (from {f})", e.path, change.words()),
                        None => format!("{}\n{}", e.path, change.words()),
                    };
                    if hover && !busy {
                        let mut bx = letter_x - 12.0;
                        let mut btn = |ui: &mut egui::Ui, g: Glyph, tip: &str, salt: &str| {
                            bx -= 22.0;
                            let r = Rect::from_min_size(Pos2::new(bx, rect.center().y - 11.0), Vec2::splat(22.0));
                            let b = ui.interact(r, ui.scope_id().with(("git-row", &e.path, list.title(), salt)), Sense::click()).role(Role::Button, tip);
                            if b.hovered() {
                                ui.painter().rect_filled(r, r_sm(), palette::mix(v.widgets.hovered.weak_bg_fill, v.text_color(), 0.08));
                            }
                            glyph(ui.painter(), r.shrink(5.0), g, if b.hovered() { v.text_color() } else { weak });
                            b.on_hover_text(tip).clicked()
                        };
                        match list {
                            List::Staged => {
                                if btn(ui, Glyph::Minus, "Unstage", "u") {
                                    ask = Some(Ask::Op(Op::Unstage(vec![e.path.clone()])));
                                }
                            }
                            List::Changes | List::Conflicts => {
                                if btn(ui, Glyph::Plus, if list == List::Conflicts { "Mark resolved (stages it)" } else { "Stage" }, "s") {
                                    ask = Some(Ask::Op(Op::Stage(vec![e.path.clone()])));
                                }
                                if list == List::Changes && btn(ui, Glyph::Undo, if e.untracked() { "Delete this untracked file…" } else { "Discard changes…" }, "d") {
                                    let (t, u) = split(std::iter::once(e));
                                    ask = Some(Ask::Discard(t, u));
                                }
                            }
                        }
                    }
                    let resp = resp.on_hover_text(words);
                    resp.context_menu(|ui| {
                        super::draw::menu_style(ui);
                        if super::draw::item(ui, "Open File", "", change != Change::Deleted) {
                            ask = Some(Ask::Open(e.path.clone(), true));
                        }
                        ui.separator();
                        match list {
                            List::Staged => {
                                if super::draw::item(ui, "Unstage", "", !busy) {
                                    ask = Some(Ask::Op(Op::Unstage(vec![e.path.clone()])));
                                }
                            }
                            _ => {
                                if super::draw::item(ui, "Stage", "", !busy) {
                                    ask = Some(Ask::Op(Op::Stage(vec![e.path.clone()])));
                                }
                                if list == List::Changes && super::draw::item(ui, if e.untracked() { "Delete File…" } else { "Discard Changes…" }, "", !busy) {
                                    let (t, u) = split(std::iter::once(e));
                                    ask = Some(Ask::Discard(t, u));
                                }
                            }
                        }
                        ui.separator();
                        if super::draw::item(ui, "Copy Path", "", true) {
                            ask = Some(Ask::Copy(repo.root.join(&e.path).display().to_string()));
                        }
                    });
                    if resp.double_clicked() || (resp.clicked() && ui.input(|i| i.modifiers.command)) {
                        ask = Some(Ask::Open(e.path.clone(), true));
                    } else if resp.clicked() {
                        ask = Some(Ask::Open(e.path.clone(), false));
                    }
                }
                let more = n.saturating_sub(MAX_ROWS);
                if more > 0 {
                    ui.label(small(&format!("and {more} more")));
                }
                ui.add_space(6.0);
            }
        });

        match ask {
            Some(Ask::Op(op)) => {
                self.git.run(op);
            }
            Some(Ask::Open(rel, keep)) => {
                let p = repo.root.join(&rel);
                if !p.exists() {
                    self.note = Some((format!("{rel} is deleted"), false));
                } else if keep {
                    self.open_file(&p);
                } else {
                    self.preview_file(&p);
                }
            }
            Some(Ask::Discard(t, u)) => self.prompt = Some(Prompt::Discard { tracked: t, untracked: u }),
            Some(Ask::Branches) => self.open_branches(),
            Some(Ask::Commit) => self.commit(),
            Some(Ask::Amend(on)) => {
                self.git.amend = on;
                if on && self.git.message.trim().is_empty() {
                    self.git.message = repo.last_message().unwrap_or_default();
                } else if !on && repo.last_message().as_deref() == Some(self.git.message.as_str()) {
                    self.git.message.clear();
                }
            }
            Some(Ask::Fold(t)) => {
                if !self.git.folded.remove(t) {
                    self.git.folded.insert(t);
                }
            }
            Some(Ask::Copy(s)) => ctx.copy_text(s),
            None => {}
        }
    }

    /// Commit what's staged (everything, when nothing is), with the panel's message.
    pub(crate) fn commit(&mut self) {
        let Some(st) = &self.git.status else { return };
        if st.conflicted().count() > 0 {
            self.note = Some(("Resolve the merge conflicts first".into(), true));
            return;
        }
        let amend = self.git.amend;
        let all = st.staged().count() == 0;
        if !amend && all && st.unstaged().count() == 0 {
            self.note = Some(("There's nothing to commit".into(), false));
            return;
        }
        if !amend && self.git.message.trim().is_empty() {
            self.side = editing::Side::Git;
            self.s.show_sidebar = true;
            self.note = Some(("Write a message for the commit first".into(), true));
            return;
        }
        let message = self.git.message.trim_end().to_string();
        self.git.run(Op::Commit { message, amend, all: all && !amend });
    }

    /// The branch list (Switch Branch).
    pub(crate) fn open_branches(&mut self) {
        if self.git.repo.is_none() {
            self.note = Some(("This folder isn't a Git repository".into(), true));
            return;
        }
        self.git.branches = None;
        self.git.load_branches();
        self.open_quick(quick::QuickKind::Branches);
    }

    /// Stage (or unstage) the open file as it is on disk.
    pub(crate) fn stage_open(&mut self, stage: bool) {
        let Some(rel) = self.tabs.get(self.active).and_then(|t| t.path.as_ref()).and_then(|p| self.git.rel(p)) else {
            self.note = Some(("This file isn't in the folder's Git repository".into(), true));
            return;
        };
        if self.tabs.get(self.active).is_some_and(Tab::dirty) {
            self.note = Some(("Staged the saved file. Unsaved edits aren't included.".into(), false));
        }
        self.git.run(if stage { Op::Stage(vec![rel]) } else { Op::Unstage(vec![rel]) });
    }

    /// Show the Git panel.
    pub(crate) fn show_git(&mut self) {
        self.s.show_sidebar = true;
        self.side = editing::Side::Git;
    }

    /// The open file's changes against its staged copy (for the gutter), brought up to date.
    pub(crate) fn update_gutter(&mut self) {
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let base = t.path.as_ref().and_then(|p| self.git.rel(p)).and_then(|r| self.git.bases.get(&r).cloned().flatten());
        t.gutter.update(&t.text, base.as_ref());
    }

    /// Put the cursor on the next (or previous) change in the open file.
    pub(crate) fn step_change(&mut self, back: bool) {
        self.update_gutter();
        let Some(t) = self.tabs.get(self.active) else { return };
        let hunks = &t.gutter.hunks;
        if hunks.is_empty() {
            self.note = Some(("No changes in this file".into(), false));
            return;
        }
        let cur = t.line_col.0.saturating_sub(1);
        let starts: Vec<usize> = hunks.iter().map(|h| h.new.start).collect();
        let pick = if back { starts.iter().rev().find(|s| **s < cur).or(starts.last()) } else { starts.iter().find(|s| **s > cur).or(starts.first()) };
        if let Some(l) = pick {
            self.goto = Some(l + 1);
        }
    }

    /// The gutter's change marks for the open file: a bar beside new and changed lines, a
    /// wedge where lines were deleted. Returns each change's place on screen (for the box).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paint_changes(ui: &egui::Ui, galley: &egui::Galley, pos: Pos2, clip: Rect, x: f32, hunks: &[rivet_git::Hunk], peek: Option<usize>) -> Vec<(usize, Rect)> {
        let ctx = ui.ctx();
        let st = status(ctx);
        let p = ui.painter();
        let mut out: Vec<(usize, Rect)> = Vec::new();
        if hunks.is_empty() {
            return out;
        }
        // Each line's top and bottom, also when it wraps over several rows.
        let mut line = 0usize;
        let mut top: Option<f32> = None;
        let mut h = 0usize;
        let place = |k: usize, r: Rect, out: &mut Vec<(usize, Rect)>| match out.last_mut() {
            Some((j, rr)) if *j == k => *rr = rr.union(r),
            _ => out.push((k, r)),
        };
        let rows = &galley.rows;
        for (i, row) in rows.iter().enumerate() {
            let rt = pos.y + row.pos.y;
            let line_top = *top.get_or_insert(rt);
            let last = i + 1 == rows.len();
            if !(row.ends_with_newline || last) {
                continue;
            }
            let bottom = rt + row.rect().height();
            top = None;
            if line_top > clip.bottom() {
                break;
            }
            // Skip the changes wholly above this line.
            while h < hunks.len() && if hunks[h].new.is_empty() { hunks[h].new.start < line } else { hunks[h].new.end <= line } {
                h += 1;
            }
            let visible = bottom >= clip.top() - 40.0;
            let mut k = h;
            while k < hunks.len() && hunks[k].new.start <= line {
                let hk = &hunks[k];
                if hk.new.is_empty() {
                    if hk.new.start == line && visible {
                        place(k, Rect::from_min_max(Pos2::new(x - 1.0, line_top - 4.0), Pos2::new(x + 6.0, line_top + 4.0)), &mut out);
                    }
                } else if hk.new.contains(&line) && visible {
                    place(k, Rect::from_min_max(Pos2::new(x, line_top), Pos2::new(x + 3.0, bottom)), &mut out);
                }
                k += 1;
            }
            // A deletion at the very end: below the last line.
            if last {
                for (k, hk) in hunks.iter().enumerate() {
                    if hk.new.is_empty() && hk.new.start > line {
                        place(k, Rect::from_min_max(Pos2::new(x - 1.0, bottom - 4.0), Pos2::new(x + 6.0, bottom + 4.0)), &mut out);
                    }
                }
            }
            line += 1;
        }
        for (k, r) in &out {
            let hk = &hunks[*k];
            let col = match hk.kind() {
                HunkKind::Added => st.success,
                HunkKind::Modified => st.info,
                HunkKind::Deleted => st.danger,
            };
            let col = if peek == Some(*k) { col } else { col.gamma_multiply(0.85) };
            if hk.kind() == HunkKind::Deleted {
                let c = Pos2::new(r.left(), r.center().y);
                p.add(egui::Shape::convex_polygon(vec![c + Vec2::new(-1.0, -5.0), c + Vec2::new(6.0, 0.0), c + Vec2::new(-1.0, 5.0)], col, Stroke::NONE));
            } else {
                p.rect_filled(*r, 1.0, col);
            }
        }
        out
    }

    /// The box on one change: the lines as they were, with Stage, Revert, and the next and
    /// previous change. `at` is the change's place on screen; `text_left` where the code starts.
    pub(crate) fn peek_box(&mut self, ctx: &egui::Context, at: Rect, text_left: f32, width: f32, font: &egui::FontId) {
        let Some(peek) = self.peek.clone() else { return };
        let Some(t) = self.tabs.get(self.active) else { return };
        if t.path.as_ref() != Some(&peek.path) {
            self.peek = None;
            return;
        }
        let Some(h) = t.gutter.hunks.get(peek.hunk).cloned() else {
            self.peek = None;
            return;
        };
        let Some(base) = t.gutter.base().cloned() else { return };
        let total = t.gutter.hunks.len();
        let old: Vec<&str> = rivet_git::diff::lines(&base)[h.old.clone()].to_vec();
        let st = status(ctx);
        let v = ctx.global_style().visuals.clone();
        let rel = t.path.as_ref().and_then(|p| self.git.rel(p));
        let mut act: Option<&str> = None;
        let area = egui::Area::new(egui::Id::unique("git-peek")).order(egui::Order::Foreground).fixed_pos(Pos2::new(text_left, at.bottom() + 2.0));
        let resp = area.show(ctx, |ui| {
            egui::Frame::popup(&ctx.global_style()).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                ui.set_width(width.clamp(260.0, 900.0));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let what = match h.kind() {
                        HunkKind::Added => format!("{} new line{}", h.new.len(), if h.new.len() == 1 { "" } else { "s" }),
                        HunkKind::Deleted => format!("{} line{} deleted", h.old.len(), if h.old.len() == 1 { "" } else { "s" }),
                        HunkKind::Modified => format!("{} line{} changed", h.old.len().max(h.new.len()), if h.old.len().max(h.new.len()) == 1 { "" } else { "s" }),
                    };
                    ui.label(egui::RichText::new(format!("Change {} of {total}", peek.hunk + 1)).strong().color(v.strong_text_color()));
                    ui.label(egui::RichText::new(format!("  {what}, since staged")).color(v.weak_text_color()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if glyph_button(ui, Glyph::Close, "Close (Esc)", true).clicked() {
                            act = Some("close");
                        }
                        if glyph_button(ui, Glyph::Down, "Next change (Alt+F5)", total > 1).clicked() {
                            act = Some("next");
                        }
                        if glyph_button(ui, Glyph::Up, "Previous change (Shift+Alt+F5)", total > 1).clicked() {
                            act = Some("prev");
                        }
                        ui.add_space(6.0);
                        if ui.add(egui::Button::new("Revert").corner_radius(r_sm())).on_hover_text("Restores the staged lines. Ctrl+Z undoes this.").clicked() {
                            act = Some("revert");
                        }
                        if rel.is_some() && ui.add_enabled(self.git.running.is_none(), egui::Button::new("Stage").corner_radius(r_sm())).on_hover_text("Stages only this change").clicked() {
                            act = Some("stage");
                        }
                    });
                });
                if !old.is_empty() {
                    ui.add_space(6.0);
                    let bg = palette::mix(v.extreme_bg_color, st.danger, 0.14);
                    egui::Frame::NONE.fill(bg).corner_radius(r_sm()).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
                        egui::ScrollArea::both().max_height(260.0).auto_shrink([false, true]).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            for l in &old {
                                let l = l.trim_end_matches('\n').trim_end_matches('\r').replace('\t', "    ");
                                ui.add(egui::Label::new(egui::RichText::new(format!("- {l}")).font(font.clone()).color(palette::mix(v.text_color(), st.danger, 0.35))).extend());
                            }
                        });
                    });
                }
            });
        });
        let esc = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
        // A click outside closes it (not the click that opened it, on its mark).
        let clicked_out = ctx.input(|i| i.pointer.primary_clicked()) && ctx.pointer_interact_pos().is_some_and(|p| !resp.response.rect.contains(p) && !at.expand2(Vec2::new(6.0, 2.0)).contains(p));
        if esc || clicked_out {
            act = act.or(Some("close"));
        }
        match act {
            Some("close") => self.peek = None,
            Some("next") | Some("prev") => {
                let k = if act == Some("next") { (peek.hunk + 1) % total } else { (peek.hunk + total - 1) % total };
                if let Some(h2) = self.tabs.get(self.active).and_then(|t| t.gutter.hunks.get(k)) {
                    self.goto = Some(h2.new.start + 1);
                }
                self.peek = Some(Peek { path: peek.path, hunk: k });
            }
            Some("revert") => {
                if let Some(t) = self.tabs.get_mut(self.active) {
                    t.hist.command();
                    let reverted = rivet_git::diff::revert(&base, &t.text, &h);
                    t.text.set(reverted);
                    t.sels.clear();
                    t.preview = false;
                }
                self.peek = None;
            }
            Some("stage") => {
                if let (Some(rel), Some(t)) = (rel, self.tabs.get(self.active)) {
                    let text = rivet_git::diff::apply(&base, &t.text, &h);
                    self.git.run(Op::SetIndex { path: rel, text, what: "Staged the change" });
                }
                self.peek = None;
            }
            _ => {}
        }
    }
}

/// Files to discard: tracked ones (put back) and new ones (deleted).
fn split<'a>(it: impl Iterator<Item = &'a rivet_git::Entry>) -> (Vec<String>, Vec<String>) {
    let (mut t, mut u) = (Vec::new(), Vec::new());
    for e in it {
        if e.untracked() { u.push(e.path.clone()) } else { t.push(e.path.clone()) }
    }
    (t, u)
}
