//! Memory hygiene shared by the Rivet apps, with no interface and nothing but libc:
//!
//! * [`alloc`]: the allocator's settings, set once at the start of `main`, and handing memory
//!   the app has freed back to the system (after a big free, and when the app has been quiet);
//! * [`ByteCache`]: a cache kept under a size in bytes, least recently used out first, for
//!   textures, thumbnails and pictures;
//! * [`shared_file`]: a file's bytes read once per process and shared (fonts, for every egui
//!   context);
//! * [`Usage`]: how much memory a process uses (RSS, PSS), for checks, benchmarks and rivetfetch.

pub mod alloc;
mod cache;
mod files;
mod usage;

pub use cache::ByteCache;
pub use files::shared_file;
pub use usage::{Usage, heap};
