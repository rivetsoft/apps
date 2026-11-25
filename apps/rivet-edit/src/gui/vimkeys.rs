//! Vim keys in the editor (Preferences > Editor): the keys are taken before the text box sees
//! them and handed to `crate::vim`, except while typing in Insert mode.

use eframe::egui::{self, Key, Modifiers};

use super::*;
use crate::ops::Sel;
use crate::vim::{self, Effect, Mode};

impl EditApp {
    /// Before the text box: Vim's keys for the open file.
    pub(crate) fn vim_keys(&mut self, ctx: &egui::Context, id: egui::Id) {
        if !self.s.vim_keys || !ctx.memory(|m| m.has_focus(id)) {
            return;
        }
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        t.sels.clear();
        let st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        let cur0 = st.cursor.char_range().map(|r| r.primary.index.0).unwrap_or(0);
        let insert = self.vim.mode == Mode::Insert && self.vim.cmdline.is_none();
        // Insert mode: the text box types; only Esc (or Ctrl+[) is Vim's.
        if insert {
            let esc = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape) || i.consume_key(Modifiers::COMMAND, Key::OpenBracket));
            if esc {
                let chars: Vec<char> = t.text.chars().collect();
                let mut cur = cur0.min(chars.len());
                self.vim.insert_done(&chars, &mut cur);
                editing::set_primary(ctx, id, Sel::new(cur, cur));
            }
            return;
        }
        // The rest of the time every key is Vim's, apart from Ctrl and Alt shortcuts.
        let mut keys: Vec<char> = Vec::new();
        ctx.input_mut(|i| {
            let events = std::mem::take(&mut i.events);
            for e in events {
                match &e {
                    egui::Event::Text(s) => keys.extend(s.chars()),
                    egui::Event::Key { key, pressed: true, modifiers, .. } => {
                        let ctrl = modifiers.command || modifiers.ctrl;
                        if ctrl && !modifiers.alt && !modifiers.shift {
                            match key {
                                Key::D => keys.push(vim::CTRL_D),
                                Key::U => keys.push(vim::CTRL_U),
                                Key::R => keys.push(vim::CTRL_R),
                                Key::OpenBracket => keys.push(vim::ESC),
                                _ => i.events.push(e),
                            }
                        } else if modifiers.alt || ctrl {
                            i.events.push(e);
                        } else {
                            match key {
                                Key::Escape => keys.push(vim::ESC),
                                Key::Enter => keys.push(vim::ENTER),
                                Key::Backspace => keys.push(vim::BS),
                                Key::ArrowLeft => keys.push('h'),
                                Key::ArrowRight => keys.push('l'),
                                Key::ArrowUp => keys.push('k'),
                                Key::ArrowDown => keys.push('j'),
                                Key::Home => keys.push(vim::HOME),
                                Key::End => keys.push('$'),
                                Key::PageDown => keys.push(vim::CTRL_F),
                                Key::PageUp => keys.push(vim::CTRL_B),
                                Key::Delete => keys.push('x'),
                                // Letters and the rest arrive as text; F-keys stay the app's.
                                k if k.name().starts_with('F') && k.name().len() > 1 => i.events.push(e),
                                _ => {}
                            }
                        }
                    }
                    // A paste, a copy, the pointer…: as usual.
                    _ => i.events.push(e),
                }
            }
        });
        let mut chars: Vec<char> = t.text.chars().collect();
        let mut cur = cur0.min(chars.len());
        if keys.is_empty() {
            // A click in Normal mode lands on a character, not past a line's end.
            if self.vim.mode == Mode::Normal && self.vim.cmdline.is_none() {
                let c = vim::clamp(&chars, cur);
                let selecting = st.cursor.char_range().is_some_and(|r| r.primary != r.secondary);
                if c != cur && !selecting {
                    editing::set_primary(ctx, id, Sel::new(c, c));
                }
            }
            return;
        }
        let unit = t.indent.unit();
        let width = t.indent.width;
        let before_mode = self.vim.mode;
        let mut fx = Vec::new();
        for k in keys {
            if self.vim.mode == Mode::Insert && self.vim.cmdline.is_none() {
                // Typed quickly after i / a / o in the same moment: put it in.
                match k {
                    vim::ESC => self.vim.insert_done(&chars, &mut cur),
                    vim::ENTER => {
                        chars.insert(cur, '\n');
                        cur += 1;
                    }
                    vim::BS => {
                        if cur > 0 {
                            cur -= 1;
                            chars.remove(cur);
                        }
                    }
                    c if !c.is_control() => {
                        chars.insert(cur, c);
                        cur += 1;
                    }
                    _ => {}
                }
                continue;
            }
            fx.extend(self.vim.key(k, &mut chars, &mut cur, &unit, width));
        }
        if chars.iter().ne(t.text.chars().collect::<Vec<_>>().iter()) {
            // Normal and Visual mode's changes are an undo step each (Insert mode's typing
            // joins up as typing does).
            if before_mode != vim::Mode::Insert {
                t.hist.command();
            }
            t.text = chars.iter().collect();
            t.preview = false;
        }
        self.vim.head = cur;
        // The text box shows the cursor, or Visual mode's selection.
        let sel = match self.vim.visual_range(&chars, cur) {
            Some((a, b)) if cur >= self.vim.anchor => Sel::new(a, b),
            Some((a, b)) => Sel::new(b, a),
            None => Sel::new(cur, cur),
        };
        editing::set_primary(ctx, id, sel);
        if before_mode != self.vim.mode || !fx.is_empty() {
            ctx.request_repaint();
        }
        for f in fx {
            match f {
                Effect::Save => {
                    self.save(self.active);
                }
                Effect::SaveAll => self.save_all(),
                Effect::Close { force } => self.close_tab(self.active, force),
                Effect::SaveClose => {
                    if self.save(self.active) {
                        self.close_tab(self.active, false);
                    }
                }
                Effect::QuitAll { .. } => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Effect::Undo(n) | Effect::Redo(n) => {
                    let redo = matches!(f, Effect::Redo(_));
                    let modifiers = if redo { Modifiers::COMMAND | Modifiers::SHIFT } else { Modifiers::COMMAND };
                    ctx.input_mut(|i| {
                        for _ in 0..n.min(100) {
                            i.events.push(egui::Event::Key { key: Key::Z, physical_key: None, pressed: true, repeat: false, modifiers });
                        }
                    });
                }
                Effect::Message(m, bad) => self.note = Some((m, bad)),
                Effect::Reveal => self.reveal = true,
                Effect::Definition => self.nav.pending = Some(super::navigate::NavCmd::Definition),
                Effect::References => self.nav.pending = Some(super::navigate::NavCmd::References),
                Effect::Hover => self.nav.pending = Some(super::navigate::NavCmd::Hover),
                Effect::Yanked(text) => {
                    // Copies go to the clipboard too (never in checks).
                    if self.shot.is_none() {
                        ctx.copy_text(text);
                    }
                }
            }
        }
    }

    /// The status bar's Vim part, on the right: the mode as a coloured tag.
    pub(crate) fn vim_mode_tag(&self, ui: &mut egui::Ui) {
        if !self.s.vim_keys || self.tabs.is_empty() {
            return;
        }
        let v = ui.visuals().clone();
        let accent = self.accent();
        let hue = rivet_look::builtin_icons::hue;
        // A colour per mode, from the accent so it sits in the theme.
        let col = match self.vim.mode {
            Mode::Normal => accent,
            Mode::Insert => hue(accent, 120.0),
            Mode::Visual | Mode::VisualLine => hue(accent, -110.0),
        };
        let fg = if v.dark_mode { rivet_look::palette::mix(col, egui::Color32::WHITE, 0.25) } else { rivet_look::palette::mix(col, egui::Color32::BLACK, 0.3) };
        let g = ui.painter().layout_no_wrap(self.vim.mode_name().to_string(), egui::FontId::new(10.5, egui::FontFamily::Proportional), fg);
        let (r, resp) = ui.allocate_exact_size(egui::Vec2::new(g.size().x + 16.0, 18.0), egui::Sense::hover());
        ui.painter().rect_filled(r, rivet_look::theme::r_sm(), col.gamma_multiply(if v.dark_mode { 0.24 } else { 0.18 }));
        ui.painter().rect_stroke(r, rivet_look::theme::r_sm(), egui::Stroke::new(1.0, col.gamma_multiply(0.45)), egui::StrokeKind::Inside);
        // A touch of weight: the text drawn twice, a hair apart.
        let pos = r.center() - g.size() / 2.0;
        ui.painter().galley(pos, g.clone(), fg);
        ui.painter().galley(pos + egui::Vec2::new(0.45, 0.0), g, fg);
        resp.on_hover_text("Vim keys: Esc for Normal, i for Insert, v or V to select, : for commands");
    }

    /// Whether something shows in the middle of the status bar (the `:` / `/` line, or a
    /// Normal-mode command being typed).
    pub(crate) fn vim_center_busy(&self) -> bool {
        self.s.vim_keys && (self.vim.cmdline.is_some() || !self.vim.typed().is_empty())
    }

    /// The middle of the status bar: the `:` / `/` line while it's typed, or the keys of a
    /// Normal-mode command not finished yet ("2d", "ci") with what it's waiting for.
    pub(crate) fn vim_cmdline(&self, ui: &egui::Ui, bar: egui::Rect) {
        if !self.s.vim_keys {
            return;
        }
        let typed = self.vim.typed();
        let (prompt, line, hint, editing) = match &self.vim.cmdline {
            Some((k, line)) => (k.to_string(), line.clone(), if *k == ':' { "Enter runs it · Esc cancels" } else { "Enter searches · Esc cancels" }, true),
            None if !typed.is_empty() => (String::new(), typed.clone(), pending_hint(&typed), false),
            None => return,
        };
        let v = ui.visuals().clone();
        let accent = self.accent();
        let font = egui::FontId::monospace(12.5);
        let gk = ui.painter().layout_no_wrap(prompt, font.clone(), accent);
        let gl = ui.painter().layout_no_wrap(line.clone(), font, v.strong_text_color());
        let gh = ui.painter().layout_no_wrap(hint.to_string(), egui::FontId::proportional(11.5), v.weak_text_color());
        // Room for the hint as well when it shows.
        let show_hint = line.is_empty() || !editing;
        let need = gk.size().x + gl.size().x + 34.0 + if show_hint { gh.size().x + 14.0 } else { 0.0 };
        let w = need.max(280.0).min(bar.width() * 0.6);
        let r = egui::Rect::from_center_size(bar.center(), egui::Vec2::new(w, 22.0));
        let p = ui.painter().with_clip_rect(bar.expand(4.0));
        p.rect_filled(r, rivet_look::theme::r_md(), rivet_look::palette::mix(v.panel_fill, v.text_color(), 0.07));
        let edge = if editing { accent.gamma_multiply(0.7) } else { accent.gamma_multiply(0.35) };
        p.rect_stroke(r, rivet_look::theme::r_md(), egui::Stroke::new(1.0, edge), egui::StrokeKind::Inside);
        let x = r.left() + 12.0;
        let y = r.center().y;
        let kw = gk.size().x;
        p.galley(egui::Pos2::new(x, y - gk.size().y / 2.0), gk, accent);
        let tx = x + kw + if kw > 0.0 { 4.0 } else { 0.0 };
        let lw = gl.size().x;
        p.with_clip_rect(r.shrink(2.0)).galley(egui::Pos2::new(tx, y - gl.size().y / 2.0), gl, v.strong_text_color());
        // The caret.
        let cx = (tx + lw + 1.0).min(r.right() - 8.0);
        p.line_segment([egui::Pos2::new(cx, y - 7.0), egui::Pos2::new(cx, y + 7.0)], egui::Stroke::new(1.5, accent));
        if show_hint {
            p.with_clip_rect(r.shrink(2.0)).galley(egui::Pos2::new(cx + 8.0, y - gh.size().y / 2.0), gh, v.weak_text_color());
        }
    }
}

/// What a half-typed Normal-mode command is waiting for.
fn pending_hint(typed: &str) -> &'static str {
    let op = typed.trim_start_matches(|c: char| c.is_ascii_digit());
    let mut cs = op.chars();
    match (cs.next(), cs.next()) {
        (None, _) => "a count: now a motion or command",
        (Some('d'), None) => "delete: a motion (w, $, j…), iw, i( …, or d for the line",
        (Some('c'), None) => "change: a motion, iw, i\" …, or c for the line",
        (Some('y'), None) => "copy: a motion, iw, i( …, or y for the line",
        (Some('>'), None) => "indent: a motion, or > for the line",
        (Some('<'), None) => "outdent: a motion, or < for the line",
        (Some(_), Some('i')) => "inside: w, \", ', (, {, [, <",
        (Some(_), Some('a')) => "around: w, \", ', (, {, [, <",
        (Some('g'), _) => "g: g for the top, J to join without a space",
        (Some('f' | 't' | 'F' | 'T'), _) => "to the next character you type",
        (Some('r'), _) => "replace with the character you type",
        (Some('z'), _) => "z: z to bring the line into view",
        (Some(_), Some('f' | 't' | 'F' | 'T')) => "to the character you type",
        _ => "Esc cancels",
    }
}
