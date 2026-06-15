use std::{
    cmp::Eq,
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
};

pub struct UniqueVec<T>
where
    T: Hash + Eq + Clone,
{
    vec: Vec<T>,
    hash_set: HashSet<u64>,
}

impl<T> UniqueVec<T>
where
    T: Hash + Eq + Clone,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            vec: Vec::new(),
            hash_set: HashSet::new(),
        }
    }

    pub fn push(&mut self, item: T) {
        if self.add_hash(&item) {
            self.vec.push(item);
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        self.vec.pop().map_or_else(
            || None,
            |item| {
                self.remove_hash(&item);
                Some(item)
            },
        )
    }
}

impl<T> UniqueVec<T>
where
    T: Hash + Eq + Clone,
{
    fn hashed(item: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        item.hash(&mut hasher);
        hasher.finish()
    }

    fn add_hash(&mut self, item: &T) -> bool {
        let hash = Self::hashed(item);
        self.hash_set.insert(hash)
    }

    fn remove_hash(&mut self, item: &T) {
        let hash = Self::hashed(item);
        self.hash_set.remove(&hash);
    }
}

impl<T> Default for UniqueVec<T>
where
    T: Hash + Eq + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> IntoIterator for UniqueVec<T>
where
    T: Hash + Eq + Clone,
{
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.vec.into_iter()
    }
}
