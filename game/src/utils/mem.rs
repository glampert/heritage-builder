// mut-casts in RawPtr/Mutable are intentional and required.
#![allow(clippy::mut_from_ref)]

use core::ptr::NonNull;
use std::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::OnceLock,
    thread::ThreadId,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ----------------------------------------------
// RawPtr
// ----------------------------------------------

// Store a non-null raw pointer. This allows bypassing the language lifetime
// guarantees, so should be used with care.
pub struct RawPtr<T> {
    ptr: NonNull<T>,
}

impl<T> RawPtr<T> {
    #[inline]
    pub fn from_ref(reference: &T) -> Self {
        let ptr_mut = reference as *const T as *mut T;
        Self { ptr: NonNull::new(ptr_mut).unwrap() }
    }

    #[inline]
    pub fn from_ptr(ptr: *const T) -> Self {
        let ptr_mut = ptr as *mut T;
        Self { ptr: NonNull::new(ptr_mut).unwrap() }
    }

    // Convert raw pointer to reference.
    // Pointer is never null but there are not guarantees about its lifetime.
    #[inline(always)]
    pub fn as_ref(&self) -> &T {
        unsafe { self.ptr.as_ref() }
    }

    // Convert raw pointer to mutable reference.
    // SAFETY: Caller must ensure there are no aliasing issues
    // (e.g. no other refs) and valid pointer lifetime.
    #[inline(always)]
    pub fn as_mut(&mut self) -> &mut T {
        unsafe { self.ptr.as_mut() }
    }

    // Cast from non mutable to mutable reference (const-cast).
    #[inline(always)]
    pub fn mut_ref_cast(&self) -> &mut T {
        unsafe { &mut *self.ptr.as_ptr() }
    }
}

// Implement Deref/DerefMut to allow `&*value` or `value.field` syntax.
impl<T> Deref for RawPtr<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T> DerefMut for RawPtr<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: Caller must ensure exclusive access (no aliasing).
        self.as_mut()
    }
}

impl<T> Copy for RawPtr<T> {}
impl<T> Clone for RawPtr<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self // Just a cheap pointer copy.
    }
}

// ----------------------------------------------
// Mutable
// ----------------------------------------------

// Hold an UnsafeCell<T> which allows unchecked interior mutability (casting
// away const).
pub struct Mutable<T> {
    cell: UnsafeCell<T>,
}

impl<T> Mutable<T> {
    #[inline]
    pub fn new(instance: T) -> Self {
        Self { cell: UnsafeCell::new(instance) }
    }

    // Safe to share immutable ref, no interior mutability.
    #[inline(always)]
    pub fn as_ref(&self) -> &T {
        unsafe { &*self.cell.get() }
    }

    // SAFETY: Caller must ensure there are no aliasing issues (e.g. no other refs).
    #[inline(always)]
    pub fn as_mut(&self) -> &mut T {
        unsafe { &mut *self.cell.get() }
    }
}

// Implement Deref/DerefMut to allow `&*value` or `value.field` syntax.
impl<T> Deref for Mutable<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T> DerefMut for Mutable<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: Caller must ensure exclusive access (no aliasing).
        self.as_mut()
    }
}

// Serde serialization support.
impl<T> Serialize for Mutable<T> where T: Serialize
{
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        self.as_ref().serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for Mutable<T> where T: Deserialize<'de>
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        T::deserialize(deserializer).map(Mutable::new)
    }
}

impl<T: Default> Default for Mutable<T> {
    #[inline]
    fn default() -> Self {
        Self { cell: UnsafeCell::new(T::default()) }
    }
}

impl<T: Clone> Clone for Mutable<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self { cell: UnsafeCell::new(self.as_ref().clone()) }
    }
}

// ----------------------------------------------
// Low-level type casting helpers
// ----------------------------------------------

#[inline(always)]
pub fn mut_ref_cast<T: ?Sized>(reference: &T) -> &mut T {
    let ptr = reference as *const T as *mut T;
    unsafe { ptr.as_mut().unwrap() }
}

// ----------------------------------------------
// SingleThreadStatic
// ----------------------------------------------

// A single-threaded mutable global static variable.
// Safe as long as only one thread ever touches it.
// If another thread tries, it will panic (not UB).
// First thread to access the instance claims ownership.
pub struct SingleThreadStatic<T> {
    value: UnsafeCell<T>,
    owner: OnceLock<ThreadId>,
}

impl<T> SingleThreadStatic<T> {
    #[inline]
    pub const fn new(value: T) -> Self {
        Self { value: UnsafeCell::new(value), owner: OnceLock::new() }
    }

    #[inline]
    pub fn set(&'static self, value: T) {
        *self.as_mut() = value;
    }

