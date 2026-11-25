//! The finished picture: the kept part of the original with the marks drawn into it, as a
//! picture in memory. Drawn in memory by the software renderer, at one pixel to a point, with
//! the same code that draws the window.

use eframe::egui::{self, Pos2, Rect, Vec2};
use rivet_look::live::{Choice, LiveLook};

use crate::doc::Doc;
use crate::paint::{self, Patches, View};

/// Draw `doc` over `base` and return the result, the size of its crop.
pub fn render(base: &egui::ColorImage, doc: &Doc, choice: &Choice) -> egui::ColorImage {
    let area = doc.crop_px();
    let mut out = rivet_kit::scale::crop(base, area);
    if doc.marks.is_empty() {
        return out;
    }
    let [w, h] = out.size;
    // A context of its own, so the window's zoom, fonts in use and frame are left alone. It
    // takes the window's font, so text looks as it did on screen.
    let ctx = egui::Context::default();
    LiveLook::new().frame(&ctx, choice, false);
    let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(w as f32, h as f32));
    let mut raw = egui::RawInput { screen_rect: Some(screen), max_texture_side: Some(16384), ..Default::default() };
    if let Some(info) = raw.viewports.get_mut(&egui::ViewportId::ROOT) {
        info.native_pixels_per_point = Some(1.0);
    }
    let view = View { origin: Pos2::ZERO - doc.crop.min.to_vec2(), zoom: 1.0 };
    let mut patches = Patches::default();
    let mut frame = |raw: egui::RawInput| {
        ctx.run_ui(raw, |ui| {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("rivet-snip-marks"))).with_clip_rect(screen);
            for m in &doc.marks {
                paint::mark(&painter, &view, m, base, &mut patches);
            }
        })
    };
    let mut textures = rivet_kit::soft::Textures::default();
    // The first frame loads the font; the second is drawn with it. Each frame's textures are
    // taken up in full here (epaint panics when an update is dropped unread).
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let mut out_frame = frame(raw.clone());
        let mut delta = std::mem::take(&mut out_frame.textures_delta);
        textures.update(&delta);
        delta.clear();
        shapes = out_frame.shapes;
    }
    let prims = ctx.tessellate(shapes, 1.0);
    let mut buf: Vec<u8> = out.pixels.iter().flat_map(|p| p.to_array()).collect();
    rivet_kit::soft::draw(&mut buf, w, h, rivet_kit::soft::Order::Rgba, &prims, &textures, 1.0);
    out.pixels = buf.as_chunks::<4>().0.iter().map(|p| egui::Color32::from_rgba_premultiplied(p[0], p[1], p[2], p[3])).collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Kind, Mark};
    use eframe::egui::Color32;

    fn choice() -> Choice {
        Choice { mode: rivet_look::theme::Mode::Dark, accent: [0xe6, 0x61, 0x00], follow_desktop_colors: false, text_size: 0.0 }
    }

    fn base() -> egui::ColorImage {
        let (w, h) = (120, 80);
        egui::ColorImage::new([w, h], (0..w * h).map(|i| Color32::from_rgb((i % w) as u8, (i / w) as u8, 90)).collect())
    }

    #[test]
    fn a_crop_alone_leaves_every_pixel_as_it_was() {
        let base = base();
        let mut doc = Doc::new(base.size);
        assert_eq!(render(&base, &doc, &choice()), base);
        assert!(doc.set_crop(Rect::from_min_max(Pos2::new(10.0, 20.0), Pos2::new(50.0, 60.0))));
        let out = render(&base, &doc, &choice());
        assert_eq!(out.size, [40, 40]);
        assert_eq!(out.pixels[0], base.pixels[20 * 120 + 10]);
    }

    #[test]
    fn marks_are_drawn_where_they_were_put_and_nowhere_else() {
        let base = base();
        let mut doc = Doc::new(base.size);
        assert!(doc.set_crop(Rect::from_min_max(Pos2::new(20.0, 10.0), Pos2::new(120.0, 80.0))));
        let mut line = Mark::new(Kind::Line, Pos2::new(30.0, 40.0), Color32::from_rgb(255, 0, 0), 6.0);
        line.b = Pos2::new(100.0, 40.0);
        let mut blur = Mark::new(Kind::Blur, Pos2::new(80.0, 50.0), Color32::RED, 4.0);
        blur.b = Pos2::new(112.0, 74.0);
        let mut step = Mark::new(Kind::Step, Pos2::new(40.0, 62.0), Color32::from_rgb(0, 0, 255), 4.0);
        step.n = 1;
        doc.marks = vec![line, blur, step];
        let out = render(&base, &doc, &choice());
        let at = |x: usize, y: usize| out.pixels[(y - 10) * 100 + (x - 20)];
        // On the line; the crop has moved it 20 left and 10 up in the file.
        assert_eq!(at(60, 40), Color32::from_rgb(255, 0, 0));
        // Away from every mark: the picture, untouched.
        assert_eq!(at(25, 15), base.pixels[15 * 120 + 25]);
        // In the blurred patch: one colour across a square, where the picture had a slope.
        assert_eq!(at(82, 52), at(86, 55));
        assert_ne!(base.pixels[52 * 120 + 82], base.pixels[55 * 120 + 86]);
        // The step's circle, and the number drawn on it in white.
        assert_eq!(at(31, 62), Color32::from_rgb(0, 0, 255));
        let white = (56..68).flat_map(|y| (34..46).map(move |x| (x, y))).filter(|&(x, y)| at(x, y).r() > 200 && at(x, y).g() > 200).count();
        assert!(white > 4, "the step's number was not drawn ({white} light pixels)");
    }
}
