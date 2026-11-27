//! The picture: mpv draws each frame into a texture of ours (an OpenGL framebuffer), and egui
//! shows that texture like any other image, so the controls draw over it in the Rivet look.

use eframe::egui;
use eframe::glow::{self, HasContext};
use rivet_media::mpv::{Display, Player, ProcAddress, Render};

#[derive(Default)]
pub struct Screen {
    render: Option<Render>,
    fbo: Option<glow::Framebuffer>,
    tex: Option<glow::Texture>,
    id: Option<egui::TextureId>,
    size: [i32; 2],
    /// Why the picture can't be drawn, once that is known.
    pub failed: Option<String>,
}

impl Screen {
    /// Draw mpv's newest frame at `px` pixels (when there is a new one, or the size changed) and
    /// return the texture holding it. Call from the window's frame, where its OpenGL context is
    /// current.
    pub fn frame(&mut self, frame: &mut eframe::Frame, player: &Player, proc: Option<&ProcAddress>, display: Display, ctx: &egui::Context, px: [i32; 2]) -> Option<egui::TextureId> {
        if self.failed.is_some() || px[0] < 2 || px[1] < 2 {
            return self.id;
        }
        let gl = frame.gl()?.clone();
        if self.render.is_none() {
            let Some(proc) = proc.cloned() else {
                self.failed = Some("This window can't show video: it has no OpenGL.".into());
                return None;
            };
            let wake = ctx.clone();
            match unsafe { Render::new(player, proc, display, move || wake.request_repaint()) } {
                Ok(r) => self.render = Some(r),
                Err(e) => {
                    self.failed = Some(e);
                    return None;
                }
            }
        }
        let resized = self.size != px;
        unsafe {
            if self.tex.is_none() {
                let tex = gl.create_texture().ok()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
                self.tex = Some(tex);
                self.fbo = Some(gl.create_framebuffer().ok()?);
                // egui owns the texture from here (and deletes it); we keep drawing into it.
                self.id = Some(frame.register_native_glow_texture(tex));
            }
            let (tex, fbo) = (self.tex?, self.fbo?);
            if resized {
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, px[0], px[1], 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                gl.bind_texture(glow::TEXTURE_2D, None);
                self.size = px;
            }
            let render = self.render.as_ref()?;
            if render.frame_due() || resized {
                // egui reads a texture's first row as its top, so the frame is drawn flipped.
                if let Err(e) = render.draw(fbo.0.get(), px[0], px[1], true) {
                    eprintln!("rivet-play: {e}");
                }
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                render.swapped();
            }
        }
        self.id
    }

    /// The texture the picture is drawn into, once there is one.
    pub fn id(&self) -> Option<egui::TextureId> {
        self.id
    }

    /// Let go of the renderer and the framebuffer, while the OpenGL context is still there.
    pub fn free(&mut self, gl: Option<&glow::Context>) {
        self.render = None;
        if let (Some(gl), Some(fbo)) = (gl, self.fbo.take()) {
            unsafe { gl.delete_framebuffer(fbo) };
        }
    }
}
