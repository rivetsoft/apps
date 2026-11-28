//! Choosing a font: a button showing the family in use, opening a list of the installed
//! ones to search, each name written in the interface's font.

use eframe::egui;

use std::sync::OnceLock;

/// The installed families, found once (fontconfig's `fc-list`).
fn families() -> &'static [String] {
    static ALL: OnceLock<Vec<String>> = OnceLock::new();
    ALL.get_or_init(rivet_look::fonts::families)
}

/// The family the desktop's own font is in, for the "like the desktop" entry.
pub fn desktop_name() -> String {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| rivet_look::fonts::resolve("").map(|(f, _)| f).unwrap_or_else(|| "Sans".into())).clone()
}

/// `value` is a family's name, or empty for `follow` (what an empty choice is called here,
/// e.g. "Like Appearance (Inter)"). True when it changed.
pub fn font_pick(ui: &mut egui::Ui, id: &str, value: &mut String, follow: &str) -> bool {
    let before = value.clone();
    let shown = if value.is_empty() { follow.to_string() } else { value.clone() };
    let query_id = ui.make_persistent_id((id, "query"));
    egui::ComboBox::from_id_salt(id).selected_text(shown).width(240.0).height(320.0).show_ui(ui, |ui| {
        let mut query: String = ui.data_mut(|d| d.get_temp(query_id)).unwrap_or_default();
        let edit = ui.add(egui::TextEdit::singleline(&mut query).hint_text("Search fonts").desired_width(f32::INFINITY)).accessible_name("Search fonts");
        if !edit.has_focus() && query.is_empty() && ui.memory(|m| m.focused().is_none()) {
            edit.request_focus();
        }
        ui.data_mut(|d| d.insert_temp(query_id, query.clone()));
        ui.add_space(4.0);
        let q = query.trim().to_lowercase();
        if q.is_empty() && ui.selectable_label(value.is_empty(), follow).clicked() {
            value.clear();
        }
        let all = families();
        let mut any = false;
        for f in all.iter().filter(|f| q.is_empty() || f.to_lowercase().contains(&q)) {
            any = true;
            if ui.selectable_label(value == f, f).clicked() {
                *value = f.clone();
            }
        }
        if all.is_empty() {
            ui.weak("No fonts found. Install fontconfig to list them.");
        } else if !any {
            ui.weak("No font by that name.");
        }
    }).response.accessible_name("Font");
    let changed = *value != before;
    if changed {
        ui.data_mut(|d| d.remove::<String>(query_id));
    }
    changed
}

/// What an empty choice is called where it follows Appearance's font.
pub fn like_appearance(base: &str) -> String {
    format!("Like Appearance ({})", if base.is_empty() { desktop_name() } else { base.to_string() })
}
