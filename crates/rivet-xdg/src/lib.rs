//! Desktop plumbing shared by the Rivet apps, with no user interface: the XDG folders,
//! bookmarks and drives (`places`), file types from shared-mime-info (`mime`), and default
//! applications, Open With and terminals (`launch`).

pub mod crash;
pub mod launch;
pub mod mime;
pub mod places;
