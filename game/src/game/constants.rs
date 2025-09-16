// Pool allocation hints: These are not hard limits, pools will resize when needed.

// Buildings:
pub const PRODUCER_BUILDINGS_POOL_CAPACITY: usize = 32;
pub const STORAGE_BUILDINGS_POOL_CAPACITY:  usize = 32;
pub const SERVICE_BUILDINGS_POOL_CAPACITY:  usize = 128;
pub const HOUSE_BUILDINGS_POOL_CAPACITY:    usize = 256;

// Units:
pub const UNIT_SPAWN_POOL_CAPACITY: usize = 512;
pub const UNIT_TASK_POOL_CAPACITY:  usize = UNIT_SPAWN_POOL_CAPACITY * 2;

// We reserve generation 0 as a sentinel value to detect uninitialized deserialized data.
pub const INITIAL_GENERATION:  u32 = 1;
pub const RESERVED_GENERATION: u32 = 0;
