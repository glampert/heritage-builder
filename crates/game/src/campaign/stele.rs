// ----------------------------------------------
// The campaign's stele (docs/plans/realm-ui.md, Phase 3: Stele)
// ----------------------------------------------
//
// What a campaign's stele shows: its title, one carved line per finished step (the mission's
// `stele_line`, or its choice point's for a variant without one) followed by a line for each
// distinction it earned (M9), and an empty row for each step still to come. The debrief marks the
// line of the mission just finished as freshly carved, the campaign-complete screen the last.
// Read from the configs and the player's profile only, so the dialogs, the Skin Gallery and
// automation (intermission.state) all see the same thing; menu::stele draws it.

use engine::ui::text::{self, UiTextCategory};

use super::config::{CampaignConfigs, CampaignDef};
use crate::profile::{CampaignRecord, Profile};

// One carved line.
#[derive(Clone, Debug, PartialEq)]
pub struct SteleLine {
    // The mission it records, by index in the campaign, and its number as the player reads it.
    pub mission_index: usize,
    pub mission_number: usize,
    // The distinction it records (M9), if it's a distinction's line rather than the mission's.
    pub distinction: Option<String>,
    // Its Story key (none: the mission had no line, and its name stands in), and its text.
    pub key: Option<String>,
    pub text: String,
    // The line freshly carved: drawn in the stele's `newest` look, with its mark.
    pub newest: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stele {
    pub campaign_index: usize,
    pub title: String,
    pub lines: Vec<SteleLine>,
    // The steps not finished yet: an empty row each (as many as fit).
    pub rows_to_come: usize,
}

impl Stele {
    // What the dialogs' stele canvas records for automation (MissionComplete/stele,
    // CampaignComplete/stele): its title, its lines' texts, the newest one's and the rows to come.
    pub fn canvas_value(&self) -> serde_json::Value {
        serde_json::json!({
            "title": self.title,
            "lines": self.lines.iter().map(|line| line.text.clone()).collect::<Vec<_>>(),
            "newest": self.lines.iter().find(|line| line.newest).map(|line| line.text.clone()),
            "rows_to_come": self.rows_to_come,
        })
    }

    // What intermission.state reports: its title, each line in full (mission index and number,
    // distinction, Story key, text, newest) and the rows to come.
    pub fn state_json(&self) -> serde_json::Value {
        serde_json::json!({
            "title": self.title,
            "lines": self.lines.iter().map(|line| serde_json::json!({
                "mission": line.mission_index,
                "number": line.mission_number,
                "distinction": line.distinction,
                "key": line.key,
                "text": line.text,
                "newest": line.newest,
            })).collect::<Vec<_>>(),
            "rows_to_come": self.rows_to_come,
        })
    }
}

// The stele of the campaign at `campaign_index` as the profile has it carved. `newest`: the mission
// whose lines are marked as freshly carved (the one just finished, for the debrief); None, or a
// mission with no line on the stele, marks the last line's. None for a campaign that doesn't exist.
pub fn stele(configs: &CampaignConfigs, profile: &Profile, campaign_index: usize, newest: Option<usize>) -> Option<Stele> {
    let campaign = configs.campaign(campaign_index)?;
    let record = profile.record(&campaign.id);
    let finished = record.map_or(&[][..], |record| record.missions_finished.as_slice());
    let mut carved = Vec::new();
    let mut rows_to_come = 0;
    for step in campaign.steps() {
        match finished_mission(campaign, record, finished, step) {
            Some(mission_index) => carved.push(mission_index),
            None => rows_to_come += 1,
        }
    }
    Some(build(campaign, campaign_index, &carved, record, rows_to_come, newest))
}

// The stele as it would stand with every step finished, at its first missions (no choice made, no
// distinctions), the last line freshly carved: intermission.state's `full`.
pub fn carved_in_full(configs: &CampaignConfigs, campaign_index: usize) -> Option<Stele> {
    let campaign = configs.campaign(campaign_index)?;
    Some(build(campaign, campaign_index, &campaign.steps(), None, 0, None))
}

// The stele as it stands in the middle of the campaign: every step but the last finished (at
// their first missions; at least one), the last carved line freshly marked, and `rows_to_come`
// empty rows under it: the Skin Gallery's sample, which shows the rows and their capping.
pub fn partly_carved(configs: &CampaignConfigs, campaign_index: usize, rows_to_come: usize) -> Option<Stele> {
    let campaign = configs.campaign(campaign_index)?;
    let steps = campaign.steps();
    let carved = steps.len().saturating_sub(1).max(1).min(steps.len());
    Some(build(campaign, campaign_index, &steps[..carved], None, rows_to_come, None))
}

// The mission that finished the step `step`, if one did: the site picked there's, if it's
// finished, else the first finished of the step and the missions its sites start.
fn finished_mission(
    campaign: &CampaignDef,
    record: Option<&CampaignRecord>,
    finished: &[usize],
    step: usize,
) -> Option<usize> {
    let picked = record.and_then(|record| record.choice(step)).and_then(|site| campaign.site_mission(step, site));
    if let Some(mission) = picked
        && finished.contains(&mission)
    {
        return Some(mission);
    }
    std::iter::once(step).chain(campaign.choice_targets(step)).find(|mission| finished.contains(mission))
}

fn build(
    campaign: &CampaignDef,
    campaign_index: usize,
    carved: &[usize],
    record: Option<&CampaignRecord>,
    rows_to_come: usize,
    newest: Option<usize>,
) -> Stele {
    let mut lines = Vec::new();
    for &mission_index in carved {
        let mission = &campaign.missions[mission_index];
        let mission_number = campaign.mission_number(mission_index);
        // A variant without a line of its own takes its choice point's.
        let key =
            mission.stele_line.clone().or_else(|| campaign.missions[campaign.step_of(mission_index)].stele_line.clone());
        let text = match &key {
            Some(key) => text::get(UiTextCategory::Story, key).to_string(),
            None => campaign.mission_display_name(mission_index).unwrap_or_default().to_string(),
        };
        lines.push(SteleLine { mission_index, mission_number, distinction: None, key, text, newest: false });
        // TODO(realm-ui): M9 awards distinctions into the profile (CampaignRecord::distinctions).
        let earned = record.map_or(&[][..], |record| record.distinctions.as_slice());
        for distinction in mission.distinctions.iter().filter(|distinction| earned.contains(&distinction.id)) {
            lines.push(SteleLine {
                mission_index,
                mission_number,
                distinction: Some(distinction.id.clone()),
                key: Some(distinction.stele_line.clone()),
                text: text::get(UiTextCategory::Story, &distinction.stele_line).to_string(),
                newest: false,
            });
        }
    }
    let marked = newest
        .filter(|mission| lines.iter().any(|line| line.mission_index == *mission))
        .or(lines.last().map(|line| line.mission_index));
    for line in &mut lines {
        line.newest = Some(line.mission_index) == marked;
    }
    Stele { campaign_index, title: campaign.stele_title_text().to_string(), lines, rows_to_come }
}
