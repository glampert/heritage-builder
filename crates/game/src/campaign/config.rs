use serde::{Deserialize, Serialize};

use common::hash;
use engine::{log, ui::text::{self, UiTextCategory}};

pub use crate::world_map::config::{CityKind, LabelSide};
use crate::{
    realm::Realm,
    sim::{calendar::EraDef, finance::TaxLevel, resources::ResourceKind},
    world::stats::WorldStats,
};

// ----------------------------------------------
// Campaign / Mission Definitions (data-driven)
// ----------------------------------------------

// Top-level campaign configs, loaded from `assets/configs/game/campaigns.json`.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CampaignConfigs {
    pub campaigns: Vec<CampaignDef>,
}

// A single campaign: an ordered sequence of missions.
#[derive(Serialize, Deserialize)]
pub struct CampaignDef {
    // Names the campaign in the player's profile, so their progress survives reordering this
    // file. One left out becomes "campaign_<index>" when the configs load.
    #[serde(default)]
    pub id: String,
    pub name: String,
    // The realm whose look the UI takes while the campaign is played, and in the menus once the
    // campaign is the latest reached (see realm::ui_realm).
    #[serde(default)]
    pub realm: Realm,
    pub missions: Vec<MissionDef>,
    // The realm's eras, in the order they start: the calendar counts years from the latest.
    #[serde(default)]
    pub eras: Vec<EraDef>,
    // A test campaign that only automation reaches (profile.set): never listed by the home menu,
    // the campaign picker or Custom Game, never the home menu's realm, never reached by finishing
    // another campaign, and left out of the campaigns' numbering ("Campaign II"). Keep hidden
    // campaigns after the others: session saves name a campaign by its index in this file.
    #[serde(default)]
    pub hidden: bool,
    // The world map it shows (assets/configs/game/world/<name>.json, crate::world_map). None: the
    // one named by its id.
    #[serde(default)]
    pub world_map: Option<String>,
    // The title carved at the head of its stele (the debrief's and the campaign-complete screen's:
    // crate::campaign::stele), a Story key, e.g. "stele.dragon_legacy.title". Required once it has
    // missions (validate).
    #[serde(default)]
    pub stele_title: Option<String>,
    // The closing line the campaign-complete screen shows under its name, a Story key. Required
    // once it has missions.
    #[serde(default)]
    pub stele_closing: Option<String>,
}

impl CampaignDef {
    // Its stele's title as the player reads it (a Story key's text), else its name.
    pub fn stele_title_text(&self) -> &str {
        self.stele_title.as_deref().map_or_else(|| self.display_name(), |key| text::get(UiTextCategory::Story, key))
    }

    // Its name as the player reads it: the Realm string `campaign.<id>` (en.json), else its `name`.
    pub fn display_name(&self) -> &str {
        let key = format!("campaign.{}", self.id);
        text::find_str(UiTextCategory::Realm, hash::fnv1a_from_str(&key)).unwrap_or(&self.name)
    }

    // The name of its mission at `mission_index` as the player reads it: the Realm string
    // `mission.<id>.<number>` (numbered from 1), else the mission's `name`. None if it has no such
    // mission.
    pub fn mission_display_name(&self, mission_index: usize) -> Option<&str> {
        let mission = self.missions.get(mission_index)?;
        let key = format!("mission.{}.{}", self.id, mission_index + 1);
        Some(text::find_str(UiTextCategory::Realm, hash::fnv1a_from_str(&key)).unwrap_or(&mission.name))
    }

    // The world map file it shows: `world_map`, else its id.
    pub fn world_map_name(&self) -> &str {
        self.world_map.as_deref().unwrap_or(&self.id)
    }

    // ----------------------
    // Missions and choices:
    // ----------------------
    //
    // A mission with `choices` is a choice point: the world map offers its sites, and each starts
    // the mission it names, the choice point itself or a variant (a mission only another's choice
    // starts). A step is a mission that isn't a variant; the campaign plays its steps in order, and
    // a step is finished once it or one of the missions its sites start is.

