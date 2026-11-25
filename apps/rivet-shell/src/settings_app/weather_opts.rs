//! The weather's options, the same on the Widgets page and in the panel item's card: the place
//! (looked up by name) and the units.

use std::sync::Mutex;

use eframe::egui;

use super::controls;
use crate::config::Settings;
use crate::weather::{Place, Units};

/// The place search: what's typed, and what the last look-up found.
#[derive(Default)]
struct Search {
    query: String,
    busy: bool,
    found: Option<Result<Vec<Place>, String>>,
    /// Counts look-ups, so a slow answer to an old one is dropped.
    round: u64,
}

static SEARCH: Mutex<Option<Search>> = Mutex::new(None);

/// Checks: RIVET_SHELL_WEATHER_FIND=name shows the search with places found for that name
/// (none: with nothing found).
pub(super) fn preload() {
    let Ok(q) = std::env::var("RIVET_SHELL_WEATHER_FIND") else { return };
    let place = |name: &str, lat, lon| Place { name: name.into(), lat, lon };
    let found = if q == "none" { Vec::new() } else { vec![place("Portland, Oregon, United States", 45.52, -122.68), place("Portland, Maine, United States", 43.66, -70.26), place("Portland, Victoria, Australia", -38.34, 141.6)] };
    if let Ok(mut s) = SEARCH.lock() {
        *s = Some(Search { query: if q == "none" { "Prtland".into() } else { q }, found: Some(Ok(found)), ..Default::default() });
    }
}

fn look_up(ctx: &egui::Context) {
    let Ok(mut guard) = SEARCH.lock() else { return };
    let s = guard.get_or_insert_with(Search::default);
    if s.query.trim().is_empty() {
        return;
    }
    s.round += 1;
    (s.busy, s.found) = (true, None);
    let (query, round, ctx) = (s.query.clone(), s.round, ctx.clone());
    std::thread::spawn(move || {
        let found = crate::weather::find(&query);
        if let Ok(mut guard) = SEARCH.lock()
            && let Some(s) = guard.as_mut().filter(|s| s.round == round)
        {
            (s.busy, s.found) = (false, Some(found));
        }
        ctx.request_repaint();
    });
}

/// The place and the units, `width` wide.
pub(super) fn options(ui: &mut egui::Ui, s: &mut Settings, width: f32) {
    let weak = ui.visuals().weak_text_color();
    ui.label(egui::RichText::new("Place").size(13.0));
    if s.weather_at.is_some() {
        ui.add(egui::Label::new(egui::RichText::new(&s.weather_place).size(13.0).strong()).wrap());
    } else {
        ui.label(egui::RichText::new("No place chosen yet").size(13.0).color(weak));
    }
    ui.add_space(4.0);
    let mut go = false;
    let mut chosen = None;
    {
        let Ok(mut guard) = SEARCH.lock() else { return };
        let search = guard.get_or_insert_with(Search::default);
        ui.horizontal(|ui| {
            let field = ui.add(egui::TextEdit::singleline(&mut search.query).hint_text("City or town").desired_width((width - 84.0).max(80.0)));
            go = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            go |= controls::button_if(ui, !search.busy && !search.query.trim().is_empty(), controls::Icon::Forward, "Find").clicked();
        });
        if search.busy {
            ui.label(egui::RichText::new("Looking…").size(12.0).color(weak));
        }
        match &search.found {
            Some(Ok(list)) if list.is_empty() => {
                ui.add(egui::Label::new(egui::RichText::new("No place by that name. Check the spelling, or try a larger town nearby.").size(12.0).color(weak)).wrap());
            }
            Some(Ok(list)) => {
                for place in list {
                    if ui.add_sized([width, 26.0], egui::Button::new(egui::RichText::new(&place.name).size(12.5)).right_text("")).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        chosen = Some(place.clone());
                    }
                }
            }
            Some(Err(why)) => {
                ui.add(egui::Label::new(egui::RichText::new(why).size(12.0).color(rivet_look::theme::status(ui.ctx()).danger)).wrap());
            }
            None => {}
        }
        if chosen.is_some() {
            *search = Search { round: search.round, ..Default::default() };
        }
    }
    if let Some(place) = chosen {
        (s.weather_place, s.weather_at) = (place.name, Some((place.lat, place.lon)));
    }
    if go {
        look_up(ui.ctx());
    }
    ui.add_space(10.0);
    ui.label(egui::RichText::new("Units").size(13.0));
    super::panel_editor::segmented(ui, &mut s.weather_units, &[(Units::Metric, "°C, km/h"), (Units::Imperial, "°F, mph")]);
    ui.add_space(8.0);
    ui.add(egui::Label::new(egui::RichText::new("Open-Meteo is asked for the weather at this place every 15 minutes while the widget or the panel item is on.").size(12.0).color(weak)).wrap());
}
