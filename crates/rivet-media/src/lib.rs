//! Previews shared by the Rivet apps: loading pictures, video frames, album art and text
//! (`preview`), freedesktop thumbnails (`thumbs`), the full-window light box (`lightbox`),
//! pictures at full size with their frames and camera details (`picture`), and the stage that
//! zooms, moves and turns a picture (`stage`), which Rivet Long Shot and the light box share. With the
//! `mpv` feature, playback through libmpv (`mpv`).

use rivet_look::icons;
use rivet_xdg::places;

pub mod lightbox;
pub mod picture;
#[cfg(feature = "mpv")]
pub mod mpv;
pub mod preview;
pub mod stage;
pub mod thumbs;