    // The index of the mission `mission` names: its index, or its id. None if there's no such mission.
    pub fn resolve_mission(&self, mission: &MissionRef) -> Option<usize> {
        match mission {
            MissionRef::Index(index) => (*index < self.missions.len()).then_some(*index),
            MissionRef::Id(id) => self.missions.iter().position(|mission| !mission.id.is_empty() && mission.id == *id),
        }
    }

    // The missions the choice point at `mission_index` starts, by index (the ones that resolve).
    pub fn choice_targets(&self, mission_index: usize) -> Vec<usize> {
        self.missions.get(mission_index).map_or_else(Vec::new, |mission| {
            mission.choices.iter().filter_map(|choice| self.resolve_mission(&choice.mission)).collect()
        })
    }

    // The mission the site `site` of the choice point at `step` starts. None for a site the step
    // doesn't offer, or one that names no mission.
    pub fn site_mission(&self, step: usize, site: ChoiceSite) -> Option<usize> {
        let choice = self.missions.get(step)?.choices.iter().find(|choice| choice.site == site)?;
        self.resolve_mission(&choice.mission)
    }

    // The site of the choice point at `step` that starts the mission at `mission_index`, if one does.
    pub fn site_starting(&self, step: usize, mission_index: usize) -> Option<ChoiceSite> {
        self.missions
            .get(step)?
            .choices
            .iter()
            .find(|choice| self.resolve_mission(&choice.mission) == Some(mission_index))
            .map(|choice| choice.site)
    }

    // The mission to play at the step `step`: the mission of the site picked there (`picked`), else
    // the step itself, which one of a choice point's sites starts (validate checks it).
    pub fn step_mission(&self, step: usize, picked: Option<ChoiceSite>) -> usize {
        picked.and_then(|site| self.site_mission(step, site)).unwrap_or(step)
    }

    // The step a mission belongs to: the choice point whose site starts it, for a variant; else
    // the mission itself.
    pub fn step_of(&self, mission_index: usize) -> usize {
        (0..self.missions.len())
            .find(|&step| step != mission_index && self.choice_targets(step).contains(&mission_index))
            .unwrap_or(mission_index)
    }

    // Whether the mission is a variant: only another mission's choice starts it.
    pub fn is_variant(&self, mission_index: usize) -> bool {
        self.step_of(mission_index) != mission_index
    }

    // The campaign's steps (its missions that aren't variants), by index, in order.
    pub fn steps(&self) -> Vec<usize> {
        (0..self.missions.len()).filter(|&index| !self.is_variant(index)).collect()
    }

    // Whether the step at `step` is finished, given the missions finished: it, or a mission one of
    // its sites starts.
    pub fn is_step_finished(&self, step: usize, missions_finished: &[usize]) -> bool {
        missions_finished.contains(&step) || self.choice_targets(step).iter().any(|target| missions_finished.contains(target))
    }

    // The first step after the step of `mission_index`. None after the last.
    pub fn next_step_after(&self, mission_index: usize) -> Option<usize> {
        let step = self.step_of(mission_index);
        self.steps().into_iter().find(|&index| index > step)
    }

    // The mission's number as the player reads it, from 1: its step's place among the steps (a
    // variant takes its choice point's).
    pub fn mission_number(&self, mission_index: usize) -> usize {
        let step = self.step_of(mission_index);
        self.steps().iter().position(|&index| index == step).map_or(mission_index + 1, |position| position + 1)
    }

    // How many missions the player plays through: the steps.
    pub fn mission_count(&self) -> usize {
        self.steps().len()
    }

    // Whether finishing the mission at `mission_index` finishes the campaign: it belongs to the
    // last step.
    pub fn is_last_step(&self, mission_index: usize) -> bool {
        self.steps().last().is_some_and(|&last| last == self.step_of(mission_index))
    }
}

