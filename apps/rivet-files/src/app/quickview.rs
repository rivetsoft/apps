//! Preview: the full-window light box for pictures, videos, music covers and text, opened from
//! the right-click menu (or Space). The light box itself is shared with the other Rivet apps
//! (`rivet_media::lightbox`); this file feeds it the folder's files and acts on what it reports.

use eframe::egui;
use std::path::PathBuf;

use rivet_media::lightbox::{self, Event, Item, LightBox, Look, Opener};

use super::{Action, RivetApp};
use crate::model::Entry;

pub(super) type QuickView = LightBox;

impl RivetApp {
    /// Open Preview on `path`, stepping through the previewable files of the current view.
    pub(super) fn open_quick_view(&mut self, _ctx: &egui::Context, path: PathBuf) {
        let t = self.tab();
        let items: Vec<Item> = t
            .visible
            .iter()
            .map(|&i| &t.entries[i])
            .filter(|e| Self::quick_viewable(e))
            .map(|e| Item { path: e.path.clone(), name: e.name.clone(), mime: e.mime.clone(), size: e.size, ready: true })
            .collect();
        let Some(idx) = items.iter().position(|e| e.path == path) else { return };
        self.quick = Some(LightBox::new(items, idx));
        self.peek.stop();
    }

    /// Whether this file can be shown in Preview.
    pub(super) fn quick_viewable(e: &Entry) -> bool {
        !e.is_dir && lightbox::viewable(&e.mime)
    }

    /// Draw Preview (if open) over everything and handle its keys.
    pub(super) fn quick_view(&mut self, ctx: &egui::Context) {
        // The default app for the current file (name + icon) for the Open button.
        let Some(cur) = self.quick.as_ref().map(|q| q.current().clone()) else { return };
        let app = crate::launch::default_app(&self.apps, &cur.mime).map(|a| (a.name.clone(), a.icon.clone()));
        let app_icon = match &app {
            Some((_, icon)) if icon.starts_with('/') => self.icons.texture_from_path(ctx, std::path::Path::new(icon), (18.0 * ctx.pixels_per_point()) as u32),
            Some((_, icon)) => self.icon(ctx, &[icon.as_str(), "application-x-executable"], 18.0),
            None => None,
        };
        let label = match &app {
            Some((name, _)) => format!("Open with {name}"),
            None => "Open".to_string(),
        };
        // The same theme colours as Peek's code preview; recoloured when the theme or the
        // desktop palette changes while the light box is open.
        let v = ctx.global_style().visuals.clone();
        let (p, s, t) = self.theme_trio();
        let code_bg = crate::palette::mix(v.panel_fill, v.extreme_bg_color, 0.55);
        let look = Look { accent: super::accent_color(&self.s), code_colors: crate::highlight::colors(v.text_color(), code_bg, p, s, t), code_bg };
        let opener = Opener { label, icon: app_icon, choose: true };
        let Some(q) = &mut self.quick else { return };
        let events = q.show(ctx, &look, &opener);
        let path = q.current().path.clone();
        for ev in events {
            match ev {
                Event::Moved(_) => {
                    // The file view follows along, so closing leaves the last one selected.
                    let t = self.tab_mut();
                    t.selected.clear();
                    t.selected.insert(path.clone());
                    t.scroll_to = Some(path.clone());
                }
                Event::Open => self.pending.push(Action::Open(path.clone())),
                // The Open With window opens over the folder, so Preview steps aside.
                Event::ChooseApp => self.pending.push(Action::OpenWithDialog(vec![path.clone()])),
                Event::Close => self.quick = None,
            }
        }
    }
}
