use std::any::Any;
use std::collections::hash_map::Entry;

use serde::{
    Serialize,
    Deserialize,
};

use super::{
    SingleThreadStatic,
    hash::{self, FNV1aHash, PreHashedKeyMap},
};

// ----------------------------------------------
// Callback
// ----------------------------------------------

// Serializable callback wrapper.
// Supports only plain function pointers without capture.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Callback<F> {
    key: CallbackKey,

    #[serde(skip, default = "default_cb_name")]
    name: &'static str,

    #[serde(skip, default = "default_cb_fptr")]
    fptr: Option<F>,
}

impl<F> Callback<F>
    where F: 'static + Copy + Clone + PartialEq
{
    #[inline]
    pub fn new(key: CallbackKey, name: &'static str, fptr: F) -> Self {
        register_internal(key, name, fptr);
        Self { key, name, fptr: Some(fptr) }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.key.is_valid()
    }

    #[inline]
    pub fn key(&self) -> CallbackKey {
        self.key
    }

    #[inline]
    pub fn name(&self) -> &'static str {
        self.name
    }

    #[inline]
    pub fn get(&self) -> F {
        debug_assert!(self.is_valid(), "Callback '{}' is not valid!", self.name);
        self.fptr.unwrap_or_else(|| panic!("Function pointer for callback '{}' is not set!", self.name))
    }

    #[inline]
    pub fn try_get(&self) -> Option<F> {
        if self.is_valid() {
            return self.fptr;
        }
        None
    }

    #[inline]
    pub fn post_load(&mut self) {
        if let Some(entry) = REGISTRY.find_entry(self.key) {
            self.name = entry.name;
            self.fptr = entry.cb.downcast_ref::<F>().copied();

            debug_assert!(self.fptr.is_some(), "Failed to lookup deserialized callback '{}'!", self.name);
            debug_assert!(self.key.hash == hash::fnv1a_from_str(self.name), "Callback name and key do not match for '{}'!", self.name);
        }
        // Else the callback key was invalid/default.
    }
}

impl<F> Default for Callback<F> {
    #[inline]
    fn default() -> Self {
        Self {
            key: CallbackKey::invalid(),
            name: default_cb_name(),
            fptr: default_cb_fptr(),
        }
    }
}

// Deserialization defaults:
#[inline]
const fn default_cb_name() -> &'static str { "<invalid>" }

#[inline]
const fn default_cb_fptr<F>() -> Option<F> { None }

// ----------------------------------------------
// CallbackKey
// ----------------------------------------------

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct CallbackKey {
    hash: FNV1aHash,
}

impl CallbackKey {
    #[inline]
    pub const fn new(name: &'static str) -> Self {
        Self { hash: hash::fnv1a_from_str(name) }
    }

    #[inline]
    pub const fn invalid() -> Self {
        Self { hash: hash::NULL_HASH }
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.hash != hash::NULL_HASH
    }
}

// ----------------------------------------------
// CallbackRegistry
// ----------------------------------------------

struct CallbackEntry {
    name: &'static str,
    cb: Box<dyn Any>,
}

// Global registry that maps a callback function to a 64bits integer that we can serialize/deserialize.
// The callback function name is also stored for debugging purposes.
struct CallbackRegistry {
    lookup: PreHashedKeyMap<FNV1aHash, CallbackEntry>,
}

impl CallbackRegistry {
    const fn new() -> Self {
        Self { lookup: hash::new_const_hash_map() }
    }

    fn register<F>(&'static mut self, key: CallbackKey, name: &'static str, fptr: F)
        where F: 'static + Copy + Clone + PartialEq
    {
        debug_assert!(key.is_valid());
        debug_assert!(!name.is_empty());

        match self.lookup.entry(key.hash) {
            Entry::Occupied(entry) => {
                if let Some(stored_fptr) = entry.get().cb.downcast_ref::<F>() {
                    if *stored_fptr != fptr {
                        panic!("A callback with a different address is already registered for '{name}'.");
                    }
                } else {
                    panic!("A callback with a different signature is already registered for '{name}'.");
                }
            },
            Entry::Vacant(entry) => {
                entry.insert(CallbackEntry { name, cb: Box::new(fptr) });
            },
        }
    }

