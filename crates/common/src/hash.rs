use std::{
    collections::HashMap,
    hash::{BuildHasherDefault, Hash, Hasher},
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
// This can be used in a `const` context, such as to initialize
// a static variable.
#[inline]
pub const fn new_const_hash_map<K, V>() -> PreHashedKeyMap<K, V> {
    PreHashedKeyMap::with_hasher(BuildHasherDefault::<IdentityHasher>::new())
}

// SmallMap starts with a fixed-size buffer but can expand into the heap.
// This allows us to mostly stay on the stack and avoid any allocations.
// We only care about the key being present or not, so value is an empty type.
pub struct SmallSet<const N: usize, T>(SmallMap<N, T, ()>);

impl<const N: usize, T> SmallSet<N, T>
where
    T: Eq + Hash,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fnv1a_empty_is_null() {
        assert_eq!(fnv1a_from_str(""), NULL_HASH);
    }

    #[test]
    fn test_fnv1a_known_values() {
        // Reference 64-bit FNV-1a test vectors.
        assert_eq!(fnv1a_from_str("a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a_from_str("foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn test_fnv1a_distinct_inputs() {
        assert_ne!(fnv1a_from_str("house"), fnv1a_from_str("House"));
        assert_ne!(fnv1a_from_str("ab"), fnv1a_from_str("ba"));
        assert_eq!(fnv1a_from_str("house"), fnv1a_from_str("house"));
    }

    #[test]
    fn test_fnv1a_const_eval() {
        const HASH: FNV1aHash = fnv1a_from_str("granary");
        assert_eq!(HASH, fnv1a_from_str("granary"));
    }

    #[test]
    fn test_str_hash_pair() {
        let empty = StrHashPair::empty();
        assert_eq!(empty.string, "");
        assert!(!empty.is_valid());

        // Default should match empty().
        let default = StrHashPair::default();
        assert_eq!(default.hash, NULL_HASH);
        assert!(!default.is_valid());

        let pair = StrHashPair::from_str("well");
        assert_eq!(pair.string, "well");
        assert_eq!(pair.hash, fnv1a_from_str("well"));
        assert!(pair.is_valid());

        // Empty string hashes to NULL_HASH, so it is not valid.
        assert!(!StrHashPair::from_str("").is_valid());
    }

    #[test]
    fn test_identity_hasher() {
        let mut hasher = IdentityHasher::default();
        assert_eq!(hasher.finish(), 0);

        hasher.write_u64(0x1234_5678_9abc_def0);
        assert_eq!(hasher.finish(), 0x1234_5678_9abc_def0);

        // Last write wins.
        hasher.write_u64(42);
        assert_eq!(hasher.finish(), 42);
    }

    #[test]
    #[should_panic]
    fn test_identity_hasher_write_bytes_panics() {
        let mut hasher = IdentityHasher::default();
        hasher.write(&[1, 2, 3]);
    }

    #[test]
    fn test_pre_hashed_key_map() {
        let mut map: PreHashedKeyMap<StringHash, i32> = new_const_hash_map();
        assert!(map.is_empty());

        map.insert(fnv1a_from_str("house"), 1);
        map.insert(fnv1a_from_str("farm"), 2);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&fnv1a_from_str("house")), Some(&1));
        assert_eq!(map.get(&fnv1a_from_str("farm")), Some(&2));
        assert_eq!(map.get(&fnv1a_from_str("well")), None);

        // Overwrite existing key.
        map.insert(fnv1a_from_str("house"), 3);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&fnv1a_from_str("house")), Some(&3));
    }

    #[test]
    fn test_small_set() {
        let mut set = SmallSet::<4, u32>::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);

        set.insert(1);
        set.insert(2);
        assert!(set.contains(&1));
        assert!(set.contains(&2));
        assert!(!set.contains(&3));
        assert_eq!(set.len(), 2);

        // Duplicate insert is a no-op.
        set.insert(1);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_small_set_grows_past_inline_capacity() {
        let mut set = SmallSet::<2, u32>::new();
        for i in 0..10 {
            set.insert(i);
        }

        assert_eq!(set.len(), 10);
        for i in 0..10 {
            assert!(set.contains(&i));
        }

        let mut keys: Vec<u32> = set.iter().map(|(k, _)| *k).collect();
        keys.sort();
        assert_eq!(keys, (0..10).collect::<Vec<_>>());
    }
}
