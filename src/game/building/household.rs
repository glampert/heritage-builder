use strum::{EnumCount, EnumProperty};
use strum_macros::{EnumCount, EnumProperty};
use num_enum::{IntoPrimitive, TryFromPrimitive};

use super::{
    BuildingKind,
    BuildingUpdateContext
};

// ----------------------------------------------
// HouseholdState
// ----------------------------------------------

pub struct HouseholdState {
    upgrade: UpgradeState,
}

impl HouseholdState {
    pub fn new() -> Self {
        Self {
            upgrade: UpgradeState::new(),
        }
    }

    pub fn update(&mut self, update_ctx: &mut BuildingUpdateContext, delta_time_secs: f32) {
        let has_water =
            update_ctx.query.is_near_building(update_ctx.map_cell, BuildingKind::Well, 3);

        let has_food =
            update_ctx.query.is_near_building(update_ctx.map_cell, BuildingKind::Market, 5);

        self.upgrade.requirements.has_water = has_water;
        self.upgrade.requirements.has_food  = has_food;

        if self.can_upgrade() {
            self.try_upgrade(update_ctx);
        } else {
            self.upgrade.time_since_last_secs += delta_time_secs;
        }
    }

    fn can_upgrade(&self) -> bool {
        if self.upgrade.level.is_max() {
            return false;
        }

        if self.upgrade.time_since_last_secs < self.upgrade.frequency_secs {
            return false;
        }

        self.upgrade.requirements.has_food && self.upgrade.requirements.has_water
    }

    // TODO:
    // - Merge neighboring houses into larger ones when upgrading.
    // - Downgrade back to smaller house when requirements are missing.

    fn try_upgrade(&mut self, update_ctx: &mut BuildingUpdateContext) {
        let mut tile_placed_successfully = false;
        let new_tile_def_name = self.upgrade.level.next().tile_def_name();

        if let Some(new_tile_def) = 
            update_ctx.find_tile_def(HOUSE_TILE_CATEGORY_NAME, new_tile_def_name) {

            // Try placing new. Might fail if there isn't enough space.
            if update_ctx.try_replace_tile(new_tile_def) {

                self.upgrade.level.upgrade();
                tile_placed_successfully = true;

                // Set a random variation for the new building:
                update_ctx.set_random_variation();

                println!("{}: upgraded to level {:?}.", update_ctx, self.upgrade.level);
            }
        }

        if !tile_placed_successfully {
            println!("{}: Failed to place new tile for upgrade. Building cannot upgrade.", update_ctx);
        }

        self.upgrade.time_since_last_secs = 0.0;
    }
}

// ----------------------------------------------
// UpgradeRequirements
// ----------------------------------------------

struct UpgradeRequirements {
    has_water: bool,
    has_food: bool, 
}

impl UpgradeRequirements {
    fn new() -> Self {
        Self {
            has_water: false,
            has_food: false
        }
    }
}

// ----------------------------------------------
// UpgradeState
// ----------------------------------------------

struct UpgradeState {
    level: HouseLevel,
    frequency_secs: f32,
    time_since_last_secs: f32,
    requirements: UpgradeRequirements,
}

impl UpgradeState {
    fn new() -> Self {
        Self {
            level: HouseLevel::Level0,
            frequency_secs: HOUSE_UPGRADE_FREQUENCY_SECS,
            time_since_last_secs: 0.0,
            requirements: UpgradeRequirements::new()
        }
    }
}

// ----------------------------------------------
// HouseLevel
// ----------------------------------------------

const HOUSE_LEVEL_COUNT: usize = HouseLevel::COUNT;
const HOUSE_TILE_CATEGORY_NAME: &str = "households";
const HOUSE_UPGRADE_FREQUENCY_SECS: f32 = 10.0;

#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, EnumProperty, EnumCount, IntoPrimitive, TryFromPrimitive)]
enum HouseLevel {
    #[strum(props(TileDef = "house_0"))]
    Level0,

    #[strum(props(TileDef = "house_1"))]
    Level1,
}

impl HouseLevel {
    #[inline]
    fn tile_def_name(self) -> &'static str {
        self.get_str("TileDef").unwrap()
    }

    #[inline]
    fn is_max(self) -> bool {
        let curr: u32 = self.into();
        if (curr as usize) == (HOUSE_LEVEL_COUNT - 1) {
            true
        } else {
            false
        }
    }

    #[inline]
    fn is_min(self) -> bool {
        let curr: u32 = self.into();
        if curr == 0 {
            true
        } else {
            false
        }
    }

    #[inline]
    #[must_use]
    fn next(self) -> HouseLevel {
        let curr: u32 = self.into();
        let next = curr + 1;
        HouseLevel::try_from(next).expect("Max HouseLevel exceeded!")
    }

    #[inline]
    #[must_use]
    fn prev(self) -> HouseLevel {
        let curr: u32 = self.into();
        let next = curr - 1;
        HouseLevel::try_from(next).expect("Min HouseLevel exceeded!")
    }

    #[inline]
    fn upgrade(&mut self) {
        *self = self.next();
    }

    #[inline]
    fn downgrade(&mut self) {
        *self = self.prev();
    }
}
