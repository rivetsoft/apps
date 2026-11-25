//! Calendars with no interface: events kept in plain `.ics` folders (the layout vdirsyncer and
//! khal use), read and written without losing what Rivet doesn't use, and the occurrences in a
//! span of days, repeating events included. Rivet Calendar edits them; Rivet Shell's calendar
//! shows them through Rivet Calendar's plugin.

pub mod caldav;
pub mod event;
pub mod expand;
pub mod feed;
pub mod graph;
pub mod ics;
pub mod oauth;
pub mod store;
pub mod xml;

pub use event::{Event, When};
pub use store::{Calendar, Occurrence, Store, Watch, default_root};
