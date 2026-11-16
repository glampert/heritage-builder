// Faith and festivals (docs/plans/faith.md, roadmap M8): each realm's patrons, their favour, the
// blessings and misfortunes it brings, and the realm's festival year. The data is `config`, one
// file per realm; the game's state is `sim::faith::CityFaith`, saved in the simulation; `events`
// rolls and sets off the blessings and misfortunes.

pub mod config;
pub mod events;

use std::collections::BTreeMap;

use common::coords::CellRange;
pub use config::{FaithConfigs, PatronDef, RealmFaithDef};
use engine::ui::text::{self, UiTextCategory};
use strum::{Display, EnumIter, EnumString};

use crate::{
    building::{Building, BuildingKind, config::BuildingConfigs, footprints_within_reach},
    config::FaithRules,
    realm::Realm,
    sim::SimContext,
    tile::{TileKind, TileMap},
    world::World,
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
// Coverage
// ----------------------------------------------

// How well a patron is tended (§2): its finished shrines and temples, the residents its working
// ones reach, and its coverage, 0 to 1.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct PatronCoverage {
    pub shrines: u32,
    pub temples: u32,
    pub residents: u32, // In the houses its working shrines and temples reach.
    pub coverage: f32,
}

// Each patron of `realm`'s coverage, by id (decisions Q2, Q3): the residents of the houses within
// the effect radius of its working shrines and temples, along the grid, a temple counting
// `temple_weight` times a shrine and a house reached by several counting once at its best weight;
// over the city's population times `coverage_needed`, capped at 1. A shrine or temple works while
// it's operational (a temple needs its priests); one under construction counts for nothing.
pub fn coverage(world: &World, tile_map: &TileMap, realm: Realm, rules: &FaithRules) -> BTreeMap<String, PatronCoverage> {
    let Some(def) = FaithConfigs::get().for_realm(realm) else {
        return BTreeMap::new();
    };
    let mut patrons: Vec<PatronCoverage> = vec![PatronCoverage::default(); def.patrons.len()];

    // The working shrines and temples: (patron index, footprint, radius, weight).
    let mut tenders: Vec<(usize, CellRange, i32, f32)> = Vec::new();
    world.for_each_building(DEDICATED_KINDS, |building| {
        if building.is_under_construction() {
            return true;
        }
        let Some(index) = dedicated_patron_in(realm, tile_map, building)
            .and_then(|patron| def.patrons.iter().position(|other| other.id == patron.id))
        else {
            return true;
        };
        let is_temple = building.is(BuildingKind::Temple);
        if is_temple {
            patrons[index].temples += 1;
        } else {
            patrons[index].shrines += 1;
        }
        if building.is_operational() {
            let radius = BuildingConfigs::get().find_service_config(building.kind()).effect_radius;
            let weight = if is_temple { rules.temple_weight } else { 1.0 };
            tenders.push((index, building.cell_range(), radius, weight));
        }
        true
    });

    let mut population = 0_u32;
    let mut weighted = vec![0.0_f32; patrons.len()];
    world.for_each_building(BuildingKind::House, |house| {
        let residents = house.population_count();
        if house.is_under_construction() || residents == 0 {
            return true;
        }
        population += residents;
        let mut best = vec![0.0_f32; patrons.len()];
        for (index, footprint, radius, weight) in &tenders {
            if footprints_within_reach(*footprint, house.cell_range(), *radius) {
                best[*index] = best[*index].max(*weight);
            }
        }
        for (index, weight) in best.iter().enumerate().filter(|(_, weight)| **weight > 0.0) {
            weighted[index] += residents as f32 * weight;
            patrons[index].residents += residents;
        }
        true
    });

    let needed = population as f32 * rules.coverage_needed;
    for (patron, weighted) in patrons.iter_mut().zip(weighted) {
        patron.coverage = if needed > 0.0 { (weighted / needed).min(1.0) } else { 0.0 };
    }
    def.patrons.iter().map(|patron| patron.id.clone()).zip(patrons).collect()
}

// The city's population, as coverage() counts it: the residents of finished houses.
pub fn population(world: &World) -> u32 {
    let mut population = 0;
    world.for_each_building(BuildingKind::House, |house| {
        if !house.is_under_construction() {
            population += house.population_count();
        }
        true
    });
    population
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
