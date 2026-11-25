//! The Properties block above a note's text: the frontmatter as typed rows. Each row has a
//! type icon and name (click it, or right-click, for the type, Rename and Remove) and an editor
//! that fits the type: text, number, checkbox, date with a calendar, list or tag chips, link.

use chrono::Datelike;
use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::theme::{r_md, r_sm, r_xs, status};
use rivet_look::{fonts, palette};

use super::*;
use crate::props::{self, PropType};

const NAME_W: f32 = 150.0;
const ROW_H: f32 = 30.0;

/// Icons the block draws from the icon theme (the rest are drawn).
pub(super) struct PropIcons {
    pub tag: Option<egui::TextureHandle>,
    pub link: Option<egui::TextureHandle>,
}

/// What a row asked for, carried out after the rows are drawn.
enum Act {
    Remove(usize),
    SetType(usize, PropType),
    Rename(usize),
}

/// The type a property is edited as: chosen earlier, else worked out from it.
fn type_of(types: &mut Vec<(String, PropType)>, key: &str, v: &Value) -> PropType {
    if let Some((_, t)) = types.iter().find(|(k, _)| k == key) {
        // A file edit can turn a value into nested YAML.
        if matches!(v, Value::Raw(_)) { PropType::Raw } else { *t }
    } else {
        let t = PropType::infer(key, v);
        types.push((key.to_string(), t));
        t
    }
}