// A single mission: a starting map plus the requirements to complete it.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionDef {
    // Names the mission for another's choices (MissionRef::Id). Optional.
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub map: MissionMap,
    #[serde(default)]
    pub requirements: MissionRequirements,
    // When the mission starts, and its seasons. None: the defaults from SimConfigs::calendar.
    #[serde(default)]
    pub calendar: Option<MissionCalendarDef>,
    // The treasury, tax level and wage the mission starts with. None: the map's own.
    #[serde(default)]
    pub finance: Option<MissionFinanceDef>,
    // Whether fire and collapse risks build up. None: RiskConfigs::enabled.
    #[serde(default)]
    pub risks: Option<MissionRisksDef>,
    // The city the mission founds, on its campaign's world map: the player's once the mission is
    // finished, and while it's played. None: it founds none.
    #[serde(default)]
    pub city: Option<MissionCityDef>,
    // The sites the world map offers when this mission is next (see CampaignDef: Missions and
    // choices). Empty: no choice.
    #[serde(default)]
    pub choices: Vec<MissionChoiceDef>,
    // The world map's choice box's second line for those sites: a Menu key, its {a} and {b} the
    // sites' names (their `phrase`). None: world_map.choice.subtitle ("Choose {a} or {b}").
    #[serde(default)]
    pub choice_text: Option<String>,
    // The line carved on the campaign's stele once it's finished (crate::campaign::stele), a Story
    // key, e.g. "mission.dragon_legacy.1.stele". Required in a campaign (validate); a variant may
    // leave it out and take its choice point's.
    #[serde(default)]
    pub stele_line: Option<String>,
    // The briefing's lead-in, the line the stele will hold quoted above its text, a Story key.
    // Required of every mission, variants included.
    #[serde(default)]
    pub lead_in: Option<String>,
    // The briefing's text, a Story key (paragraphs apart by an empty line). Required of every
    // mission, variants included; `description` is the developers' note.
    #[serde(default)]
    pub briefing: Option<String>,
    // The briefing's "New in this mission" list (what the mission brings: buildings, goods, rules)
    // and its "Threats" line, Story keys. Optional: the briefing leaves out a section it lacks.
    #[serde(default)]
    pub briefing_new: Vec<String>,
    #[serde(default)]
    pub briefing_threats: Option<String>,
    // The distinctions the mission can earn and the line each adds to the stele once earned
    // (docs/plans/realm-ui.md, Phase 3: Stele). Empty until M9 defines distinctions and awards
    // them into the profile (CampaignRecord::distinctions, by id).
    // TODO(realm-ui): M9 fills these in, with the debrief's distinctions table.
    #[serde(default)]
    pub distinctions: Vec<MissionDistinctionDef>,
}

impl MissionDef {
    // The name of the city it founds, as the player reads it. None if it founds none.
    pub fn city_display_name(&self) -> Option<&str> {
        self.city.as_ref().map(|city| text::get(UiTextCategory::Realm, &city.name))
    }

    // The briefing's text as the player reads it: `briefing`'s, else the developers' `description`.
    pub fn briefing_text(&self) -> &str {
        self.briefing.as_deref().map_or(&self.description, |key| text::get(UiTextCategory::Story, key))
    }

    // The briefing's lead-in as the player reads it, if it has one.
    pub fn lead_in_text(&self) -> Option<&str> {
        self.lead_in.as_deref().map(|key| text::get(UiTextCategory::Story, key))
    }

    // The Story keys it names: its stele line, lead-in, briefing (its text, what's new and its
    // threats) and distinctions' lines.
    pub fn story_keys(&self) -> impl Iterator<Item = &str> {
        [&self.stele_line, &self.lead_in, &self.briefing, &self.briefing_threats]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .chain(self.briefing_new.iter().map(String::as_str))
            .chain(self.distinctions.iter().map(|distinction| distinction.stele_line.as_str()))
    }
}

