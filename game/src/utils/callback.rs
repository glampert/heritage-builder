use std::{any::Any, collections::hash_map::Entry};

use serde::{Deserialize, Serialize};

use super::hash::{self, FNV1aHash, PreHashedKeyMap};
use crate::singleton;

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

impl<F> Callback<F> where F: 'static + Copy + Clone + PartialEq
{
    #[inline]
    pub fn create(key: CallbackKey, name: &'static str, fptr: F) -> Self {
        CallbackRegistry::get_mut().register(key, name, fptr, true);
        Self { key, name, fptr: Some(fptr) }
    }

    #[inline]
    pub fn register(key: CallbackKey, name: &'static str, fptr: F) -> Self {
        CallbackRegistry::get_mut().register(key, name, fptr, false);
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
        self.fptr
            .unwrap_or_else(|| panic!("Function pointer for callback '{}' is not set!", self.name))
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
        if let Some(entry) = CallbackRegistry::get().find_entry(self.key) {
            self.name = entry.name;
            self.fptr = entry.cb.downcast_ref::<F>().copied();

            debug_assert!(self.fptr.is_some(),
                          "Failed to lookup deserialized callback '{}'!",
                          self.name);
            debug_assert!(self.key.hash == hash::fnv1a_from_str(self.name),
                          "Callback name and key do not match for '{}'!",
                          self.name);
        } else {
            // Else the callback key must be invalid/default.
            // If it isn't then we failed to find it in the registry.
            debug_assert!(!self.key.is_valid(),
                          "Failed to find callback '{}' in registry!",
                          self.name);
        }
    }
}

impl<F> Default for Callback<F> {
    #[inline]
    fn default() -> Self {
        Self { key: CallbackKey::invalid(), name: default_cb_name(), fptr: default_cb_fptr() }
    }
}

// Deserialization defaults:
#[inline]
const fn default_cb_name() -> &'static str {
    "<invalid>"
}

#[inline]
const fn default_cb_fptr<F>() -> Option<F> {
    None
}

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

// Global registry that maps a callback function to a 64bits integer that we can
// serialize/deserialize. The callback function name is also stored for
// debugging purposes.
struct CallbackRegistry {
    lookup: PreHashedKeyMap<FNV1aHash, CallbackEntry>,
}

impl CallbackRegistry {
    const fn new() -> Self {
        Self { lookup: hash::new_const_hash_map() }
    }

    fn register<F>(&'static mut self,
                   key: CallbackKey,
                   name: &'static str,
                   fptr: F,
                   expect_entry: bool)
        where F: 'static + Copy + Clone + PartialEq
    {
        debug_assert!(key.is_valid());
        debug_assert!(!name.is_empty());

        match self.lookup.entry(key.hash) {
            Entry::Occupied(entry) => {
                if let Some(registered_fptr) = entry.get().cb.downcast_ref::<F>() {
                    if *registered_fptr != fptr {
                        panic!("A callback with a different address is already registered for '{name}'.");
                    }
                } else {
                    panic!("A callback with a different signature is already registered for '{name}'.");
                }
            }
            Entry::Vacant(entry) => {
                if expect_entry {
                    panic!("Callback '{name}' is not registered!");
                }
                entry.insert(CallbackEntry { name, cb: Box::new(fptr) });
            }
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

singleton! { CALLBACK_REGISTRY_SINGLETON, CallbackRegistry }

#[inline]
pub fn find<F>(key: CallbackKey) -> Option<&'static F>
    where F: 'static + Copy + Clone + PartialEq
{
    CallbackRegistry::get().find_func_ptr(key)
}

// ----------------------------------------------
// Public Macros
// ----------------------------------------------

#[macro_export]
macro_rules! register_callback {
    ($func:expr) => {{
        const KEY: $crate::utils::callback::CallbackKey =
            $crate::utils::callback::CallbackKey::new(stringify!($func));
        $crate::utils::callback::Callback::register(KEY, stringify!($func), $func)
    }};
}

#[macro_export]
macro_rules! create_callback {
    ($func:expr) => {{
        const KEY: $crate::utils::callback::CallbackKey =
            $crate::utils::callback::CallbackKey::new(stringify!($func));
        $crate::utils::callback::Callback::create(KEY, stringify!($func), $func)
    }};
}

// Re-export here so usage is scoped, e.g.: callback::register!(...)
#[allow(unused_imports)]
pub use crate::{create_callback as create, register_callback as register};

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

    let add_one_cb:  Callback<AddOneFn>    = callback::register!(add_one);
    let to_upper_cb: Callback<ToUpperFn>   = callback::register!(to_upper);
    let multiply_cb: Callback<MultiplyFn>  = callback::register!(multiply);
    let member_cb:   Callback<MemberFn>    = callback::register!(Test::member_fn);

    let add_one_cb2:  Callback<AddOneFn>   = callback::create!(add_one);
    let to_upper_cb2: Callback<ToUpperFn>  = callback::create!(to_upper);
    let multiply_cb2: Callback<MultiplyFn> = callback::create!(multiply);
    let member_cb2:   Callback<MemberFn>   = callback::create!(Test::member_fn);

    assert!(add_one_cb2.is_valid()  && add_one_cb2.try_get().is_some());
    assert!(to_upper_cb2.is_valid() && to_upper_cb2.try_get().is_some());
    assert!(multiply_cb2.is_valid() && multiply_cb2.try_get().is_some());
    assert!(member_cb2.is_valid()   && member_cb2.try_get().is_some());

    if let Some(cb) = callback::find::<AddOneFn>(add_one_cb.key) {
        assert_eq!(cb(41), 42);
    } else {
        panic!("add_one callback not found!");
    }

    if let Some(cb) = callback::find::<ToUpperFn>(to_upper_cb.key) {
        assert_eq!(cb("hello"), "HELLO");
    } else {
        panic!("to_upper callback not found!");
    }

    if let Some(cb) = callback::find::<MultiplyFn>(multiply_cb.key) {
        assert_eq!(cb(2, 2), 4);
    } else {
        panic!("multiply callback not found!");
    }

    if let Some(cb) = callback::find::<MemberFn>(member_cb.key) {
        assert_eq!(cb(), 1234);
    } else {
        panic!("member_fn callback not found!");
    }
}