/// The properties block. Returns true when the frontmatter changed.
pub(super) fn block(ui: &mut egui::Ui, d: &mut Doc, icons: &PropIcons) -> bool {
    let v = ui.visuals().clone();
    let weak = v.weak_text_color();
    let mut changed = false;
    let Some(fm) = d.fm.as_mut() else {
        if ui.add(egui::Button::new(egui::RichText::new("+  Add properties").color(weak)).frame_when_inactive(false).corner_radius(r_sm())).on_hover_text("Start a frontmatter block for this note").clicked() {
            let mut fm = Frontmatter::default();
            fm.set("tags", Value::List(Vec::new()));
            d.fm = Some(fm);
            d.new_prop = Some(String::new());
            changed = true;
        }
        ui.add_space(18.0);
        return changed;
    };
    let mut acts: Vec<Act> = Vec::new();
    egui::Frame::NONE.fill(v.faint_bg_color).corner_radius(r_md()).inner_margin(egui::Margin { left: 10, right: 10, top: 6, bottom: 6 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 2.0;
        // ---- header: fold, title, count
        let (hr, hresp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
        if hresp.hovered() {
            ui.painter().rect_filled(hr, r_sm(), v.widgets.hovered.weak_bg_fill.gamma_multiply(0.5));
        }
        let c = Pos2::new(hr.left() + 10.0, hr.center().y);
        let tri = if d.props_open {
            vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)]
        } else {
            vec![c + Vec2::new(-2.0, -4.0), c + Vec2::new(-2.0, 4.0), c + Vec2::new(3.0, 0.0)]
        };
        ui.painter().add(egui::Shape::convex_polygon(tri, weak, Stroke::NONE));
        let title = ui.painter().layout_no_wrap("Properties".into(), fonts::ui_font(13.0), v.strong_text_color());
        let tw = title.size().x;
        ui.painter().galley(Pos2::new(hr.left() + 22.0, hr.center().y - title.size().y / 2.0), title, v.strong_text_color());
        ui.painter().text(Pos2::new(hr.left() + 30.0 + tw, hr.center().y), Align2::LEFT_CENTER, fm.fields.iter().filter(|(k, _)| !props::hidden(k)).count().to_string(), fonts::ui_font(12.0), weak);
        if hresp.on_hover_text(if d.props_open { "Fold the properties" } else { "Show the properties" }).clicked() {
            d.props_open = !d.props_open;
        }
        if !d.props_open {
            return;
        }
        ui.add_space(2.0);
        // ---- the rows
        for i in 0..fm.fields.len() {
            let key = fm.fields[i].0.clone();
            if props::hidden(&key) {
                continue;
            }
            let ty = type_of(&mut d.types, &key, &fm.fields[i].1);
            let row_id = egui::Id::unique(("prop-row", i));
            let was_hovered = ui.ctx().data(|m| m.get_temp::<bool>(row_id)).unwrap_or(false);
            let bg = ui.painter().add(egui::Shape::Noop);
            let row = ui.horizontal(|ui| {
                ui.set_min_height(ROW_H);
                ui.spacing_mut().item_spacing.x = 6.0;
                // Name: the type's icon and the name; click for its menu.
                if let Some((ri, buf)) = d.renaming.as_mut()
                    && *ri == i
                {
                    let r = ui.add_sized([NAME_W, 24.0], egui::TextEdit::singleline(buf).font(fonts::ui_font(13.0)));
                    if !r.has_focus() && !r.lost_focus() {
                        r.request_focus();
                    }
                    if r.lost_focus() {
                        if ui.input(|i| i.key_pressed(Key::Escape)) {
                            d.renaming = None;
                        } else {
                            acts.push(Act::Rename(i));
                        }
                    }
                } else {
                    let (nr, nresp) = ui.allocate_exact_size(Vec2::new(NAME_W, 26.0), Sense::click());
                    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&nresp));
                    if nresp.hovered() || open {
                        ui.painter().rect_filled(nr, r_sm(), v.widgets.hovered.weak_bg_fill);
                    }
                    let gr = Rect::from_center_size(Pos2::new(nr.left() + 13.0, nr.center().y), Vec2::splat(14.0));
                    glyph(ui.painter(), gr, ty, weak, icons);
                    let text_r = Rect::from_min_max(Pos2::new(nr.left() + 28.0, nr.top()), Pos2::new(nr.right() - 4.0, nr.bottom()));
                    ui.painter().with_clip_rect(text_r).text(text_r.left_center(), Align2::LEFT_CENTER, &key, fonts::ui_font(13.0), palette::mix(weak, v.text_color(), 0.45));
                    let nresp = nresp.on_hover_text(format!("{key} · {}\nClick to change its type, rename or remove it", ty.label()));
                    let mut menu = |ui: &mut egui::Ui, acts: &mut Vec<Act>| {
                        ui.set_min_width(180.0);
                        ui.label(egui::RichText::new("Type").small().color(weak));
                        if ty == PropType::Raw {
                            ui.label(egui::RichText::new("Nested YAML: edit it in the file").color(weak));
                        } else {
                            for t in PropType::CHOICES {
                                if type_item(ui, t, t == ty, icons).clicked() && t != ty {
                                    acts.push(Act::SetType(i, t));
                                    ui.close();
                                }
                            }
                        }
                        ui.separator();
                        if ui.button("Rename…").clicked() {
                            d.renaming = Some((i, key.clone()));
                            ui.close();
                        }
                        if ui.button(egui::RichText::new("Remove").color(status(ui.ctx()).danger)).clicked() {
                            acts.push(Act::Remove(i));
                            ui.close();
                        }
                    };
                    // One menu for a click or a right-click.
                    let cmd = (nresp.clicked() || nresp.secondary_clicked()).then_some(egui::SetOpenCommand::Toggle);
                    egui::Popup::menu(&nresp).open_memory(cmd).show(|ui| menu(ui, &mut acts));
                }
                // The value, up to the remove button.
                let value_w = (ui.available_width() - 30.0).max(80.0);
                ui.allocate_ui_with_layout(Vec2::new(value_w, 26.0), egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true), |ui| {
                    ui.set_width(value_w);
                    if value_ui(ui, &key, ty, &mut fm.fields[i].1, &mut d.item_input, icons) {
                        changed = true;
                    }
                });
                // Remove: shown while the row is pointed at.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let b = egui::Button::new(egui::RichText::new("×").size(15.0).color(weak)).frame_when_inactive(false).min_size(Vec2::splat(24.0)).corner_radius(r_sm());
                    if ui.add_visible(was_hovered, b).on_hover_text("Remove this property").clicked() {
                        acts.push(Act::Remove(i));
                    }
                });
            });
            let rr = row.response.rect.expand2(Vec2::new(4.0, 0.0));
            let hovered = ui.rect_contains_pointer(rr);
            ui.ctx().data_mut(|m| m.insert_temp(row_id, hovered));
            if hovered {
                ui.painter().set(bg, egui::Shape::rect_filled(rr, r_sm(), v.extreme_bg_color.gamma_multiply(0.55)));
            }
        }
        // ---- a new property: its type and name
        ui.add_space(2.0);
        if let Some(mut name) = d.new_prop.take() {
            let mut keep = true;
            ui.horizontal(|ui| {
                ui.set_min_height(ROW_H);
                ui.spacing_mut().item_spacing.x = 6.0;
                let t = d.new_type;
                let (tr, tresp) = ui.allocate_exact_size(Vec2::new(26.0, 26.0), Sense::click());
                ui.painter().rect_filled(tr, r_sm(), if tresp.hovered() { v.widgets.hovered.weak_bg_fill } else { v.extreme_bg_color });
                glyph(ui.painter(), Rect::from_center_size(tr.center(), Vec2::splat(14.0)), t, v.text_color(), icons);
                let tresp = tresp.on_hover_text(format!("Type: {} (click to change)", t.label()));
                egui::Popup::menu(&tresp).show(|ui| {
                    ui.set_min_width(160.0);
                    for c in PropType::CHOICES {
                        if type_item(ui, c, c == t, icons).clicked() {
                            d.new_type = c;
                            ui.close();
                        }
                    }
                });
                let r = ui.add(egui::TextEdit::singleline(&mut name).hint_text("Property name").font(fonts::ui_font(13.0)).desired_width(NAME_W + 40.0));
                let menu_open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&tresp));
                if !r.has_focus() && !r.lost_focus() && !menu_open {
                    r.request_focus();
                }
                // Typing a known name picks its usual type.
                if r.changed() {
                    let n = name.trim().to_ascii_lowercase();
                    if n == "tags" || n == "tag" {
                        d.new_type = PropType::Tags;
                    } else if ["created", "updated", "modified", "date", "due"].contains(&n.as_str()) {
                        d.new_type = PropType::Date;
                    }
                }
                let enter = ui.input(|i| i.key_pressed(Key::Enter));
                let n = name.trim().replace(':', "");
                let ok = !n.is_empty() && fm.get(&n).is_none() && !props::hidden(&n);
                if ui.add_enabled(ok, egui::Button::new("Add").corner_radius(r_sm())).clicked() || (r.lost_focus() && enter && ok) {
                    fm.fields.push((n.clone(), d.new_type.empty()));
                    d.types.retain(|(k, _)| *k != n);
                    d.types.push((n, d.new_type));
                    changed = true;
                    keep = false;
                } else if (r.lost_focus() && !enter && !menu_open && name.trim().is_empty()) || ui.input(|i| i.key_pressed(Key::Escape)) {
                    keep = false;
                }
                if ui.add(egui::Button::new(egui::RichText::new("Cancel").color(weak)).frame_when_inactive(false).corner_radius(r_sm())).clicked() {
                    keep = false;
                }
            });
            if keep {
                d.new_prop = Some(name);
            }
        } else if ui.add(egui::Button::new(egui::RichText::new("+  Add property").size(12.5).color(weak)).frame_when_inactive(false).corner_radius(r_sm())).clicked() {
            d.new_prop = Some(String::new());
            d.new_type = PropType::Text;
        }
    });
    // ---- carry out the rows' choices
    for a in acts {
        let Some(fm) = d.fm.as_mut() else { break };
        match a {
            Act::Remove(i) if i < fm.fields.len() => {
                let (k, _) = fm.fields.remove(i);
                d.types.retain(|(n, _)| *n != k);
                changed = true;
            }
            Act::SetType(i, t) if i < fm.fields.len() => {
                let (k, val) = &mut fm.fields[i];
                *val = t.convert(val);
                let k = k.clone();
                d.types.retain(|(n, _)| *n != k);
                d.types.push((k, t));
                changed = true;
            }
            Act::Rename(i) if i < fm.fields.len() => {
                if let Some((_, buf)) = d.renaming.take() {
                    let n = buf.trim().replace(':', "");
                    let old = fm.fields[i].0.clone();
                    if !n.is_empty() && n != old && fm.get(&n).is_none() {
                        fm.fields[i].0 = n.clone();
                        for (k, _) in &mut d.types {
                            if *k == old {
                                *k = n.clone();
                            }
                        }
                        changed = true;
                    }
                }
            }
            _ => {}
        }
    }
    ui.add_space(24.0);
    changed
}

