use std::{
    hash::Hash,
    collections::HashMap,
    hash::{BuildHasherDefault, Hasher},
};

use small_map::SmallMap;

// ----------------------------------------------
// PreHashedKeyMap / IdentityHasher
// ----------------------------------------------

#[derive(Default)]
pub struct IdentityHasher {
    hash: u64,
}

// Hasher for maps where the key is a u64 that is itself already
// the hash of some data, so no further hashing is needed.
// Just returns the value as is.
impl Hasher for IdentityHasher {
    fn write(&mut self, _: &[u8]) {
        panic!("Only write_u64 is supported!");
    }

    #[inline]
    fn write_u64(&mut self, h: u64) {
        self.hash = h;
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

pub type PreHashedKeyMap<K, V> = HashMap<K, V, BuildHasherDefault<IdentityHasher>>;

// Creates a default initialized empty PreHashedKeyMap.
// This can be used in a `const` context, such as to initialize a static
// variable.
#[inline]
pub const fn new_const_hash_map<K, V>() -> PreHashedKeyMap<K, V> {
    PreHashedKeyMap::with_hasher(BuildHasherDefault::<IdentityHasher>::new())
}

// SmallMap starts with a fixed-size buffer but can expand into the heap.
// This allows us to mostly stay on the stack and avoid any allocations.
// We only care about the key being present or not, so value is an empty type.
pub struct SmallSet<const N: usize, T>(SmallMap<N, T, ()>);

impl<const N: usize, T> SmallSet<N, T>
    where T: Eq + Hash
{
    #[inline]
    pub fn new() -> Self {
        Self(SmallMap::new())
    }

    #[inline]
    pub fn contains(&self, key: &T) -> bool {
        self.0.get(key).is_some()
    }

    #[inline]
    pub fn insert(&mut self, key: T) {
        self.0.insert(key, ());
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[inline]
    pub fn iter(&self) -> small_map::Iter<'_, N, T, ()> {
        self.0.iter()
    }
}

// ----------------------------------------------
// FNV-1a hash utilities
// ----------------------------------------------

pub type FNV1aHash = u64;
pub type StringHash = FNV1aHash;
pub const NULL_HASH: FNV1aHash = 0;

#[derive(Copy, Clone, Default)]
pub struct StrHashPair {
    pub string: &'static str,
    pub hash: StringHash,
}

impl StrHashPair {
    #[inline]
    pub const fn empty() -> Self {
        Self { string: "", hash: NULL_HASH }
    }

    #[inline]
    pub const fn from_str(string: &'static str) -> Self {
        Self { string, hash: fnv1a_from_str(string) }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.hash != NULL_HASH
    }
}

pub const fn fnv1a_from_str(s: &str) -> FNV1aHash {
    if s.is_empty() {
        return NULL_HASH;
    }

    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let bytes = s.as_bytes();
    let mut hash = FNV_OFFSET;
    let mut i = 0;

    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        i += 1;
    }

    hash
}
