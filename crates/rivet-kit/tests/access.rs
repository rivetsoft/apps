//! Every control in a sample window has a name and a role a screen reader can say: Rivet's
//! shared controls (tabs, the quick panel, an in-app window, the drawn buttons) beside egui's
//! own. Fails on any control a person can click or Tab to that reads as nothing.

use eframe::egui::{self, Color32, Rect, Vec2};
use rivet_kit::access;
use rivet_look::quick::{QuickPanel, QuickRow, QuickText};
use rivet_look::{tabs, widgets, windows};

fn sample(ui: &mut egui::Ui, quick: &mut QuickPanel, win: &mut windows::WinState, on: &mut bool, text: &mut String) {
    let ctx = ui.ctx().clone();
    let strip = tabs::TabStrip::new("sample", Color32::from_rgb(0x35, 0x84, 0xe4));
    let list = [
        tabs::Tab { label: "notes.md".into(), dirty: true, ..Default::default() },
        tabs::Tab { label: "todo.md".into(), ..Default::default() },
        tabs::Tab { label: "ideas.md".into(), badge: Some("2".into()), ..Default::default() },
    ];
    strip.show(ui, &list, 0);
    ui.horizontal(|ui| {
        widgets::arrow_button(ui, true, true);
        widgets::arrow_button(ui, false, false);
        widgets::trash_button(ui);
        widgets::danger_button(ui, "Delete");
    });
    ui.checkbox(on, "Show hidden files");
    ui.horizontal(|ui| {
        let l = ui.label("Name");
        ui.text_edit_singleline(text).labelled_by(l.id);
    });
    let chrome = windows::Chrome { title: "Preferences".into(), icon: None, close: None, flush: false };
    windows::show(&ctx, win, &chrome, Rect::from_min_size(egui::pos2(0.0, 200.0), Vec2::new(500.0, 300.0)), Vec2::new(200.0, 100.0), |ui| {
        let _ = ui.button("Done");
    });
    let rows = [
        QuickRow::Item { icon: None, name: "Documents".into(), sub: "~/Documents".into() },
        QuickRow::Command { group: "View".into(), label: "Show hidden files".into(), keys: "Ctrl+H".into(), state: Some(true) },
    ];
    quick.show(&ctx, "sample-quick", &rows, &QuickText { hint: "Go to a folder or run a command", empty: "Nothing matches", legend: "Enter to open" }, None);
}

#[test]
fn every_control_in_the_sample_window_has_a_name() {
    let ctx = egui::Context::default();
    let (mut quick, mut win, mut on, mut text) = (QuickPanel::new(), windows::WinState::new(1, Vec2::new(300.0, 200.0)), false, String::new());
    let tree = access::tree(&ctx, Vec2::new(900.0, 700.0), |ui| sample(ui, &mut quick, &mut win, &mut on, &mut text));
    let missing = access::unlabeled(&tree);
    let all = access::describe(&tree);
    assert!(missing.is_empty(), "controls with no name or role:\n{}\n\nthe whole tree:\n{all}", missing.join("\n"));
    // The shared controls are really in the tree, with the names a screen reader says.
    for want in ["Tab \"notes.md\"", "Button \"Move up\"", "Button \"Remove\"", "TitleBar \"Preferences\"", "Button \"Close\"", "ListBoxOption \"Documents\"", "ListBoxOption \"View: Show hidden files\""] {
        assert!(all.contains(want), "no {want} in the tree:\n{all}");
    }
}

#[test]
fn the_check_catches_a_control_with_no_name() {
    let ctx = egui::Context::default();
    let tree = access::tree(&ctx, Vec2::new(200.0, 100.0), |ui| {
        ui.allocate_exact_size(Vec2::new(40.0, 20.0), egui::Sense::click());
    });
    assert_eq!(access::unlabeled(&tree).len(), 1);
}