    fn find_func_ptr<F>(&'static self, key: CallbackKey) -> Option<&'static F>
        where F: 'static + Copy + Clone + PartialEq
    {
        if !key.is_valid() {
            return None;
        }

        if let Some(entry) = self.lookup.get(&key.hash) {
            return entry.cb.downcast_ref::<F>();
        }

        None
    }

    fn find_entry(&'static self, key: CallbackKey) -> Option<&'static CallbackEntry> {
        if !key.is_valid() {
            return None;
        }
        self.lookup.get(&key.hash)
    }
}

// ----------------------------------------------
// Global Registry
// ----------------------------------------------

static REGISTRY: SingleThreadStatic<CallbackRegistry> = SingleThreadStatic::new(CallbackRegistry::new());

#[inline]
pub fn register_internal<F>(key: CallbackKey, name: &'static str, callback: F)
    where F: 'static + Copy + Clone + PartialEq
{
    REGISTRY.as_mut().register(key, name, callback);
}

#[inline]
pub fn find_internal<F>(key: CallbackKey) -> Option<&'static F>
    where F: 'static + Copy + Clone + PartialEq
{
    REGISTRY.find_func_ptr(key)
}

// ----------------------------------------------
// Public Macros
// ----------------------------------------------

#[macro_export]
macro_rules! register_callback {
    ($signature:path, $func:expr) => {{
        const KEY: $crate::utils::callback::CallbackKey = $crate::utils::callback::CallbackKey::new(stringify!($func));
        $crate::utils::callback::register_internal(KEY, stringify!($func), $func as $signature);
        KEY
    }};
}

#[macro_export]
macro_rules! find_callback {
    ($signature:path, $key:expr) => {
        $crate::utils::callback::find_internal::<$signature>($key)
    };
}

#[macro_export]
macro_rules! create_callback {
    ($func:expr) => {{
        const KEY: $crate::utils::callback::CallbackKey = $crate::utils::callback::CallbackKey::new(stringify!($func));
        $crate::utils::callback::Callback::new(KEY, stringify!($func), $func)
    }};
}

// Re-export here so usage is scoped, e.g.: callback::register!(...)
#[allow(unused_imports)]
pub use crate::{register_callback as register, find_callback as find, create_callback as create};

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[test]
fn test_callback_registry() {
    use crate::utils::callback;

    struct Test;
    impl Test {
        fn member_fn() -> usize { 1234 }
    }

    fn add_one(x: i32) -> i32 { x + 1 }
    fn to_upper(s: &str) -> String { s.to_uppercase() }
    fn multiply(a: i32, b: i32) -> i32 { a * b }

    type AddOneFn   = fn(i32) -> i32;
    type ToUpperFn  = fn(&str) -> String;
    type MultiplyFn = fn(i32, i32) -> i32;
    type MemberFn   = fn() -> usize;

    let add_one_key  = callback::register!(AddOneFn,   add_one);
    let to_upper_key = callback::register!(ToUpperFn,  to_upper);
    let multiply_key = callback::register!(MultiplyFn, multiply);
    let member_key   = callback::register!(MemberFn,   Test::member_fn);

    let cb0: Callback<AddOneFn>   = callback::create!(add_one);
    let cb1: Callback<ToUpperFn>  = callback::create!(to_upper);
    let cb2: Callback<MultiplyFn> = callback::create!(multiply);
    let cb3: Callback<MemberFn>   = callback::create!(Test::member_fn);

    assert!(cb0.is_valid() && cb0.try_get().is_some());
    assert!(cb1.is_valid() && cb1.try_get().is_some());
    assert!(cb2.is_valid() && cb2.try_get().is_some());
    assert!(cb3.is_valid() && cb3.try_get().is_some());

    if let Some(cb) = callback::find!(AddOneFn, add_one_key) {
        assert_eq!(cb(41), 42);
    } else {
        panic!("add_one callback not found!");
    }

    if let Some(cb) = callback::find!(ToUpperFn, to_upper_key) {
        assert_eq!(cb("hello"), "HELLO");
    } else {
        panic!("to_upper callback not found!");
    }

    if let Some(cb) = callback::find!(MultiplyFn, multiply_key) {
        assert_eq!(cb(2, 2), 4);
    } else {
        panic!("multiply callback not found!");
    }

    if let Some(cb) = callback::find!(MemberFn, member_key) {
        assert_eq!(cb(), 1234);
    } else {
        panic!("member_fn callback not found!");
    }
}
