use std::hash::BuildHasher;

use rustc_hash::{FxHashMap, FxHashSet};

pub trait HashMapExt {
    fn new() -> Self;
    fn with_capacity(capacity: usize) -> Self;
}

impl<K, V, S> HashMapExt for std::collections::HashMap<K, V, S>
where
    S: BuildHasher + Default,
{
    fn new() -> Self {
        std::collections::HashMap::with_hasher(S::default())
    }

    fn with_capacity(capacity: usize) -> Self {
        std::collections::HashMap::with_capacity_and_hasher(capacity, S::default())
    }
}

pub trait HashSetExt {
    fn new() -> Self;
    fn with_capacity(capacity: usize) -> Self;
}

impl<K, S> HashSetExt for std::collections::HashSet<K, S>
where
    S: BuildHasher + Default,
{
    fn new() -> Self {
        std::collections::HashSet::with_hasher(S::default())
    }

    fn with_capacity(capacity: usize) -> Self {
        std::collections::HashSet::with_capacity_and_hasher(capacity, S::default())
    }
}

pub type Map<K, V> = FxHashMap<K, V>;
pub type Set<K> = FxHashSet<K>;
