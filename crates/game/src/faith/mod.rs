// Faith and festivals (docs/plans/faith.md, roadmap M8): each realm's patrons, their favour, the
// blessings and misfortunes it brings, and the realm's festival year. The data is `config`, one
// file per realm; the game's state is `sim::faith::CityFaith`, saved in the simulation.

pub mod config;

pub use config::{FaithConfigs, PatronDef, RealmFaithDef};
use engine::ui::text::{self, UiTextCategory};
use strum::{Display, EnumIter, EnumString};

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
