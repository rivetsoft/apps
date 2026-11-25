//! The terminal shared by Rivet apps (Rivet Terminal, and later Rivet Edit's panel).
//!
//! * [`Session`]: one shell or program in a pseudo-terminal, its screen and scrollback kept up
//!   to date on a reader thread (the emulation is `alacritty_terminal`'s);
//! * [`view::show`]: draws a session as an egui widget and sends it the keyboard and mouse;
//! * [`TermColors`]: its colours, from the app's theme or a shared colour scheme.
//!
//! Apps own the sessions and their layout (tabs, splits); this crate knows nothing of windows.

pub mod colors;
pub mod session;
pub mod view;

pub use colors::TermColors;
pub use session::{MAX_SCROLLBACK, Notice, Session, Size, Spawn};
pub use view::{CursorStyle, Style};
