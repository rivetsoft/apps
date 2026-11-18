//! Settings, Displays: the screens drawn where they sit (drag one to move it; it snaps to the
//! others' edges), and the chosen one's resolution, refresh rate, scale and rotation. Apply
//! writes them to the overrides file the compositor's config includes (see displays.rs and
//! comp.rs), and goes back by itself unless the change is kept within a few seconds, in case
//! a screen went dark.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::fonts::ui_font;
use rivet_look::palette::mix;
use rivet_look::theme::{r_md, r_sm};

use super::panel_editor::segmented;
use super::{SettingsApp, grid, hint, section};
use crate::comp;
use crate::config::{NO_SAVE, Settings};
use crate::displays::{self, Display, Mode, Output};
use crate::sys::Compositor;

/// How long a change waits to be kept.
const KEEP_FOR: Duration = Duration::from_secs(15);

/// A change waiting to be kept: what to go back to, and the thread that does at the end.
struct Pending {
    until: Instant,
    old: Vec<Display>,
    kept: Arc<AtomicBool>,
    went_back: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct State {
    outputs: Vec<Output>,
    /// What's being set: one for each connected screen.
    draft: Vec<Display>,
    edited: bool,
    read_at: Option<f64>,
    err: Option<String>,
    sel: usize,
    /// While a screen is dragged: which, and where it is before snapping.
    drag: Option<(usize, Vec2)>,
    /// The drawing's scale and the point at its middle, kept still while dragging.
    view: Option<(f32, Vec2)>,
    pending: Option<Pending>,
    msg: Option<Result<String, String>>,
}

/// Screens for checks (three, the last one scaled).
fn demo() -> Vec<Output> {
    let modes = |w, h, rates: &[u32]| rates.iter().map(|&mhz| Mode { w, h, mhz }).collect::<Vec<_>>();
    let out = |name: &str, what: &str, modes: Vec<Mode>, scale: f32, x: i32, on: bool| Output { name: name.into(), what: what.into(), preferred: modes[0], vrr_supported: name == "DP-1", now: Display { name: name.into(), on, mode: Some(modes[0]), scale, pos: (x, 0), rotate: 0, flip: false, vrr: false }, modes };
    let mut a = modes(2560, 1440, &[143973, 119998, 59951]);
    a.extend(modes(1920, 1080, &[120000, 60000]));
    vec![out("DP-1", "Dell Inc. AW2725DM", a, 1.0, 0, true), out("DP-2", "LG Electronics LG ULTRAFINE", modes(3840, 2160, &[59996, 30000]), 1.25, 2560, true), out("eDP-1", "BOE 0x0A59", modes(1920, 1200, &[60003, 48003]), 1.0, 5632, false)]
}

fn size_of(d: &Display, outs: &[Output]) -> (f32, f32) {
    let fallback = outs.iter().find(|o| o.name == d.name).map(|o| o.preferred).unwrap_or(Mode { w: 1920, h: 1080, mhz: 60000 });
    d.size(fallback)
}

/// Move every screen so the top left of them all is 0, 0.
fn tidy(draft: &mut [Display]) {
    let (x, y) = (draft.iter().map(|d| d.pos.0).min().unwrap_or(0), draft.iter().map(|d| d.pos.1).min().unwrap_or(0));
    for d in draft {
        d.pos = (d.pos.0 - x, d.pos.1 - y);
    }
}

/// `at` moved onto the nearest of `spots` within `reach`.
fn snap(at: f32, spots: impl Iterator<Item = f32>, reach: f32) -> f32 {
    spots.map(|s| (s, (s - at).abs())).filter(|s| s.1 <= reach).min_by(|a, b| a.1.total_cmp(&b.1)).map(|s| s.0).unwrap_or(at)
}

/// Write the settings' displays out now (the shell does too when it's running).
fn write(s: &Settings) -> Result<(), String> {
    if NO_SAVE.load(Ordering::Relaxed) {
        return Ok(());
    }
    s.save();
    comp::apply_now(Compositor::detect(), s)
}

impl SettingsApp {
    pub(super) fn displays_page(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let comp = Compositor::detect();
        let shot = self.shot.is_some();
        let target = comp::target(comp);
        let now = ctx.input(|i| i.time);
        let Some(t) = target.filter(|t| displays::supported(t.format) || shot) else {
            section(ui, "Displays", &mut |ui| {
                hint(ui, &format!("Screens can be set here for niri and for Hyprland with hyprland.conf. This session is {}, so they're left to its own settings.", comp.name()));
            });
            return;
        };
        // ---- what's connected: read again every couple of seconds while nothing is being changed
        let st = &mut self.disp;
        if st.read_at.is_none_or(|at| now - at > 2.0 && !st.edited && st.drag.is_none()) {
            st.read_at = Some(now);
            match if shot { Ok(demo()) } else { displays::read(comp) } {
                Ok(outs) => {
                    st.err = None;
                    if outs != st.outputs || st.draft.is_empty() {
                        st.draft = outs.iter().map(|o| Display { mode: o.now.mode.or(Some(o.preferred)), ..o.now.clone() }).collect();
                        st.outputs = outs;
                        st.view = None;
                    }
                }
                Err(e) => st.err = Some(e),
            }
            st.sel = st.sel.min(st.draft.len().saturating_sub(1));
        }
        ctx.request_repaint_after(Duration::from_secs(1));
        // ---- a change waiting to be kept
        let mut go_back = false;
        if let Some(p) = &st.pending {
            if p.went_back.load(Ordering::Relaxed) {
                // The thread put the old ones back: the settings here follow.
                self.s.displays = p.old.clone();
                (st.pending, st.read_at, st.edited) = (None, None, false);
                st.msg = Some(Ok("Nothing was kept, so the screens are as they were.".into()));
            } else {
                let left = p.until.saturating_duration_since(Instant::now()).as_secs() + 1;
                let warn = rivet_look::theme::status(&ctx).warning;
                egui::Frame::NONE.fill(mix(v.panel_fill, warn, 0.14)).stroke(Stroke::new(1.0, mix(v.panel_fill, warn, 0.5))).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Keep these display settings?").strong());
                            hint(ui, &format!("Going back to how they were in {left} second{}.", if left == 1 { "" } else { "s" }));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Go Back").clicked() {
                                go_back = true;
                            }
                            if ui.add(egui::Button::new(egui::RichText::new("Keep").color(v.selection.stroke.color)).fill(accent)).clicked() {
                                p.kept.store(true, Ordering::Relaxed);
                            }
                        });
                    });
                });
                ui.add_space(14.0);
                ctx.request_repaint_after(Duration::from_millis(250));
            }
        }
        if let Some(p) = st.pending.take_if(|p| go_back || p.kept.load(Ordering::Relaxed)) {
            // Kept (the thread then does nothing), or put back now.
            p.kept.store(true, Ordering::Relaxed);
            if go_back {
                self.s.displays = p.old;
                st.msg = Some(write(&self.s).map(|_| "The screens are as they were.".into()));
                (st.read_at, st.edited) = (None, false);
                st.draft.clear();
            } else {
                st.msg = Some(Ok("Kept.".into()));
            }
        }
        if let Some(e) = &st.err {
            ui.label(egui::RichText::new(e).color(rivet_look::theme::status(&ctx).danger));
            ui.add_space(8.0);
        }
        if st.draft.is_empty() {
            hint(ui, "No screens found.");
            return;
        }
        // ---- the arrangement
        ui.label(egui::RichText::new("Arrangement").strong().size(14.5));
        ui.add_space(4.0);
        let (canvas, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 240.0), Sense::hover());
        ui.painter().rect_filled(canvas, r_md(), v.faint_bg_color);
        let sizes: Vec<(f32, f32)> = st.draft.iter().map(|d| size_of(d, &st.outputs)).collect();
        let rect_of = |d: &Display, s: (f32, f32)| Rect::from_min_size(Pos2::new(d.pos.0 as f32, d.pos.1 as f32), Vec2::new(s.0, s.1));
        let (k, mid) = *st.view.get_or_insert_with(|| {
            let all = st.draft.iter().zip(&sizes).fold(Rect::NOTHING, |a, (d, s)| a.union(rect_of(d, *s)));
            (((canvas.width() - 56.0) / all.width().max(1.0)).min((canvas.height() - 56.0) / all.height().max(1.0)), all.center().to_vec2())
        });
        let to_screen = |r: Rect| Rect::from_min_size(canvas.center() + (r.min.to_vec2() - mid) * k, r.size() * k);
        let mut released = false;
        for i in 0..st.draft.len() {
            let r = to_screen(rect_of(&st.draft[i], sizes[i])).shrink(1.5);
            let resp = ui.interact(r, egui::Id::unique(("display", i)), Sense::click_and_drag());
            if resp.clicked() || resp.drag_started() {
                st.sel = i;
            }
            if resp.drag_started() {
                st.drag = Some((i, Vec2::new(st.draft[i].pos.0 as f32, st.draft[i].pos.1 as f32)));
            }
            if resp.dragged()
                && let Some((who, at)) = st.drag.as_mut().filter(|d| d.0 == i)
            {
                *at += resp.drag_delta() / k;
                let (w, h) = sizes[*who];
                let others = || st.draft.iter().zip(&sizes).enumerate().filter(|(j, _)| *j != i).map(|(_, (d, s))| (d.pos.0 as f32, d.pos.1 as f32, s.0, s.1));
                // Onto the others' edges: side by side, or lined up.
                let x = snap(at.x, others().flat_map(|(ox, _, ow, _)| [ox - w, ox + ow, ox, ox + ow - w]), 18.0 / k);
                let y = snap(at.y, others().flat_map(|(_, oy, _, oh)| [oy - h, oy + oh, oy, oy + oh - h]), 18.0 / k);
                st.draft[i].pos = (x.round() as i32, y.round() as i32);
                st.edited = true;
            }
            if resp.drag_stopped() {
                released = true;
            }
            let d = &st.draft[i];
            let sel = i == st.sel;
            let p = ui.painter().with_clip_rect(canvas);
            let base = if d.on { mix(v.panel_fill, accent, if sel { 0.34 } else { 0.14 }) } else { mix(v.faint_bg_color, v.text_color(), 0.05) };
            p.rect_filled(r, r_sm(), base);
            let line = if sel { Stroke::new(2.0, accent) } else { Stroke::new(1.0, mix(v.panel_fill, v.text_color(), if d.on { 0.3 } else { 0.15 })) };
            p.rect_stroke(r, r_sm(), line, egui::StrokeKind::Inside);
            let col = if d.on { v.strong_text_color() } else { v.weak_text_color() };
            p.text(r.center() - Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, &d.name, ui_font(13.5), col);
            let sub = if d.on { format!("{:.0} × {:.0}", sizes[i].0, sizes[i].1) } else { "Off".to_string() };
            p.text(r.center() + Vec2::new(0.0, 10.0), Align2::CENTER_CENTER, sub, ui_font(11.5), v.weak_text_color());
            resp.on_hover_cursor(if st.drag.is_some() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        }
        if released {
            st.drag = None;
            st.view = None;
            tidy(&mut st.draft);
        }
        let clash = (0..st.draft.len()).any(|i| (0..i).any(|j| st.draft[i].on && st.draft[j].on && displays::overlap((st.draft[i].pos, sizes[i]), (st.draft[j].pos, sizes[j]))));
        ui.add_space(4.0);
        if clash && st.drag.is_none() {
            ui.label(egui::RichText::new("Two screens overlap. Drag them apart before applying.").small().color(rivet_look::theme::status(&ctx).warning));
        } else {
            hint(ui, "Drag a screen to where it sits on your desk; it snaps to the others' edges. Sizes are as the desktop sees them, after scaling.");
        }
        ui.add_space(14.0);
        // ---- the chosen screen
        let sel = st.sel;
        let out = st.outputs.iter().find(|o| o.name == st.draft[sel].name).cloned();
        let lit = st.draft.iter().filter(|d| d.on).count();
        let before = st.draft[sel].clone();
        let title = out.as_ref().map(|o| o.what.clone()).filter(|w| !w.is_empty()).map(|w| format!("{} · {w}", before.name)).unwrap_or_else(|| before.name.clone());
        section(ui, &title, &mut |ui| {
            let d = &mut st.draft[sel];
            // The last screen that's on stays on.
            ui.add_enabled(!(d.on && lit <= 1), egui::Checkbox::new(&mut d.on, "Use this screen")).on_disabled_hover_text("The only screen in use stays on");
            let Some(o) = out.as_ref().filter(|_| d.on) else { return };
            ui.add_space(6.0);
            let mode = d.mode.unwrap_or(o.preferred);
            grid(ui, "display-opts", &mut |ui| {
                ui.label("Resolution");
                let mut sizes: Vec<(u32, u32)> = o.modes.iter().map(|m| (m.w, m.h)).collect();
                sizes.sort_by_key(|s| std::cmp::Reverse((s.0 * s.1, s.0)));
                sizes.dedup();
                egui::ComboBox::from_id_salt("display-res").selected_text(format!("{} × {}", mode.w, mode.h)).width(170.0).show_ui(ui, |ui| {
                    for (w, h) in sizes {
                        let own = if (w, h) == (o.preferred.w, o.preferred.h) { "  (its own)" } else { "" };
                        if ui.selectable_label((w, h) == (mode.w, mode.h), format!("{w} × {h}{own}")).clicked() && (w, h) != (mode.w, mode.h) {
                            // The fastest rate it has at that size.
                            d.mode = o.modes.iter().copied().filter(|m| (m.w, m.h) == (w, h)).max_by_key(|m| m.mhz);
                        }
                    }
                });
                ui.end_row();
                ui.label("Refresh rate");
                let mut rates: Vec<Mode> = o.modes.iter().copied().filter(|m| (m.w, m.h) == (mode.w, mode.h)).collect();
                rates.sort_by_key(|m| std::cmp::Reverse(m.mhz));
                rates.dedup_by_key(|m| (m.mhz as f32 / 10.0).round() as u32);
                egui::ComboBox::from_id_salt("display-rate").selected_text(format!("{:.2} Hz", mode.hz())).width(170.0).show_ui(ui, |ui| {
                    for m in rates {
                        if ui.selectable_label(m == mode, format!("{:.2} Hz", m.hz())).clicked() {
                            d.mode = Some(m);
                        }
                    }
                });
                ui.end_row();
                ui.label("Scale");
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("display-scale").selected_text(format!("{:.0}%", d.scale * 100.0)).width(90.0).show_ui(ui, |ui| {
                        for s in displays::scales(d.scale) {
                            if ui.selectable_label((s - d.scale).abs() < 0.005, format!("{:.0}%", s * 100.0)).clicked() {
                                d.scale = s;
                            }
                        }
                    });
                    let (w, h) = d.size(o.preferred);
                    hint(ui, &format!("Looks like {w:.0} × {h:.0}"));
                });
                ui.end_row();
                ui.label("Rotation");
                segmented(ui, &mut d.rotate, &[(0, "None"), (90, "90°"), (180, "180°"), (270, "270°")]);
                ui.end_row();
                if o.vrr_supported {
                    ui.label("");
                    ui.checkbox(&mut d.vrr, "Variable refresh rate").on_hover_text("The screen follows the app's frame rate (FreeSync, G-Sync)");
                    ui.end_row();
                }
            });
        });
        if st.draft[sel] != before {
            st.edited = true;
            // The screens after it keep their places beside it as its size changes.
            let (old, new) = (size_of(&before, &st.outputs), size_of(&st.draft[sel], &st.outputs));
            let (right, bottom) = (before.pos.0 as f32 + old.0, before.pos.1 as f32 + old.1);
            if before.on && st.draft[sel].on {
                for (_, d) in st.draft.iter_mut().enumerate().filter(|(j, _)| *j != sel) {
                    if d.pos.0 as f32 >= right - 0.5 {
                        d.pos.0 += (new.0 - old.0).round() as i32;
                    }
                    if d.pos.1 as f32 >= bottom - 0.5 {
                        d.pos.1 += (new.1 - old.1).round() as i32;
                    }
                }
            }
            st.view = None;
        }
        // ---- apply
        let key = egui::Id::unique("displays-included");
        let included = shot || {
            match ctx.data(|d| d.get_temp::<(f64, bool)>(key)) {
                Some((at, inc)) if now - at < 2.0 => inc,
                _ => {
                    let inc = comp::included(&t);
                    ctx.data_mut(|d| d.insert_temp(key, (now, inc)));
                    inc
                }
            }
        };
        let name = t.main.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let ours = t.ours.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if !included {
            let warn = rivet_look::theme::status(&ctx).warning;
            ui.label(egui::RichText::new(format!("{name} doesn't include {ours} yet, so nothing set here would show.")).color(warn));
            ui.add_space(4.0);
            if ui.button(format!("Add the Include to {name}")).clicked() {
                let (a, dark) = comp::shell_look(&self.s);
                let look = self.s.comp_on.then(|| comp::Look::from(&self.s, a, dark));
                st.msg = Some(comp::add_include(&t, look.as_ref(), &self.s.displays).map(|_| format!("Added. The original is kept as {name}.rivet-backup.")));
                ctx.data_mut(|d| d.remove::<(f64, bool)>(key));
            }
            hint(ui, &format!("One line at its end ({}), so what's set here wins over the screens set above it. A copy of the file is kept first.", comp::include_line(&t)));
            ui.add_space(10.0);
        }
        let mut apply = false;
        let mut leave = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let ready = st.edited && included && !clash && st.pending.is_none();
            if ui.add_enabled(ready, egui::Button::new(egui::RichText::new("Apply").color(if ready { v.selection.stroke.color } else { v.weak_text_color() })).fill(if ready { accent } else { v.faint_bg_color })).clicked() {
                apply = true;
            }
            if ui.add_enabled(st.edited, egui::Button::new("Reset")).on_hover_text("Back to how the screens are now").clicked() {
                (st.edited, st.read_at) = (false, None);
                st.draft.clear();
            }
            if !self.s.displays.is_empty() && st.pending.is_none() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(format!("Leave the Screens to {}", comp.name())).on_hover_text(format!("Take the screens out of {ours}, so {name} sets them again")).clicked() {
                        leave = true;
                    }
                });
            }
        });
        if apply {
            // Every connected screen as drawn; ones set before and not connected now are kept.
            let old = self.s.displays.clone();
            let mut all: Vec<Display> = old.iter().filter(|d| !st.draft.iter().any(|n| n.name == d.name)).cloned().collect();
            all.extend(st.draft.iter().cloned());
            all.sort_by(|a, b| a.name.cmp(&b.name));
            self.s.displays = all;
            match write(&self.s) {
                Ok(()) => {
                    let (kept, went_back) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
                    st.pending = Some(Pending { until: Instant::now() + KEEP_FOR, old: old.clone(), kept: kept.clone(), went_back: went_back.clone() });
                    // On a thread: a window on a screen that went dark may never be drawn again.
                    if !NO_SAVE.load(Ordering::Relaxed) {
                        std::thread::spawn(move || {
                            std::thread::sleep(KEEP_FOR);
                            if !kept.load(Ordering::Relaxed) {
                                let mut s = Settings::load();
                                s.displays = old;
                                let _ = write(&s);
                                went_back.store(true, Ordering::Relaxed);
                            }
                        });
                    }
                    st.msg = None;
                }
                Err(e) => {
                    self.s.displays = old;
                    st.msg = Some(Err(e));
                }
            }
            (st.edited, st.read_at) = (false, Some(now - 1.0));
        }
        if leave {
            self.s.displays.clear();
            st.msg = Some(write(&self.s).map(|_| format!("{name} sets the screens again.")));
            (st.edited, st.read_at) = (false, None);
            st.draft.clear();
        }
        ui.add_space(6.0);
        match &st.msg {
            Some(Ok(m)) => hint(ui, m),
            Some(Err(e)) => {
                ui.label(egui::RichText::new(e).color(rivet_look::theme::status(&ctx).danger));
            }
            None => hint(ui, &format!("Apply writes the screens to {ours}, which {name} includes, and asks whether to keep them: with no answer in {} seconds they go back.", KEEP_FOR.as_secs())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_and_tidies() {
        assert_eq!(snap(2550.0, [2560.0, 0.0].into_iter(), 20.0), 2560.0);
        assert_eq!(snap(2500.0, [2560.0, 0.0].into_iter(), 20.0), 2500.0);
        let mut d = demo().into_iter().map(|o| o.now).collect::<Vec<_>>();
        d[0].pos = (-100, 50);
        tidy(&mut d);
        assert_eq!((d[0].pos, d[1].pos), ((0, 50), (2660, 0)));
    }
}