// A distinction a mission can earn (M9), and the stele line it adds.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionDistinctionDef {
    // Names it in the profile (CampaignRecord::distinctions): unique in its campaign.
    pub id: String,
    // The line it adds to the stele, after its mission's: a Story key.
    pub stele_line: String,
}

// The city a mission founds, on its campaign's world map (crate::world_map).
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionCityDef {
    pub id: String,
    // Its name: a Realm string key (en.json), e.g. "city.phu_sa".
    pub name: String,
    // In the world map's coordinates (WorldMapDef::extent).
    pub position: [f32; 2],
    // Where its name goes beside its mark.
    #[serde(default)]
    pub label: LabelSide,
    #[serde(default = "default_city_kind")]
    pub kind: CityKind,
}

fn default_city_kind() -> CityKind {
    CityKind::City
}

// One of a choice point's sites.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionChoiceDef {
    pub site: ChoiceSite,
    // The site's name: a Realm string key, e.g. "city.hoi_thuy".
    pub name: String,
    // Its name as the choice box's line words it ("royal port" in "The royal port or the upriver
    // town"): a Realm key. None: `name`.
    #[serde(default)]
    pub phrase: Option<String>,
    // In the world map's coordinates.
    pub position: [f32; 2],
    // Where its name goes beside its mark, as MissionCityDef::label.
    #[serde(default)]
    pub label: LabelSide,
    // The mission picking it starts: the choice point itself or a variant.
    pub mission: MissionRef,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize, strum::Display)]
pub enum ChoiceSite {
    A,
    B,
}

// A mission by its index in the campaign or by its id.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MissionRef {
    Index(usize),
    Id(String),
}

// The calendar a mission starts with. Fields left out keep SimConfigs::calendar's.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MissionCalendarDef {
    // World Year and month (1 to 12) of the mission's first day.
    pub start_year: Option<u32>,
    pub start_month: Option<u32>,
    // Months (1 to 12) of the wet season.
    pub wet_months: Option<Vec<u32>>,
}

// The finances a mission starts with: a new treasury with fresh accounts and no debt. Fields
// left out take FinanceConfigs' defaults, and the map's balance if `starting_gold` is.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MissionFinanceDef {
    pub starting_gold: Option<u32>,
    pub tax_level: Option<TaxLevel>,
    // Gold per employed worker per year.
    pub wage: Option<u32>,
}

// City risks in a mission (docs/plans/city-risks.md). Missions that come before the fire watch
// and the builders' guild turn them off; `disease` (on unless set) leaves houses falling sick out
// while keeping fires and collapses.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MissionRisksDef {
    pub enabled: Option<bool>,
    pub disease: Option<bool>,
}

// How a mission's starting map is loaded.
#[derive(Clone, Serialize, Deserialize)]
pub enum MissionMap {
    // PRIMARY: load an existing map through the save-game framework.
    SaveGame { save_file: String },
    // Dev/testing fallback: a built-in preset map (see debug/preset_maps.rs).
    Preset { preset_number: usize },
}

// A mission is complete once every goal in this list is satisfied.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MissionRequirements {
    pub goals: Vec<MissionGoal>,
}

// A single completion goal. Extend by adding a variant + a match arm in `is_met`.
#[derive(Serialize, Deserialize)]
pub enum MissionGoal {
    Population { min: u32 },
    Employment { min_employed: u32 },
    Treasury   { min_gold: u32 },
    Resource   { kind: ResourceKind, min: u32 },
}

impl MissionGoal {
    // Evaluate this goal against the current world stats. `WorldStats` already
    // aggregates population, employment, treasury total and resource counts.
    pub fn is_met(&self, stats: &WorldStats) -> bool {
        match self {
            Self::Population { min } => stats.population.total >= *min,
            Self::Employment { min_employed } => stats.population.employed >= *min_employed,
            Self::Treasury   { min_gold } => stats.treasury.gold_units_total >= i64::from(*min_gold),
            Self::Resource   { kind, min } => stats.resources.all.count(*kind) >= *min,
        }
    }
}