/// One type in a menu: its icon and name, ticked when it's the current one.
fn type_item(ui: &mut egui::Ui, t: PropType, on: bool, icons: &PropIcons) -> egui::Response {
    let r = ui.add(egui::Button::selectable(on, format!("        {}", t.label())).min_size(Vec2::new(0.0, 24.0)));
    let g = Rect::from_center_size(Pos2::new(r.rect.left() + 16.0, r.rect.center().y), Vec2::splat(13.0));
    let col = if on { ui.visuals().selection.stroke.color } else { ui.visuals().text_color() };
    glyph(ui.painter(), g, t, col, icons);
    r
}

/// The editor for one value. Returns true when it changed.
fn value_ui(ui: &mut egui::Ui, key: &str, ty: PropType, val: &mut Value, inputs: &mut std::collections::HashMap<String, String>, icons: &PropIcons) -> bool {
    let v = ui.visuals().clone();
    let weak = v.weak_text_color();
    let danger = status(ui.ctx()).danger;
    let font = fonts::ui_font(13.0);
    let mut changed = false;
    let field = |text, hint, w, color| field(text, hint, w, color, font.clone(), weak);
    match (ty, &mut *val) {
        (PropType::Raw, Value::Raw(r)) => {
            ui.add(egui::Label::new(egui::RichText::new(r.trim()).monospace().size(12.0).color(weak)).wrap()).on_hover_text("Nested YAML, kept as written. Edit it in the file's text.");
        }
        (PropType::Checkbox, Value::Text(t)) => {
            let mut b = props::parse_bool(t).unwrap_or(false);
            if ui.checkbox(&mut b, "").changed() {
                *t = if b { "true" } else { "false" }.into();
                changed = true;
            }
        }
        (PropType::Number, Value::Text(t)) => {
            let bad = !t.trim().is_empty() && t.trim().parse::<f64>().is_err();
            let r = ui.add(field(t, "0", 120.0, bad.then_some(danger)));
            if bad {
                r.clone().on_hover_text("Not a number");
            }
            if r.changed() {
                t.retain(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'));
                changed = true;
            }
        }
        (PropType::Date, Value::Text(t)) => {
            let bad = !t.trim().is_empty() && props::parse_date(t).is_none();
            let r = ui.add(field(t, "YYYY-MM-DD", 150.0, bad.then_some(danger)));
            if r.changed() {
                changed = true;
            }
            if bad {
                r.on_hover_text("Dates are written YYYY-MM-DD");
            }
            let (cr, cresp) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
            if cresp.hovered() {
                ui.painter().rect_filled(cr, r_sm(), v.widgets.hovered.weak_bg_fill);
            }
            glyph(ui.painter(), Rect::from_center_size(cr.center(), Vec2::splat(13.0)), PropType::Date, if cresp.hovered() { v.text_color() } else { weak }, icons);
            let cresp = cresp.on_hover_text("Pick a date");
            egui::Popup::menu(&cresp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                if let Some(day) = calendar(ui, t) {
                    *t = props::with_date(t, day);
                    changed = true;
                    ui.close();
                }
            });
            if let Some(day) = props::parse_date(t) {
                ui.label(egui::RichText::new(day.format("%a").to_string()).size(12.0).color(weak));
            }
        }
        (PropType::Link, Value::Text(t)) => {
            let w = (ui.available_width() - 32.0).max(60.0);
            let col = if props::is_url(t) { Some(v.hyperlink_color) } else { None };
            if ui.add(field(t, "https://", w, col)).changed() {
                changed = true;
            }
            let ok = props::is_url(t);
            let b = match &icons.link {
                Some(tex) => egui::Button::image(egui::Image::from_texture(tex).fit_to_exact_size(Vec2::splat(13.0)).tint(if ok { v.text_color() } else { weak })),
                None => egui::Button::new("↗"),
            };
            if ui.add_enabled(ok, b.frame_when_inactive(false).min_size(Vec2::splat(24.0)).corner_radius(r_sm())).on_hover_text("Open the link").clicked() {
                let _ = rivet_xdg::launch::open_url(t.trim());
            }
        }
        (PropType::List | PropType::Tags, Value::List(items)) => {
            let tags = ty == PropType::Tags;
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            let mut drop: Option<usize> = None;
            for (j, item) in items.iter().enumerate() {
                let (fill, fg) = if tags { (palette::mix(v.faint_bg_color, v.selection.bg_fill, 0.30), v.text_color()) } else { (palette::mix(v.faint_bg_color, v.text_color(), 0.09), v.text_color()) };
                if chip(ui, &if tags { format!("#{item}") } else { item.clone() }, fill, fg, weak) {
                    drop = Some(j);
                }
            }
            let input = inputs.entry(key.to_string()).or_default();
            let hint = if tags { "add tag" } else { "add item" };
            let r = ui.add(field(input, hint, 90.0, None));
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            // Backspace in an empty box takes the last one off.
            if r.has_focus() && input.is_empty() && ui.input(|i| i.key_pressed(Key::Backspace)) && !items.is_empty() {
                drop = Some(items.len() - 1);
            }
            let typed_sep = r.changed() && (input.ends_with(',') || (tags && input.ends_with(' ')));
            if enter || typed_sep {
                let seps: &[char] = if tags { &[',', ' '] } else { &[','] };
                for t in input.split(seps).map(|t| t.trim().trim_start_matches(if tags { "#" } else { "" })).filter(|t| !t.is_empty()) {
                    if !items.iter().any(|x| x.eq_ignore_ascii_case(t)) {
                        items.push(t.to_string());
                    }
                }
                input.clear();
                changed = true;
                r.request_focus();
            }
            if let Some(j) = drop {
                items.remove(j);
                changed = true;
            }
        }
        // Text, and anything whose value doesn't fit its type yet.
        (_, Value::Text(t)) => {
            let w = ui.available_width();
            if ui.add(field(t, "Empty", w, None)).changed() {
                changed = true;
            }
        }
        (_, other) => {
            ui.label(egui::RichText::new(other.as_text()).color(weak));
        }
    }
    changed
}

