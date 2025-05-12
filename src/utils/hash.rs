use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

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

    fn write_u64(&mut self, h: u64) {
        self.hash = h;
    }

    fn finish(&self) -> u64 {
        self.hash
    }
}

pub type PreHashedKeyMap<K, V> = HashMap<K, V, BuildHasherDefault<IdentityHasher>>;

// ----------------------------------------------
// FNV-1a hash utilities
// ----------------------------------------------

pub type FNV1aHash = u64;
pub type StringHash = FNV1aHash;
pub const NULL_HASH: FNV1aHash = 0;

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
