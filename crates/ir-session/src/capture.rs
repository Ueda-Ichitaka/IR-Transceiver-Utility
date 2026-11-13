//! Capture buffer bounded by a byte budget.
//!
//! Exists to hold everything seen during continuous capture without letting
//! memory grow without limit. Every stored item gets a sequence number that
//! keeps increasing across evictions and clears, so a UI model can address
//! rows stably. What happens when the budget is reached is a policy.

use std::collections::VecDeque;

/// Approximate memory cost of an item in bytes.
pub trait Weighted {
    fn weight(&self) -> usize;
}

/// Behaviour when a new item does not fit into the budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowPolicy {
    /// Evict the oldest items until the new one fits.
    DropOldest,
    /// Keep what is stored and reject the new item.
    Reject,
}

/// Result of a push.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pushed {
    /// Stored with this sequence number after evicting `evicted` items.
    Stored { seq: u64, evicted: usize },
    /// Not stored (budget full under `Reject`, or the item alone is larger
    /// than the whole budget).
    Rejected,
}

/// Bounded buffer of captured items.
#[derive(Debug)]
pub struct CaptureBuffer<T> {
    items: VecDeque<(u64, T)>,
    budget: usize,
    used: usize,
    next_seq: u64,
    policy: OverflowPolicy,
    dropped: u64,
}

impl<T: Weighted> CaptureBuffer<T> {
    pub fn new(budget: usize, policy: OverflowPolicy) -> Self {
        Self { items: VecDeque::new(), budget, used: 0, next_seq: 0, policy, dropped: 0 }
    }

    /// Adds an item according to the overflow policy.
    pub fn push(&mut self, item: T) -> Pushed {
        let weight = item.weight();
        let fits = |used: usize| used + weight <= self.budget;
        if weight > self.budget || (self.policy == OverflowPolicy::Reject && !fits(self.used)) {
            self.dropped += 1;
            return Pushed::Rejected;
        }
        let mut evicted = 0;
        while !fits(self.used) {
            let (_, old) = self.items.pop_front().expect("used > 0 implies items");
            self.used -= old.weight();
            evicted += 1;
        }
        self.dropped += evicted as u64;
        let seq = self.next_seq;
        self.next_seq += 1;
        self.used += weight;
        self.items.push_back((seq, item));
        Pushed::Stored { seq, evicted }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Bytes currently accounted to stored items.
    pub fn used(&self) -> usize {
        self.used
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    /// Items evicted or rejected since creation.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Stored items with their sequence numbers, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = (u64, &T)> {
        self.items.iter().map(|(seq, item)| (*seq, item))
    }

    /// Item with the given sequence number, if still stored.
    pub fn get(&self, seq: u64) -> Option<&T> {
        let first = self.items.front()?.0;
        let index = usize::try_from(seq.checked_sub(first)?).ok()?;
        self.items.get(index).map(|(_, item)| item)
    }

    /// Removes all items; sequence numbers keep increasing.
    pub fn clear(&mut self) {
        self.items.clear();
        self.used = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Item(usize);

    impl Weighted for Item {
        fn weight(&self) -> usize {
            self.0
        }
    }

    #[test]
    fn stores_within_budget() {
        let mut b = CaptureBuffer::new(10, OverflowPolicy::DropOldest);
        assert_eq!(b.push(Item(4)), Pushed::Stored { seq: 0, evicted: 0 });
        assert_eq!(b.push(Item(6)), Pushed::Stored { seq: 1, evicted: 0 });
        assert_eq!((b.len(), b.used(), b.budget()), (2, 10, 10));
        assert!(!b.is_empty());
    }

    #[test]
    fn drop_oldest_evicts_until_it_fits() {
        let mut b = CaptureBuffer::new(10, OverflowPolicy::DropOldest);
        b.push(Item(4));
        b.push(Item(4));
        assert_eq!(b.push(Item(5)), Pushed::Stored { seq: 2, evicted: 1 });
        assert_eq!(b.iter().map(|(s, _)| s).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!((b.used(), b.dropped()), (9, 1));
    }

    #[test]
    fn reject_keeps_existing_items() {
        let mut b = CaptureBuffer::new(10, OverflowPolicy::Reject);
        b.push(Item(8));
        assert_eq!(b.push(Item(3)), Pushed::Rejected);
        assert_eq!((b.len(), b.used(), b.dropped()), (1, 8, 1));
    }

    #[test]
    fn item_larger_than_budget_is_rejected_without_eviction() {
        let mut b = CaptureBuffer::new(10, OverflowPolicy::DropOldest);
        b.push(Item(5));
        assert_eq!(b.push(Item(11)), Pushed::Rejected);
        assert_eq!((b.len(), b.dropped()), (1, 1));
    }

    #[test]
    fn get_by_sequence_number() {
        let mut b = CaptureBuffer::new(10, OverflowPolicy::DropOldest);
        b.push(Item(6));
        b.push(Item(6));
        assert_eq!(b.get(0), None, "evicted");
        assert_eq!(b.get(1), Some(&Item(6)));
        assert_eq!(b.get(2), None, "not yet stored");
    }

    #[test]
    fn clear_empties_but_keeps_counting() {
        let mut b = CaptureBuffer::new(10, OverflowPolicy::DropOldest);
        b.push(Item(1));
        b.clear();
        assert!(b.is_empty());
        assert_eq!(b.used(), 0);
        assert_eq!(b.push(Item(1)), Pushed::Stored { seq: 1, evicted: 0 });
    }
}