    #[inline]
    pub fn as_ref(&'static self) -> &'static T {
        self.assert_owner();
        unsafe { &*self.value.get() }
    }

    #[inline]
    pub fn as_mut(&'static self) -> &'static mut T {
        self.assert_owner();
        unsafe { &mut *self.value.get() }
    }

    fn assert_owner(&self) {
        if cfg!(debug_assertions) {
            let this_thread = std::thread::current().id();
            match self.owner.get() {
                Some(owner) if *owner == this_thread => {} // Same thread, no action.
                Some(_) => panic!("SingleThreadStatic accessed from non-owner thread!"),
                None => {
                    // First access claims ownership:
                    self.owner
                        .set(this_thread)
                        .unwrap_or_else(|_| panic!("Failed to set owner thread id!"));
                }
            }
        }
    }
}

// SAFETY: Safe to share references because we enforce single-threaded access
// with assert_owner().
unsafe impl<T> Sync for SingleThreadStatic<T> {}

// Implement Deref/DerefMut to allow `&*value` or `value.field` syntax.
impl<T> Deref for SingleThreadStatic<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.assert_owner();
        unsafe { &*self.value.get() }
    }
}

impl<T> DerefMut for SingleThreadStatic<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.assert_owner();
        unsafe { &mut *self.value.get() }
    }
}

// ----------------------------------------------
// Singleton
// ----------------------------------------------

// Singleton with static initialization at compile time.
pub struct Singleton<T> {
    instance: SingleThreadStatic<T>,
}

impl<T> Singleton<T> {
    #[inline]
    pub const fn new(instance: T) -> Self {
        Self { instance: SingleThreadStatic::new(instance) }
    }

    #[inline]
    pub fn as_ref(&'static self) -> &'static T {
        self.instance.as_ref()
    }

    #[inline]
    pub fn as_mut(&'static self) -> &'static mut T {
        self.instance.as_mut()
    }
}

#[macro_export]
macro_rules! singleton {
    ($singleton_name:ident, $singleton_type:ty) => {
        static $singleton_name: $crate::utils::mem::Singleton<$singleton_type> =
            $crate::utils::mem::Singleton::new(<$singleton_type>::new());

        impl $singleton_type {
            #[inline]
            pub fn get() -> &'static $singleton_type {
                $singleton_name.as_ref()
            }

            #[inline]
            pub fn get_mut() -> &'static mut $singleton_type {
                $singleton_name.as_mut()
            }
        }
    };
}

// ----------------------------------------------
// SingletonLateInit
// ----------------------------------------------

// Singleton with deferred initialization. User is responsible for calling
// `initialize()` on the singleton exactly once before it can be used. If drop
// is required before program termination an explicit call to `terminate()`
// must be made.
pub struct SingletonLateInit<T> {
    maybe_instance: SingleThreadStatic<Option<T>>,
    debug_name: &'static str,
}

impl<T> SingletonLateInit<T> {
    #[inline]
    pub const fn new(name: &'static str) -> Self {
        Self { maybe_instance: SingleThreadStatic::new(None), debug_name: name }
    }

    #[inline]
    pub fn initialize(&'static self, instance: T) {
        if self.is_initialized() {
            panic!("Singleton {} is already initialized!", self.debug_name);
        }
        self.maybe_instance.set(Some(instance));
    }

    #[inline]
    pub fn terminate(&'static self) {
        self.maybe_instance.set(None);
    }

    #[inline]
    pub fn is_initialized(&'static self) -> bool {
        self.maybe_instance.is_some()
    }

    #[inline]
    pub fn as_ref(&'static self) -> &'static T {
        if let Some(instance) = self.maybe_instance.as_ref() {
            return instance;
        }
        panic!("Singleton {} is not initialized!", self.debug_name);
    }

    #[inline]
    pub fn as_mut(&'static self) -> &'static mut T {
        if let Some(instance) = self.maybe_instance.as_mut() {
            return instance;
        }
        panic!("Singleton {} is not initialized!", self.debug_name);
    }
}

#[macro_export]
macro_rules! singleton_late_init {
    ($singleton_name:ident, $singleton_type:ty) => {
        static $singleton_name: $crate::utils::mem::SingletonLateInit<$singleton_type> =
            $crate::utils::mem::SingletonLateInit::new(stringify!($singleton_type));

        impl $singleton_type {
            #[inline]
            pub fn initialize(instance: $singleton_type) {
                $singleton_name.initialize(instance);
            }

            #[inline]
            pub fn terminate() {
                $singleton_name.terminate();
            }

            #[inline]
            pub fn get() -> &'static $singleton_type {
                $singleton_name.as_ref()
            }

            #[inline]
            pub fn get_mut() -> &'static mut $singleton_type {
                $singleton_name.as_mut()
            }
        }
    };
}
