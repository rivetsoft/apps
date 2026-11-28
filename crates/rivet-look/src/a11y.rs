//! Accessibility shared by the Rivet apps: names, roles and states for drawn controls, so a
//! screen reader reads them; a focus ring that follows the keyboard; and the desktop's wish for
//! higher contrast or less motion, which the theme takes up. The rule every widget follows is in
//! `docs/accessibility.md`.
//!
//! egui hands its widgets to AccessKit, which reaches AT-SPI on Linux. A control drawn by hand
//! (`allocate_exact_size` or `interact` with a click) is in that tree too, but with no name
//! or role: give it both with [`Access::role`], and its state with [`Access::selected`],
//! [`Access::toggled`] or [`Access::expanded`].

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, Once};

use eframe::egui::{self, Id, Key, Response, Stroke, StrokeKind, WidgetInfo};

pub use eframe::egui::accesskit::Role;

/// Names, roles and states for a control drawn by hand. Each returns the response, so they
/// chain: `resp.role(Role::Tab, &name).selected(on)`.
pub trait Access: Sized {
    /// What the control is and what it's called, as a screen reader says it.
    fn role(self, role: Role, label: impl Into<String>) -> Self;
    /// The chosen one of a list, a tab strip or a grid.
    fn selected(self, on: bool) -> Self;
    /// On or off: a check box, a switch, a toggle button.
    fn toggled(self, on: bool) -> Self;
    /// Open or closed: a group that folds, a menu button, a tree row.
    fn expanded(self, open: bool) -> Self;
    /// More about it, read after the name (a tooltip's words, a keyboard shortcut).
    fn described(self, text: impl Into<String>) -> Self;
    /// A tooltip that a screen reader says too: the name of a control that has none (an icon
    /// button), else read after its name.
    fn tip(self, text: impl Into<String>) -> Self;
}

impl Access for Response {
    fn role(self, role: Role, label: impl Into<String>) -> Self {
        let label = label.into();
        let enabled = self.enabled();
        self.widget_info(|| WidgetInfo::labeled(role, enabled, &label));
        self
    }

    fn selected(self, on: bool) -> Self {
        self.ctx.accesskit_node_builder(self.id, |n| n.set_selected(on));
        self
    }

    fn toggled(self, on: bool) -> Self {
        use egui::accesskit::Toggled;
        self.ctx.accesskit_node_builder(self.id, |n| n.set_toggled(if on { Toggled::True } else { Toggled::False }));
        self
    }

    fn expanded(self, open: bool) -> Self {
        self.ctx.accesskit_node_builder(self.id, |n| n.set_expanded(open));
        self
    }

    fn described(self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.ctx.accesskit_node_builder(self.id, |n| n.set_description(text));
        self
    }

    fn tip(self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.ctx.accesskit_node_builder(self.id, |n| {
            if n.label().is_none_or(str::is_empty) && n.labelled_by().is_empty() {
                n.set_label(text.as_str());
            } else if n.label() != Some(text.as_str()) {
                n.set_description(text.as_str());
            }
        });
        self.on_hover_text(text)
    }
}

/// Name the area a `Ui` covers (a sidebar, a tab strip, a list), so a screen reader says where
/// the keyboard is: `ui.scope_builder(a11y::area(Role::TabList, "Tabs"), …)`.
pub fn area(role: Role, label: &str) -> egui::UiBuilder {
    egui::UiBuilder::new().accessibility_role(role).accessibility_label(label)
}

// ---- the desktop's wishes

static HIGH_CONTRAST: AtomicBool = AtomicBool::new(false);
static REDUCED_MOTION: AtomicBool = AtomicBool::new(false);
/// Moves on each change, so a theme built from the wishes knows to rebuild.
static GENERATION: AtomicU32 = AtomicU32::new(0);
static WATCH: Once = Once::new();
/// The windows to redraw when the desktop's wishes change (Rivet Shell has several).
static WINDOWS: Mutex<Vec<egui::Context>> = Mutex::new(Vec::new());

/// The desktop asks for stronger edges and text.
pub fn high_contrast() -> bool {
    HIGH_CONTRAST.load(Ordering::Relaxed)
}

/// The desktop asks that things appear and go at once, without sliding or fading.
pub fn reduced_motion() -> bool {
    REDUCED_MOTION.load(Ordering::Relaxed)
}

/// Changes whenever [`high_contrast`] or [`reduced_motion`] does (for a cache key).
pub fn generation() -> u32 {
    GENERATION.load(Ordering::Relaxed)
}

