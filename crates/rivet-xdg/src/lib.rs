//! Desktop plumbing shared by the Rivet apps, with no user interface: the XDG folders,
//! bookmarks and drives (`places`), file types from shared-mime-info (`mime`), and default
//! applications, Open With and terminals (`launch`).

pub mod crash;
pub mod keyring;
pub mod launch;
pub mod mime;
pub mod places;
pub mod trash;
#[cfg(feature = "portal")]
pub mod notify;
#[cfg(feature = "portal")]
pub mod portal;