/// A quiet text box: plain text until it's pointed at or typed in.
fn field<'t>(text: &'t mut String, hint: &str, w: f32, color: Option<Color32>, font: egui::FontId, weak: Color32) -> egui::TextEdit<'t> {
    egui::TextEdit::singleline(text)
        .hint_text(egui::RichText::new(hint).color(weak.gamma_multiply(0.7)))
        .font(font)
        .background_color(Color32::TRANSPARENT)
        .desired_width(w)
        .margin(egui::Margin::symmetric(6, 3))
        .text_color_opt(color)
}

/// A chip with a small × at its end. Returns true when the × was clicked.
fn chip(ui: &mut egui::Ui, text: &str, fill: Color32, fg: Color32, weak: Color32) -> bool {
    let g = ui.painter().layout_no_wrap(text.to_string(), fonts::ui_font(12.5), fg);
    let size = Vec2::new(g.size().x + 30.0, 22.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(r, r_xs(), fill);
    ui.painter().galley(Pos2::new(r.left() + 8.0, r.center().y - g.size().y / 2.0), g, fg);
    let xr = Rect::from_center_size(Pos2::new(r.right() - 11.0, r.center().y), Vec2::splat(16.0));
    let x = ui.interact(xr, resp.id.with("x"), Sense::click());
    if x.hovered() {
        ui.painter().rect_filled(xr, r_xs(), fill.gamma_multiply(1.6));
    }
    ui.painter().text(xr.center(), Align2::CENTER_CENTER, "×", fonts::ui_font(13.0), if x.hovered() || resp.hovered() { fg } else { weak });
    x.on_hover_text("Remove").clicked()
}

/// A month to pick a day from. Returns the day clicked.
pub(super) fn calendar(ui: &mut egui::Ui, value: &str) -> Option<chrono::NaiveDate> {
    let v = ui.visuals().clone();
    let weak = v.weak_text_color();
    let today = chrono::Local::now().date_naive();
    let picked = props::parse_date(value);
    let id = egui::Id::unique("prop-calendar-month");
    let mut month = ui.ctx().data(|m| m.get_temp::<chrono::NaiveDate>(id)).or(picked).unwrap_or(today);
    month = month.with_day(1).unwrap_or(month);
    let mut out = None;
    // Month and year between the previous and next buttons.
    let (hr, _) = ui.allocate_exact_size(Vec2::new(7.0 * 32.0, 28.0), Sense::hover());
    ui.painter().text(hr.center(), Align2::CENTER_CENTER, month.format("%B %Y").to_string(), fonts::ui_font(13.5), v.strong_text_color());
    let nav = |ui: &mut egui::Ui, r: Rect, text: &str, tip: &str| ui.put(r, egui::Button::new(egui::RichText::new(text).size(16.0)).frame_when_inactive(false).corner_radius(r_sm())).on_hover_text(tip).clicked();
    if nav(ui, Rect::from_min_size(hr.min, Vec2::splat(28.0)), "‹", "Previous month") {
        month = month.checked_sub_months(chrono::Months::new(1)).unwrap_or(month);
    }
    if nav(ui, Rect::from_min_size(hr.right_top() - Vec2::new(28.0, 0.0), Vec2::splat(28.0)), "›", "Next month") {
        month = month.checked_add_months(chrono::Months::new(1)).unwrap_or(month);
    }
    let cell = Vec2::new(32.0, 28.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for n in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
            let (r, _) = ui.allocate_exact_size(Vec2::new(cell.x, 20.0), Sense::hover());
            ui.painter().text(r.center(), Align2::CENTER_CENTER, n, fonts::ui_font(11.0), weak);
        }
    });
    let start = month - chrono::Days::new(month.weekday().num_days_from_monday() as u64);
    for week in 0..6 {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for k in 0..7 {
                let day = start + chrono::Days::new(week * 7 + k);
                let (r, resp) = ui.allocate_exact_size(cell, Sense::click());
                let inner = r.shrink2(Vec2::new(2.0, 1.0));
                let sel = Some(day) == picked;
                if sel {
                    ui.painter().rect_filled(inner, r_sm(), v.selection.bg_fill);
                } else if resp.hovered() {
                    ui.painter().rect_filled(inner, r_sm(), v.widgets.hovered.weak_bg_fill);
                }
                if day == today && !sel {
                    ui.painter().rect_stroke(inner, r_sm(), Stroke::new(1.0, v.selection.bg_fill), egui::StrokeKind::Inside);
                }
                let fg = if sel { v.selection.stroke.color } else if day.month() == month.month() { v.text_color() } else { weak.gamma_multiply(0.6) };
                ui.painter().text(r.center(), Align2::CENTER_CENTER, day.day().to_string(), fonts::ui_font(12.5), fg);
                if resp.clicked() {
                    out = Some(day);
                }
            }
        });
    }
    ui.add_space(4.0);
    if ui.add(egui::Button::new("Today").corner_radius(r_sm())).clicked() {
        out = Some(today);
    }
    ui.ctx().data_mut(|m| m.insert_temp(id, month));
    if out.is_some() {
        ui.ctx().data_mut(|m| m.remove::<chrono::NaiveDate>(id));
    }
    out
}