/// Set the wishes by hand: checks and screenshots, which never ask the desktop.
pub fn set_prefs(high_contrast: bool, reduced_motion: bool) {
    let old = (HIGH_CONTRAST.swap(high_contrast, Ordering::Relaxed), REDUCED_MOTION.swap(reduced_motion, Ordering::Relaxed));
    if old != (high_contrast, reduced_motion) {
        GENERATION.fetch_add(1, Ordering::Relaxed);
    }
}

/// Call once per frame (`LiveLook::frame` does): adds the focus ring, and in a live window
/// follows the desktop's contrast and motion settings from a thread that wakes the window
/// when they change. `RIVET_HIGH_CONTRAST=1` and `RIVET_REDUCED_MOTION=1` set them for a
/// screenshot.
pub fn setup(ctx: &egui::Context, live: bool) {
    if !ctx.data(|d| d.get_temp::<bool>(Id::unique("rivet-a11y-setup")).is_some()) {
        ctx.data_mut(|d| d.insert_temp(Id::unique("rivet-a11y-setup"), true));
        ctx.add_plugin(FocusRing::default());
        if live {
            WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).push(ctx.clone());
        }
    }
    let env = |k: &str| std::env::var(k).is_ok_and(|v| v == "1");
    if !live {
        if env("RIVET_HIGH_CONTRAST") || env("RIVET_REDUCED_MOTION") {
            set_prefs(env("RIVET_HIGH_CONTRAST"), env("RIVET_REDUCED_MOTION"));
        }
        return;
    }
    WATCH.call_once(|| {
        let _ = std::thread::Builder::new().name("rivet-a11y-prefs".into()).spawn(|| {
            rivet_xdg::appearance::watch(|p| {
                let before = generation();
                set_prefs(p.high_contrast, p.reduced_motion);
                if generation() != before {
                    for ctx in WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).iter() {
                        ctx.request_repaint();
                    }
                }
            });
        });
    });
}

// ---- the focus ring

/// Draw this frame's focus ring for `id` just inside its edge instead of outside: for a large
/// area that takes the keyboard as a whole (Rivet Files' file list), whose outside edge may be
/// cut off by the window.
pub fn ring_inside(ctx: &egui::Context, id: Id) {
    ctx.data_mut(|d| d.insert_temp(Id::unique("rivet-ring-inside"), Some(id)));
}

/// A ring around the control that has the keyboard, in the accent, while the keyboard is in
/// use: Tab turns it on, a click turns it off (a ring after every click only distracts).
#[derive(Default)]
struct FocusRing {
    keyboard: bool,
}

impl egui::Plugin for FocusRing {
    fn debug_name(&self) -> &'static str {
        "rivet-focus-ring"
    }

    fn input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        for e in &input.events {
            match e {
                egui::Event::Key { key: Key::Tab, pressed: true, .. } => self.keyboard = true,
                egui::Event::PointerButton { pressed: true, .. } => self.keyboard = false,
                _ => {}
            }
        }
    }

    fn on_end_pass(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let inside = ctx.data_mut(|d| d.remove_temp::<Option<Id>>(Id::unique("rivet-ring-inside"))).flatten();
        if !self.keyboard {
            return;
        }
        let Some(r) = ctx.memory(|m| m.focused()).and_then(|id| ctx.read_response(id)) else { return };
        if !r.interact_rect.is_positive() {
            return;
        }
        let (rect, radius) = if inside == Some(r.id) { (r.interact_rect.shrink(2.0), crate::theme::r_lg()) } else { (r.interact_rect.expand(2.0), crate::theme::r_sm()) };
        let v = ctx.global_style().visuals.clone();
        let width = if high_contrast() { 3.0 } else { 2.0 };
        let color = if high_contrast() { v.strong_text_color() } else { v.selection.bg_fill };
        ctx.layer_painter(r.layer_id).rect_stroke(rect, radius, Stroke::new(width, color), StrokeKind::Outside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drawn_control_gets_its_name_role_and_state() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut id = None;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let (_, r) = ui.allocate_exact_size(egui::vec2(40.0, 20.0), egui::Sense::click());
            id = Some(r.role(Role::Tab, "Notes.md").selected(true).id);
        });
        out.textures_delta.clear();
        let update = out.platform_output.accesskit_update.expect("a tree");
        let node = &update.nodes.iter().find(|(n, _)| *n == id.unwrap().accesskit_id()).expect("the tab").1;
        assert_eq!(node.role(), Role::Tab);
        assert_eq!(node.label(), Some("Notes.md"));
        assert_eq!(node.is_selected(), Some(true));
    }
}
