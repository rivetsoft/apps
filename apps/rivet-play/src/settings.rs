//! Rivet Play's settings: `~/.config/rivet-play/settings`, `key=value` lines a person can edit,
//! unknown keys skipped so a newer file still loads.

use crate::queue::Repeat;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// 0–100.
    pub volume: f64,
    pub muted: bool,
    /// Start a file where it was left.
    pub resume: bool,
    /// Decode with the graphics card where it can.
    pub hardware: bool,
    /// Languages to choose tracks by, as mpv takes them ("en,eng").
    pub audio_lang: String,
    pub sub_lang: String,
    /// How big subtitles are, 1.0 as made.
    pub sub_scale: f64,
    pub repeat: Repeat,
    pub shuffle: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { volume: 100.0, muted: false, resume: true, hardware: true, audio_lang: String::new(), sub_lang: String::new(), sub_scale: 1.0, repeat: Repeat::Off, shuffle: false }
    }
}

fn file() -> std::path::PathBuf {
    rivet_xdg::places::config_home().join("rivet-play/settings")
}

impl Settings {
    pub fn load() -> Settings {
        Settings::parse(&std::fs::read_to_string(file()).unwrap_or_default())
    }

    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let on = v == "true";
            match k {
                "volume" => s.volume = v.parse::<f64>().map(|x| x.clamp(0.0, 100.0)).unwrap_or(s.volume),
                "muted" => s.muted = on,
                "resume" => s.resume = on,
                "hardware_decoding" => s.hardware = on,
                "audio_language" => s.audio_lang = v.to_string(),
                "subtitle_language" => s.sub_lang = v.to_string(),
                "subtitle_scale" => s.sub_scale = v.parse::<f64>().map(|x| x.clamp(0.5, 3.0)).unwrap_or(1.0),
                "repeat" => s.repeat = Repeat::from_key(v),
                "shuffle" => s.shuffle = on,
                _ => {}
            }
        }
        s
    }

    pub fn text(&self) -> String {
        format!(
            "volume={}\nmuted={}\nresume={}\nhardware_decoding={}\naudio_language={}\nsubtitle_language={}\nsubtitle_scale={}\nrepeat={}\nshuffle={}\n",
            self.volume.round(),
            self.muted,
            self.resume,
            self.hardware,
            self.audio_lang,
            self.sub_lang,
            self.sub_scale,
            self.repeat.key(),
            self.shuffle
        )
    }

    /// Write the file on a thread.
    pub fn save(&self) {
        let text = self.text();
        std::thread::spawn(move || {
            let f = file();
            if let Some(d) = f.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(f, text);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = Settings { volume: 63.0, muted: true, resume: false, hardware: false, audio_lang: "fr,fre".into(), sub_lang: "en".into(), sub_scale: 1.25, repeat: Repeat::All, shuffle: true };
        assert_eq!(Settings::parse(&s.text()), s);
        assert_eq!(Settings::parse("volume=400\nnonsense\nnew_key=1\n").volume, 100.0);
        assert_eq!(Settings::parse(""), Settings::default());
    }
}
