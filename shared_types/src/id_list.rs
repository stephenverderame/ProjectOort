use super::*;
use std::collections::VecDeque;

/// A data type that caches allocated object IDs to use to create new objects
pub struct IdList {
    /// list of ranges of available ids
    ids: VecDeque<(ObjectId, ObjectId)>,
    /// Invariant: prev_id is `None` or in between the first and last
    /// ids of the front range in the list
    prev_id: Option<ObjectId>,
}

impl IdList {
    /// Creates a new empty `IdList`
    #[must_use]
    pub fn new() -> Self {
        Self {
            ids: VecDeque::new(),
            prev_id: None,
        }
    }

    /// Gets the next available ID
    /// and consumes it
    /// Returns `None` if there are no more IDs
    pub fn next_id(&mut self) -> Option<ObjectId> {
        let mut next_id: Option<ObjectId> = None;
        if let Some(prev_id) = &self.prev_id {
            if let Some((_, end)) = self.ids.front() {
                if prev_id.next() == *end {
                    self.ids.pop_front();
                    if let Some((begin, _)) = self.ids.front() {
                        next_id = Some(*begin);
                    }
                } else {
                    next_id = Some(prev_id.next());
                }
            }
        } else if let Some((begin, _)) = self.ids.front() {
            next_id = Some(*begin);
        }
        self.prev_id = next_id;
        next_id
    }

    /// Adds a range of allocated ids to the list
    pub fn add_ids(&mut self, range: (ObjectId, ObjectId)) {
        self.ids.push_back(range);
    }

    /// Computes the number of remaining IDs in the list
    #[must_use]
    pub fn remaining(&self) -> usize {
        let begin_size = self.prev_id.as_ref().map_or_else(
            || {
                self.ids.front().map_or(0, |(begin, end)| {
                    end.id.wrapping_sub(begin.id) as usize
                })
            },
            |prev| {
                self.ids.front().map_or(0, |(_, end)| {
                    end.id.wrapping_sub(prev.id.wrapping_add(1)) as usize
                })
            },
        );
        let rest_size: usize = self
            .ids
            .iter()
            .skip(1)
            .map(|(begin, end)| end.id.wrapping_sub(begin.id) as usize)
            .sum();
        begin_size + rest_size
    }
}

impl Iterator for IdList {
    type Item = ObjectId;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_id()
    }
}

impl Default for IdList {
    fn default() -> Self {
        Self::new()
    }
}

#[test]
fn id_list_add_remove_cnt() {
    let mut lst = IdList::new();
    assert_eq!(lst.next_id(), None);
    lst.add_ids((ObjectId::new(10), ObjectId::new(400)));
    lst.add_ids((ObjectId::new(500), ObjectId::new(600)));
    lst.add_ids((ObjectId::new(ObjectIdType::MAX - 10), ObjectId::new(5)));
    let mut cnt = 0 as ObjectIdType;
    for id in lst {
        cnt += 1;
        assert!(
            id.id >= 10 && id.id < 400
                || id.id >= 500 && id.id < 600
                || id.id >= ObjectIdType::MAX - 10
                || id.id < 5
        );
    }
    assert_eq!(
        cnt,
        (400 - 10)
            + (600 - 500)
            + (5 as ObjectIdType).wrapping_sub(ObjectIdType::MAX - 10)
    );
}
