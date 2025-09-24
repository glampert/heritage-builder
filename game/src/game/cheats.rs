use proc_macros::DrawDebugUi;

use crate::{
    singleton,
    utils::hash::{self, PreHashedKeyMap, StringHash}
};

// ----------------------------------------------
// CheatsLookup
// ----------------------------------------------

type CheatsLookupMap = PreHashedKeyMap<StringHash, (&'static str, &'static mut bool)>;

pub trait CheatsLookup {
    fn lookup(&self) -> &CheatsLookupMap;
    fn lookup_mut(&mut self) -> &mut CheatsLookupMap;

    fn find_by_name(&self, cheat_name: &str) -> Option<bool> {
        self.find_by_hash(hash::fnv1a_from_str(cheat_name))
    }

    fn find_by_hash(&self, cheat_hash: StringHash) -> Option<bool> {
        self.lookup().get(&cheat_hash).map(|(_, cheat)| **cheat)
    }

    fn try_set_by_name(&mut self, cheat_name: &str, value: bool) -> Result<(), &'static str> {
        self.try_set_by_hash(hash::fnv1a_from_str(cheat_name), value)
    }

    fn try_set_by_hash(&mut self, cheat_hash: StringHash, value: bool) -> Result<(), &'static str> {
        self.lookup_mut().get_mut(&cheat_hash)
            .map(|(_, cheat)| **cheat = value)
            .ok_or("Game cheat not found!")
    }
}

macro_rules! game_cheats {
    (
        $($cheat_name:ident = $default_val:literal),* $(,)?
    ) => {
        #[derive(DrawDebugUi)]
        pub struct Cheats {
            #[debug_ui(skip)]
            lookup: CheatsLookupMap,
            $(
                #[debug_ui(edit)]
                pub $cheat_name: bool,
            )*
        }

        impl Cheats {
            const fn new() -> Self {
                Self {
                    lookup: hash::new_const_hash_map(),
                    $(
                        $cheat_name: $default_val,
                    )*
                }
            }

            fn insert(lookup: &mut CheatsLookupMap, name: &'static str, cheat: &'static mut bool) {
                let _ = lookup.insert(hash::fnv1a_from_str(name), (name, cheat))
                    .is_none_or(|(name, _)| panic!("Game cheat '{name}' registered twice!"));
            }

            fn build_lookup(&'static mut self) {
                $(
                    Self::insert(&mut self.lookup, stringify!($cheat_name), &mut self.$cheat_name);
                )*
            }
        }

        impl CheatsLookup for Cheats {
            #[inline] fn lookup(&self) -> &CheatsLookupMap { &self.lookup }
            #[inline] fn lookup_mut(&mut self) -> &mut CheatsLookupMap { &mut self.lookup }
        }
    }
}

// ----------------------------------------------
// Game Cheats
// ----------------------------------------------

game_cheats! {
    ignore_worker_requirements = false,
    ignore_tile_cost = false,
}

// ----------------------------------------------
// Global Instance
// ----------------------------------------------

singleton! { CHEATS_SINGLETON, Cheats }

pub fn initialize() {
    Cheats::get_mut().build_lookup();
}

pub fn get() -> &'static Cheats {
    Cheats::get()
}

pub fn get_mut() -> &'static mut Cheats {
    Cheats::get_mut()
}
