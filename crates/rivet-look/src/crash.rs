//! The "closed unexpectedly" window every app shows once after a crash: the report as it was
//! saved (with personal details already taken out, see `rivet_xdg::crash`), and ways to share
//! it. Nothing is sent: Report on GitHub opens the app's issue form, filled in, in the browser.

use eframe::egui::{self, Id};
use rivet_xdg::crash::{self, App, Report};

pub struct Notice {
    pub app: App,
    pub report: Report,
    copied: bool,
    /// The report was too long for the link and was copied to paste in instead.
    cut: bool,
}

impl Notice {
    /// The newest report not shown yet, marked as shown.
    pub fn unseen(app: App) -> Option<Self> {
        let report = crash::unseen(&app)?;
        crash::mark_seen(&app, &report);
        Some(Self::new(app, report))
    }

    /// The newest report, shown or not (for screenshots and a "show the last report" button).
    pub fn latest(app: App) -> Option<Self> {
        let report = crash::reports(&app).first().and_then(|p| Report::load(p))?;
        Some(Self::new(app, report))
    }

    pub fn new(app: App, report: Report) -> Self {
        Self { app, report, copied: false, cut: false }
    }

    /// Draws the window; true when it's been closed.
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        let mut close = false;
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(22)).corner_radius(crate::theme::r_lg());
        let width = (ctx.content_rect().width() - 80.0).clamp(380.0, 620.0);
        let r = egui::Modal::new(Id::unique("rivet-crash-report")).frame(frame).show(ctx, |ui| {
            ui.set_width(width);
            let v = ui.visuals().clone();
            let name = self.app.name;
            let title = if self.report.closed { format!("{name} closed unexpectedly") } else { format!("{name} ran into a problem") };
            ui.label(egui::RichText::new(title).size(17.0).strong());
            ui.add_space(8.0);
            ui.add(egui::Label::new("A report was saved on this computer. Nothing has been sent anywhere.").wrap());
            ui.add_space(2.0);
            ui.add(egui::Label::new(egui::RichText::new("Names, file paths and addresses are left out, but look it over before you share it.").color(v.weak_text_color())).wrap());
            ui.add_space(12.0);
            // The report itself, selectable.
            egui::Frame::NONE.fill(v.faint_bg_color).corner_radius(crate::theme::r_md()).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::both().max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(&self.report.text).monospace().size(11.5)).selectable(true).extend());
                });
            });
            if self.cut {
                ui.add_space(8.0);
                ui.add(egui::Label::new(egui::RichText::new("The report was too long for the link, so it's copied. Paste it into the issue.").color(v.weak_text_color())).wrap());
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.button(if self.copied { "Copied" } else { "Copy Report" }).clicked() {
                    ui.ctx().copy_text(self.report.text.clone());
                    self.copied = true;
                }
                if ui.button("Open Folder").clicked() {
                    let _ = rivet_xdg::launch::open_url(&crash::dir(&self.app).to_string_lossy());
                }
                if ui.button("Report on GitHub").on_hover_text("Opens a new issue with this report filled in, for you to check and send").clicked() {
                    let (url, full) = self.report.issue_url(&self.app);
                    if !full {
                        ui.ctx().copy_text(self.report.text.clone());
                        self.copied = true;
                        self.cut = true;
                    }
                    let _ = rivet_xdg::launch::open_url(&url);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });
        });
        close || r.should_close()
    }
}
