use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

/// A cache kept under a size in bytes: each entry says what it costs, and the least recently
/// used go first when a new one doesn't fit. What it lets go of is reported to
/// [`crate::alloc::released`], so the memory goes back to the system.
pub struct ByteCache<K, V> {
    map: HashMap<K, Slot<V>>,
    /// Last use -> key, oldest first.
    order: BTreeMap<u64, K>,
    tick: u64,
    bytes: usize,
    budget: usize,
}

struct Slot<V> {
    value: V,
    bytes: usize,
    used: u64,
}

impl<K: Eq + Hash + Clone, V> ByteCache<K, V> {
    pub fn new(budget: usize) -> Self {
        ByteCache { map: HashMap::new(), order: BTreeMap::new(), tick: 0, bytes: 0, budget }
    }

    /// The entry, marked as just used.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.get_mut(key).map(|v| &*v)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let slot = self.map.get_mut(key)?;
        self.tick += 1;
        self.order.remove(&slot.used);
        slot.used = self.tick;
        self.order.insert(self.tick, key.clone());
        Some(&mut slot.value)
    }

    /// The entry, without counting it as used.
    pub fn peek(&self, key: &K) -> Option<&V> {
        self.map.get(key).map(|s| &s.value)
    }

    pub fn contains(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    /// Keep `value`, which costs `bytes`, letting go of the least recently used entries until
    /// everything fits. The new entry always stays, even when it alone is over the budget.
    pub fn insert(&mut self, key: K, value: V, bytes: usize) {
        self.remove(&key);
        self.tick += 1;
        self.order.insert(self.tick, key.clone());
        self.map.insert(key, Slot { value, bytes, used: self.tick });
        self.bytes += bytes;
        self.shrink();
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let slot = self.map.remove(key)?;
        self.order.remove(&slot.used);
        self.bytes -= slot.bytes;
        crate::alloc::released(slot.bytes);
        Some(slot.value)
    }

    /// Keep only the entries `keep` says yes to.
    pub fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) {
        let gone: Vec<K> = self.map.iter().filter(|(k, s)| !keep(k, &s.value)).map(|(k, _)| k.clone()).collect();
        for k in gone {
            self.remove(&k);
        }
    }

    pub fn clear(&mut self) {
        crate::alloc::released(self.bytes);
        self.map.clear();
        self.order.clear();
        self.bytes = 0;
    }

    /// Change the budget, letting go of entries at once if they no longer fit.
    pub fn set_budget(&mut self, budget: usize) {
        self.budget = budget;
        self.shrink();
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    /// What the entries cost together.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    fn shrink(&mut self) {
        while self.bytes > self.budget && self.map.len() > 1 {
            let Some((_, key)) = self.order.pop_first() else { break };
            if let Some(slot) = self.map.remove(&key) {
                self.bytes -= slot.bytes;
                crate::alloc::released(slot.bytes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn least_recently_used_go_first() {
        let mut c = ByteCache::new(300);
        c.insert("a", 1, 100);
        c.insert("b", 2, 100);
        c.insert("c", 3, 100);
        assert_eq!(c.get(&"a"), Some(&1));
        c.insert("d", 4, 100);
        assert!(c.contains(&"a") && !c.contains(&"b") && c.contains(&"c") && c.contains(&"d"));
        assert_eq!(c.bytes(), 300);
    }

    #[test]
    fn peek_does_not_count_as_a_use() {
        let mut c = ByteCache::new(200);
        c.insert(1, (), 100);
        c.insert(2, (), 100);
        assert!(c.peek(&1).is_some());
        c.insert(3, (), 100);
        assert!(!c.contains(&1));
    }

    #[test]
    fn replacing_counts_the_new_size_and_big_entries_still_stay() {
        let mut c = ByteCache::new(100);
        c.insert(1, (), 40);
        c.insert(1, (), 60);
        assert_eq!((c.len(), c.bytes()), (1, 60));
        c.insert(2, (), 500);
        assert_eq!((c.len(), c.bytes()), (1, 500));
        c.set_budget(1000);
        c.insert(3, (), 100);
        c.retain(|k, _| *k != 2);
        assert_eq!((c.len(), c.bytes()), (1, 100));
        c.clear();
        assert!(c.is_empty() && c.bytes() == 0);
    }
}
