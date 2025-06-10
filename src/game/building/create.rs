use crate::{
    utils::{Cell}
};

use super::{
    Building,
    BuildingKind,
    BuildingArchetype,
    service::ServiceState,
    household::HouseholdState
};

pub fn new_well(map_cell: Cell) -> Building {
    Building::new(
        "Well".to_string(),
        map_cell,
        BuildingKind::Well,
        BuildingArchetype::new_service(ServiceState::new())
    )
}

pub fn new_market(map_cell: Cell) -> Building {
    Building::new(
        "Market".to_string(),
        map_cell,
        BuildingKind::Market,
        BuildingArchetype::new_service(ServiceState::new())
    )
}

pub fn new_household(map_cell: Cell) -> Building {
    Building::new(
        "Household".to_string(),
        map_cell,
        BuildingKind::Household,
        BuildingArchetype::new_household(HouseholdState::new())
    )
}
