// Faith and festivals (docs/plans/faith.md, roadmap M8): each realm's patrons, their favour, the
// blessings and misfortunes it brings, and the realm's festival year. The data is `config`, one
// file per realm; the game's state is `sim::faith::CityFaith`, saved in the simulation.

pub mod config;

pub use config::{FaithConfigs, PatronDef, RealmFaithDef};
use engine::ui::text::{self, UiTextCategory};
use strum::{Display, EnumIter, EnumString};

use crate::{
    building::{Building, BuildingKind},
    realm::Realm,
    sim::SimContext,
    tile::{TileKind, TileMap},
};

// The building kinds dedicated to a patron when they're built (§2a).
pub const DEDICATED_KINDS: BuildingKind = BuildingKind::Shrine.union(BuildingKind::Temple);

// The patron a shrine or a temple is dedicated to: its tile's variation, named by the patron's id
// (docs/plans/faith.md, §2a). None for other buildings, and for one whose variation names no patron
// of the game's realm (a Kambara shrine before its art exists).
pub fn dedicated_patron(context: &SimContext, building: &Building) -> Option<&'static PatronDef> {
    dedicated_patron_in(context.realm(), context.tile_map(), building)
}

// dedicated_patron(), for code without a SimContext: the game's realm and its tile map.
pub fn dedicated_patron_in(realm: Realm, tile_map: &TileMap, building: &Building) -> Option<&'static PatronDef> {
    if !building.is(DEDICATED_KINDS) {
        return None;
    }
    let tile = tile_map.find_tile(building.base_cell(), TileKind::Building)?;
    FaithConfigs::get().for_realm(realm)?.patron(tile.variation_name())
}

// ----------------------------------------------
// Mood
// ----------------------------------------------

// What a patron's favour says (decision Q5): the Heritage page's and the inspector's word for it.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Display, EnumIter, EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum Mood {
    Pleased,
    Content,
    Watchful,
    Restless,
    Neglected,
}

impl Mood {
    // Pleased from 65, Content from 50, Watchful from 40, Restless from 30, Neglected below.
    pub fn from_favour(favour: f32) -> Self {
        if favour >= 65.0 {
            Self::Pleased
        } else if favour >= 50.0 {
            Self::Content
        } else if favour >= 40.0 {
            Self::Watchful
        } else if favour >= 30.0 {
            Self::Restless
        } else {
            Self::Neglected
        }
    }

    // The word, in the current language (`faith.mood.<mood>`, Menu).
    pub fn display_name(self) -> String {
        text::get(UiTextCategory::Menu, &format!("faith.mood.{self}")).to_string()
    }
}
