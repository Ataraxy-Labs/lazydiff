//! Tiny FIFO-bounded map.
//!
//! Keeps memory predictable for the app's caches (PR diffs, semantic
//! diffs, body previews, etc.) without pulling in an external LRU
//! crate. When the map exceeds its capacity it drops the oldest entry
//! (by insertion order). Re-inserting an existing key replaces the
//! value in place and does not refresh its position — strict LRU is
//! overkill for these caches and the simpler model keeps the hot path
//! cheap.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

#[derive(Debug)]
pub(crate) struct BoundedMap<K, V> {
    cap: usize,
    map: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K, V> BoundedMap<K, V>
where
    K: Eq + Hash + Clone,
{
    pub(crate) fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub(crate) fn insert(&mut self, key: K, value: V) -> Option<V> {
        if let Some(old) = self.map.insert(key.clone(), value) {
            return Some(old);
        }
        self.order.push_back(key);
        while self.order.len() > self.cap {
            if let Some(evicted) = self.order.pop_front() {
                self.map.remove(&evicted);
            }
        }
        None
    }

    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }

    pub(crate) fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    pub(crate) fn remove(&mut self, key: &K) -> Option<V> {
        let value = self.map.remove(key)?;
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
        }
        Some(value)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.map.iter()
    }

    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    pub(crate) fn retain<F>(&mut self, mut predicate: F)
    where
        F: FnMut(&K, &mut V) -> bool,
    {
        self.map.retain(|k, v| predicate(k, v));
        self.order.retain(|k| self.map.contains_key(k));
    }

    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_oldest_when_full() {
        let mut m: BoundedMap<i32, i32> = BoundedMap::new(2);
        m.insert(1, 10);
        m.insert(2, 20);
        m.insert(3, 30); // should evict 1
        assert_eq!(m.len(), 2);
        assert!(!m.contains_key(&1));
        assert_eq!(m.get(&2), Some(&20));
        assert_eq!(m.get(&3), Some(&30));
    }

    #[test]
    fn replacing_existing_does_not_evict() {
        let mut m: BoundedMap<i32, i32> = BoundedMap::new(2);
        m.insert(1, 10);
        m.insert(2, 20);
        let old = m.insert(1, 11);
        assert_eq!(old, Some(10));
        assert_eq!(m.len(), 2);
        assert_eq!(m.get(&1), Some(&11));
        assert_eq!(m.get(&2), Some(&20));
    }

    #[test]
    fn remove_drops_order_entry() {
        let mut m: BoundedMap<i32, i32> = BoundedMap::new(2);
        m.insert(1, 10);
        m.insert(2, 20);
        assert_eq!(m.remove(&1), Some(10));
        m.insert(3, 30);
        // 2 should still be present because remove pulled 1 out of order.
        assert!(m.contains_key(&2));
        assert!(m.contains_key(&3));
    }

    #[test]
    fn zero_capacity_clamps_to_one() {
        let mut m: BoundedMap<i32, i32> = BoundedMap::new(0);
        m.insert(1, 10);
        m.insert(2, 20);
        assert_eq!(m.len(), 1);
    }
}
