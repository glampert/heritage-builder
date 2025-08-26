use crate::{
    utils::Seconds
};

// Pool allocation hints: These are not hard limits, pools will resize when needed.

// Buildings:
pub const PRODUCER_BUILDINGS_POOL_CAPACITY: usize = 32;
pub const STORAGE_BUILDINGS_POOL_CAPACITY:  usize = 32;
pub const SERVICE_BUILDINGS_POOL_CAPACITY:  usize = 128;
pub const HOUSE_BUILDINGS_POOL_CAPACITY:    usize = 256;

// Units:
pub const UNIT_SPAWN_POOL_CAPACITY: usize = 512;
pub const UNIT_TASK_POOL_CAPACITY:  usize = UNIT_SPAWN_POOL_CAPACITY * 2;

// Simulation:
pub const DEFAULT_RANDOM_SEED: u64 = 0xCAFE1CAFE2CAFE3A;
pub const DEFAULT_SIM_UPDATE_FREQUENCY_SECS: Seconds = 0.5;