impl MissionRequirements {
    // All goals must be satisfied. An empty goal list is considered already met.
    pub fn all_met(&self, stats: &WorldStats) -> bool {
        self.goals.iter().all(|goal| goal.is_met(stats))
    }
}

impl CampaignConfigs {
    #[inline]
    pub fn campaign(&self, campaign_id: usize) -> Option<&CampaignDef> {
        self.campaigns.get(campaign_id)
    }

    #[inline]
    pub fn mission(&self, campaign_id: usize, mission_index: usize) -> Option<&MissionDef> {
        self.campaign(campaign_id)?.missions.get(mission_index)
    }

    // The index of the campaign with this id (CampaignDef::id).
    pub fn find_by_id(&self, id: &str) -> Option<usize> {
        self.campaigns.iter().position(|campaign| campaign.id == id)
    }

    // The campaigns the player can see (not hidden), with their index in the file, in order.
    pub fn visible(&self) -> impl Iterator<Item = (usize, &CampaignDef)> {
        self.campaigns.iter().enumerate().filter(|(_, campaign)| !campaign.hidden)
    }

    // The campaign's number as the menus write it ("Campaign II"), from 1, counting the visible
    // campaigns only. None for a hidden campaign or no such campaign.
    pub fn campaign_number(&self, campaign_index: usize) -> Option<usize> {
        let campaign = self.campaign(campaign_index)?;
        (!campaign.hidden).then(|| self.campaigns[..campaign_index].iter().filter(|other| !other.hidden).count() + 1)
    }

    // The visible campaign after the one at `campaign_index`, which finishing it reaches. None after
    // the last, and after a hidden campaign.
    pub fn next_visible_after(&self, campaign_index: usize) -> Option<usize> {
        if self.campaign(campaign_index)?.hidden {
            return None;
        }
        self.visible().map(|(index, _)| index).find(|&index| index > campaign_index)
    }

    // Called once the configs load (shadows engine::config::Configs::post_load): every campaign
    // gets an id, since the profile records progress by id; missions and choices are checked.
    fn post_load(&'static mut self) {
        for id in assign_campaign_ids(&mut self.campaigns) {
            log::error!(log::channel!("campaign"), "Two campaigns have the id '{id}': the profile can't tell them apart.");
        }
        for error in self.validate() {
            log::error!(log::channel!("campaign"), "campaigns.json: {error}");
        }
    }