/// A small drawn icon for a property type.
fn glyph(p: &egui::Painter, r: Rect, t: PropType, col: Color32, icons: &PropIcons) {
    let s = Stroke::new(1.3, col);
    let tex = |p: &egui::Painter, tex: &Option<egui::TextureHandle>, fallback: &str| match tex {
        Some(tx) => rivet_look::theme::paint_tex(p, tx, r, col),
        None => {
            p.text(r.center(), Align2::CENTER_CENTER, fallback, fonts::ui_font(12.0), col);
        }
    };
    match t {
        PropType::Text => {
            p.text(r.center() + Vec2::new(0.0, 0.5), Align2::CENTER_CENTER, "Aa", fonts::ui_font(10.5), col);
        }
        PropType::Number => {
            p.text(r.center() + Vec2::new(0.0, 0.5), Align2::CENTER_CENTER, "12", fonts::ui_font(10.5), col);
        }
        PropType::Checkbox => {
            let b = r.shrink(1.5);
            p.rect_stroke(b, 2.5, s, egui::StrokeKind::Inside);
            p.line(vec![b.left_center() + Vec2::new(2.5, 0.0), b.center_bottom() + Vec2::new(-1.0, -3.0), b.right_top() + Vec2::new(-2.5, 3.0)], s);
        }
        PropType::Date => {
            let b = Rect::from_min_max(r.min + Vec2::new(1.0, 2.5), r.max - Vec2::new(1.0, 0.5));
            p.rect_stroke(b, 2.0, s, egui::StrokeKind::Inside);
            p.line_segment([Pos2::new(b.left(), b.top() + 3.5), Pos2::new(b.right(), b.top() + 3.5)], s);
            for x in [b.left() + 3.5, b.right() - 3.5] {
                p.line_segment([Pos2::new(x, r.top() + 0.5), Pos2::new(x, b.top() + 1.5)], s);
            }
        }
        PropType::List => {
            for k in 0..3 {
                let y = r.top() + 3.5 + k as f32 * 3.5;
                p.circle_filled(Pos2::new(r.left() + 2.5, y), 1.1, col);
                p.line_segment([Pos2::new(r.left() + 5.5, y), Pos2::new(r.right() - 1.0, y)], s);
            }
        }
        PropType::Tags => tex(p, &icons.tag, "#"),
        PropType::Link => tex(p, &icons.link, "↗"),
        PropType::Raw => {
            p.text(r.center(), Align2::CENTER_CENTER, "{ }", fonts::ui_font(10.5), col);
        }
    }
}
