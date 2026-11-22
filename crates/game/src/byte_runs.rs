// Saves a grid of bytes, row by row, as runs ([value, count], ...), for grids where most cells share
// a few values: the floods' fertility (sim::floods) and the terrain's heights (tile::elevation).
// Use with `#[serde(with = "crate::byte_runs")]` on a `Vec<u8>` field.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize<S: Serializer>(cells: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    let mut runs: Vec<[u32; 2]> = Vec::new();
    for &value in cells {
        match runs.last_mut() {
            Some([last, count]) if *last == value as u32 => *count += 1,
            _ => runs.push([value as u32, 1]),
        }
    }
    runs.serialize(serializer)
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let runs: Vec<[u32; 2]> = Vec::deserialize(deserializer)?;
    let mut cells = Vec::with_capacity(runs.iter().map(|[_, count]| *count as usize).sum());
    for [value, count] in runs {
        cells.extend(std::iter::repeat_n(value.min(u8::MAX as u32) as u8, count as usize));
    }
    Ok(cells)
}
