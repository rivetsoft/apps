//! The sidebar's layout: slots from top to bottom, each holding one panel or several as tabs.
//! Panels are moved by dragging their header or tab: onto another slot's header (or middle) to
//! join it as a tab, or onto its top or bottom edge to get a slot of their own there.

use crate::settings::Browse;

#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub panels: Vec<Browse>,
    /// The tab shown when there are several.
    pub active: usize,
}

impl Slot {
    pub fn shown(&self) -> Browse {
        self.panels[self.active.min(self.panels.len() - 1)]
    }
}

/// Where a dragged panel lands on a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    /// Joins the slot as a tab.
    Join,
    Above,
    Below,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub slots: Vec<Slot>,
    /// With two slots: the share of the height the second one takes.
    pub split: f32,
}

pub const ALL: [Browse; 2] = [Browse::Folders, Browse::Tags];

pub fn id(p: Browse) -> &'static str {
    match p {
        Browse::Folders => "folders",
        Browse::Tags => "tags",
    }
}

/// The key a slot is folded under (in the settings' folded list).
pub fn fold_key(s: &Slot) -> String {
    format!("pane:{}", id(s.panels[0]))
}

pub fn title(p: Browse) -> &'static str {
    match p {
        Browse::Folders => "Folders",
        Browse::Tags => "Tags",
    }
}

impl Default for Layout {
    /// Folders, with tags as a tab beside them.
    fn default() -> Self {
        Layout::tabs(Browse::Folders)
    }
}

impl Layout {
    /// Folders and tags as tabs in one slot, showing `active`.
    pub fn tabs(active: Browse) -> Layout {
        Layout { slots: vec![Slot { panels: vec![Browse::Folders, Browse::Tags], active: if active == Browse::Tags { 1 } else { 0 } }], split: 0.4 }
    }

    /// Folders above, tags below.
    pub fn stacked() -> Layout {
        Layout { slots: vec![Slot { panels: vec![Browse::Folders], active: 0 }, Slot { panels: vec![Browse::Tags], active: 0 }], split: 0.4 }
    }

    pub fn is_stacked(&self) -> bool {
        self.slots.len() > 1
    }

    /// The slot holding tabs, if any.
    pub fn tabbed(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.panels.len() > 1)
    }

    fn slot_of(&self, p: Browse) -> Option<usize> {
        self.slots.iter().position(|s| s.panels.contains(&p))
    }

    /// Show this panel (switching its slot's tab).
    pub fn show(&mut self, p: Browse) {
        if let Some(i) = self.slot_of(p) {
            let s = &mut self.slots[i];
            s.active = s.panels.iter().position(|x| *x == p).unwrap_or(0);
        }
    }

    /// Move a panel next to (or into) slot `target`. Nothing happens for moves that change nothing.
    pub fn move_panel(&mut self, p: Browse, target: usize, zone: Zone) {
        let Some(from) = self.slot_of(p) else { return };
        if target >= self.slots.len() || (from == target && (zone == Zone::Join || self.slots[from].panels.len() == 1)) {
            return;
        }
        let mut t = target;
        self.slots[from].panels.retain(|x| *x != p);
        if self.slots[from].panels.is_empty() {
            self.slots.remove(from);
            if from < t {
                t -= 1;
            }
        } else {
            let s = &mut self.slots[from];
            s.active = s.active.min(s.panels.len() - 1);
        }
        match zone {
            Zone::Join => {
                let s = &mut self.slots[t];
                s.panels.push(p);
                s.active = s.panels.len() - 1;
            }
            Zone::Above => self.slots.insert(t, Slot { panels: vec![p], active: 0 }),
            Zone::Below => self.slots.insert(t + 1, Slot { panels: vec![p], active: 0 }),
        }
    }

    /// Saved as `folders+!tags` (tabs, ! marks the shown one) and `,` between slots.
    pub fn to_text(&self) -> String {
        self.slots
            .iter()
            .map(|s| s.panels.iter().enumerate().map(|(i, p)| if s.panels.len() > 1 && i == s.active { format!("!{}", id(*p)) } else { id(*p).to_string() }).collect::<Vec<_>>().join("+"))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Read `to_text`'s form back. Unknown names are skipped; a panel missing from the text is
    /// added as a tab of the first slot, so every panel is always somewhere.
    pub fn from_text(t: &str, split: f32) -> Layout {
        let mut slots: Vec<Slot> = Vec::new();
        let mut seen: Vec<Browse> = Vec::new();
        for part in t.split(',') {
            let mut slot = Slot { panels: Vec::new(), active: 0 };
            for name in part.split('+') {
                let (on, name) = match name.trim().strip_prefix('!') {
                    Some(n) => (true, n),
                    None => (false, name.trim()),
                };
                if let Some(p) = ALL.into_iter().find(|p| id(*p) == name).filter(|p| !seen.contains(p)) {
                    if on {
                        slot.active = slot.panels.len();
                    }
                    slot.panels.push(p);
                    seen.push(p);
                }
            }
            if !slot.panels.is_empty() {
                slots.push(slot);
            }
        }
        if slots.is_empty() {
            return Layout { split, ..Layout::default() };
        }
        for p in ALL {
            if !seen.contains(&p) {
                slots[0].panels.push(p);
            }
        }
        Layout { slots, split: split.clamp(0.15, 0.85) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Browse::*;

    #[test]
    fn moving_panels() {
        // Tabs → drag Tags below Folders → stacked.
        let mut l = Layout::tabs(Folders);
        l.move_panel(Tags, 0, Zone::Below);
        assert_eq!(l, Layout::stacked());
        // Drag Tags above Folders → tags first.
        l.move_panel(Tags, 0, Zone::Above);
        assert_eq!(l.slots.iter().map(|s| s.panels.clone()).collect::<Vec<_>>(), [vec![Tags], vec![Folders]]);
        // Drag Folders onto Tags → one slot, folders shown.
        l.move_panel(Folders, 0, Zone::Join);
        assert_eq!(l.slots.len(), 1);
        assert_eq!(l.slots[0].panels, [Tags, Folders]);
        assert_eq!(l.slots[0].shown(), Folders);
        // Dropping a lone panel on its own slot changes nothing.
        let mut s = Layout::stacked();
        s.move_panel(Folders, 0, Zone::Below);
        assert_eq!(s, Layout::stacked());
    }

    #[test]
    fn saved_form() {
        for l in [Layout::tabs(Tags), Layout::stacked(), Layout::tabs(Folders)] {
            assert_eq!(Layout::from_text(&l.to_text(), 0.4), l);
        }
        assert_eq!(Layout::tabs(Tags).to_text(), "folders+!tags");
        // A missing panel comes back as a tab; nonsense falls back to the default.
        assert_eq!(Layout::from_text("folders", 0.4).slots[0].panels, [Folders, Tags]);
        assert_eq!(Layout::from_text("??", 0.4), Layout::default());
    }
}
