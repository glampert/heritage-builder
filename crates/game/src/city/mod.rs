// What the City book shows of the city (docs/plans/realm-ui.md, Phase 6): a report read from the
// world, the simulation and the configs when the book is built (report.rs), and a short history
// sampled once a game month (history.rs) for its trend lines and monthly changes.

pub mod food;
pub mod history;
pub mod report;

pub use food::FoodReport;
pub use history::{CityHistory, MonthFlow, MonthSample};
pub use report::CityReport;

// A name the configs spell in camel case ("SmallWell", "RoofTiles"), as the player reads it:
// "Small well".
pub fn spaced_name(name: &str) -> String {
    let mut spaced = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if i != 0 && c.is_ascii_uppercase() {
            spaced.push(' ');
            spaced.push(c.to_ascii_lowercase());
        } else {
            spaced.push(c);
        }
    }
    spaced
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_case_names_read_as_words() {
        assert_eq!(spaced_name("SmallWell"), "Small well");
        assert_eq!(spaced_name("Market"), "Market");
        assert_eq!(spaced_name("PoliceStation"), "Police station");
    }
}
