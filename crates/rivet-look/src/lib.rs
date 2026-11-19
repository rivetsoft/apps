//! The look shared by the Rivet apps: live desktop colours (`palette`), fonts, icon themes and
//! Rivet's own icons (`icons`, `builtin_icons`, `nerd`), theme-coloured code (`highlight`), and
//! the egui theme built from them (`theme`), and in-app windows (`windows`).

use rivet_xdg::places;

pub mod builtin_icons;
pub mod fonts;
pub mod highlight;
pub mod live;
pub mod icons;
pub mod mdi;
pub mod nerd;
pub mod palette;
pub mod quick;
pub mod schemes;
pub mod tabdrag;
pub mod tabs;
pub mod theme;
pub mod widgets;
pub mod windows;