    // What's wrong with the campaigns' missions, cities and choices: a mission id given twice, a
    // choice point without two sites (A and B) or without one that starts it itself, a site naming
    // no mission or a choice point's variant, a mission two choice points start, a variant that's a choice point itself, two
    // mission cities with one id, a hidden campaign before a visible one; a campaign with missions
    // and no stele title or closing line, a mission without its lead-in or briefing, a step (a
    // mission that isn't a variant) without its stele line, a distinction id given twice in a
    // campaign. Empty when all is well. (That the keys are in en.json, the tests check: tests/campaign.rs.)
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut hidden_seen = false;
        for campaign in &self.campaigns {
            let name = &campaign.id;
            if campaign.hidden {
                hidden_seen = true;
            } else if hidden_seen {
                errors.push(format!("Campaign '{name}' comes after a hidden campaign: hidden campaigns go last, as saves name campaigns by index."));
            }
            for (index, mission) in campaign.missions.iter().enumerate() {
                if !mission.id.is_empty() && campaign.missions[..index].iter().any(|other| other.id == mission.id) {
                    errors.push(format!("Campaign '{name}' has two missions with the id '{}'.", mission.id));
                }
                if let Some(city) = &mission.city
                    && campaign.missions[..index].iter().any(|other| other.city.as_ref().is_some_and(|other| other.id == city.id))
                {
                    errors.push(format!("Campaign '{name}': two missions found the city '{}'.", city.id));
                }
                if mission.choices.is_empty() {
                    continue;
                }
                let sites: Vec<ChoiceSite> = mission.choices.iter().map(|choice| choice.site).collect();
                if sites != [ChoiceSite::A, ChoiceSite::B] {
                    errors.push(format!("Campaign '{name}', mission {index}: a choice offers two sites, A then B, not {sites:?}."));
                }
                if !campaign.choice_targets(index).contains(&index) {
                    errors.push(format!("Campaign '{name}', mission {index}: no site starts the choice point itself, which the campaign resumes at while no site is picked."));
                }
                for choice in &mission.choices {
                    match campaign.resolve_mission(&choice.mission) {
                        None => errors.push(format!("Campaign '{name}', mission {index}: site {} names no mission ({:?}).", choice.site, choice.mission)),
                        Some(target) if target != index && !campaign.missions[target].choices.is_empty() => {
                            errors.push(format!("Campaign '{name}', mission {index}: site {} starts mission {target}, a choice point itself.", choice.site));
                        }
                        Some(target) if target != index && target < index => {
                            errors.push(format!("Campaign '{name}', mission {index}: site {} starts mission {target}, which comes before it.", choice.site));
                        }
                        Some(_) => {}
                    }
                }
            }
            if !campaign.missions.is_empty() {
                for (field, value) in [("stele_title", &campaign.stele_title), ("stele_closing", &campaign.stele_closing)] {
                    if value.as_deref().is_none_or(str::is_empty) {
                        errors.push(format!("Campaign '{name}' has missions but no {field}."));
                    }
                }
            }
            let mut distinctions: Vec<&str> = Vec::new();
            for (index, mission) in campaign.missions.iter().enumerate() {
                let variant = campaign.is_variant(index);
                for (field, value) in
                    [("stele_line", &mission.stele_line), ("lead_in", &mission.lead_in), ("briefing", &mission.briefing)]
                {
                    if value.as_deref().is_none_or(str::is_empty) && !(variant && field == "stele_line") {
                        errors.push(format!("Campaign '{name}', mission {index}: no {field}."));
                    }
                }
                for distinction in &mission.distinctions {
                    if distinctions.contains(&distinction.id.as_str()) {
                        errors.push(format!("Campaign '{name}': two distinctions have the id '{}'.", distinction.id));
                    }
                    distinctions.push(&distinction.id);
                }
            }
            for target in 0..campaign.missions.len() {
                let points = (0..campaign.missions.len()).filter(|&step| step != target && campaign.choice_targets(step).contains(&target)).count();
                if points > 1 {
                    errors.push(format!("Campaign '{name}': mission {target} is a site of {points} choice points."));
                }
            }
        }
        errors
    }
}

// Gives a campaign without an id the id "campaign_<index>" (its place in the list), with a warning.
// Returns the ids two or more campaigns share, once each.
pub fn assign_campaign_ids(campaigns: &mut [CampaignDef]) -> Vec<String> {
    for (index, campaign) in campaigns.iter_mut().enumerate() {
        if campaign.id.is_empty() {
            campaign.id = format!("campaign_{index}");
            log::warning!(log::channel!("campaign"), "Campaign '{}' has no id: using '{}'.", campaign.name, campaign.id);
        }
    }
    let mut shared: Vec<String> = Vec::new();
    for (index, campaign) in campaigns.iter().enumerate() {
        if campaigns[..index].iter().any(|other| other.id == campaign.id) && !shared.contains(&campaign.id) {
            shared.push(campaign.id.clone());
        }
    }
    shared
}

// ----------------------------------------------
// CampaignConfigs Global Singleton
// ----------------------------------------------

engine::configurations! { CAMPAIGN_CONFIGS_SINGLETON, CampaignConfigs, "game/campaigns" }
