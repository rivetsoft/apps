//! Checks that a window reads to a screen reader: run a frame with AccessKit on and look for
//! controls a person can click or Tab to that have no name. egui gives its own widgets a name
//! from their text; a control drawn by hand has none until it is given one
//! (`rivet_look::a11y::Access`). The rule is in `docs/accessibility.md`.

use eframe::egui::accesskit::{Action, Node, Role, TreeUpdate};
use eframe::egui::{self, Vec2};

/// The AccessKit tree of `ui` drawn in a window of `size`, after a few frames so that
/// anything laid out from last frame's sizes has settled.
pub fn tree(ctx: &egui::Context, size: Vec2, mut ui: impl FnMut(&mut egui::Ui)) -> TreeUpdate {
    ctx.enable_accesskit();
    let input = || egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)), ..Default::default() };
    let mut last = None;
    for _ in 0..3 {
        let mut out = ctx.run_ui(input(), &mut ui);
        out.textures_delta.clear();
        last = out.platform_output.accesskit_update;
    }
    last.expect("AccessKit is on, so each frame has a tree")
}

/// Can a person act on it: click it, Tab to it, type in it?
fn interactive(n: &Node) -> bool {
    n.supports_action(Action::Click) || n.supports_action(Action::Focus) || matches!(n.role(), Role::Button | Role::CheckBox | Role::Switch | Role::RadioButton | Role::Tab | Role::MenuItem | Role::ListBoxOption | Role::TextInput | Role::Slider | Role::ComboBox | Role::Link)
}

/// Does it have a name a screen reader can say?
fn named(n: &Node) -> bool {
    let some = |s: Option<&str>| s.is_some_and(|s| !s.trim().is_empty());
    some(n.label()) || !n.labelled_by().is_empty() || (n.role() == Role::Label && some(n.value())) || (n.role() == Role::TextInput && some(n.placeholder()))
}

/// The controls a person can act on that have no name, or no role beyond "unknown", each as
/// its role and where it is on the window (to find it in a screenshot).
pub fn unlabeled(update: &TreeUpdate) -> Vec<String> {
    update
        .nodes
        .iter()
        .filter(|(_, n)| interactive(n) && (!named(n) || n.role() == Role::Unknown))
        .map(|(id, n)| {
            let at = n.bounds().map(|b| format!(" at {:.0},{:.0} {:.0}×{:.0}", b.x0, b.y0, b.x1 - b.x0, b.y1 - b.y0)).unwrap_or_default();
            format!("{:?} {:?} (label {:?}){at}", id, n.role(), n.label())
        })
        .collect()
}

/// Every node with its role and name, one per line, for reading what a window says.
pub fn describe(update: &TreeUpdate) -> String {
    let mut out = String::new();
    for (id, n) in &update.nodes {
        let name = n.label().or(n.value()).unwrap_or("");
        out.push_str(&format!("{id:?} {:?} {name:?}\n", n.role()));
    }
    out
}
