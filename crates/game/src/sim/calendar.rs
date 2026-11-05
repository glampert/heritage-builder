// The game calendar: date, season and era, measured in simulated time.

use std::fmt;

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use strum::Display;

use common::time::Seconds;

pub const MONTHS_PER_YEAR: u32 = 12;

// ----------------------------------------------
// CalendarSettings
// ----------------------------------------------

// Where the calendar starts and how a realm counts its years. Saved with the calendar, so a save
// keeps its own seasons and eras whatever the configs say later.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarSettings {
    // World Year (WY) and month (1 to 12) of the first day.
    pub start_year: u32,
    pub start_month: u32,

    // Months (1 to 12) of the wet season. The others are the dry season.
    pub wet_months: Vec<u32>,

    // Eras in the order they start. The date counts years from the latest era that has started.
    pub eras: Vec<EraDef>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EraDef {
    pub name: String,
    // World Year of the era's first year.
    pub start_year: u32,
}

impl Default for CalendarSettings {
    fn default() -> Self {
        Self { start_year: 1, start_month: 1, wet_months: vec![5, 6, 7, 8, 9, 10], eras: Vec::new() }
    }
}

impl CalendarSettings {
    // Settings that would make dates out of range, or eras out of order.
    pub fn validate(&self) -> Result<(), String> {
        if self.start_year == 0 {
            return Err("start_year starts at 1.".into());
        }
        if !(1..=MONTHS_PER_YEAR).contains(&self.start_month) {
            return Err(format!("start_month must be from 1 to {MONTHS_PER_YEAR}, got {}.", self.start_month));
        }
        if let Some(month) = self.wet_months.iter().find(|month| !(1..=MONTHS_PER_YEAR).contains(month)) {
            return Err(format!("wet_months must be from 1 to {MONTHS_PER_YEAR}, got {month}."));
        }
        if self.eras.windows(2).any(|pair| pair[0].start_year >= pair[1].start_year) {
            return Err("eras must be in the order they start.".into());
        }
        Ok(())
    }

    fn season_of(&self, month: u32) -> Season {
        if self.wet_months.contains(&month) { Season::Wet } else { Season::Dry }
    }

    // The latest era that has started by `year`.
    fn era_of(&self, year: u32) -> Option<&EraDef> {
        self.eras.iter().rev().find(|era| era.start_year <= year)
    }
}

// ----------------------------------------------
// Season / CalendarDate / CalendarEvent
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug, Display, Serialize, Deserialize)]
pub enum Season {
    Wet,
    Dry,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct CalendarDate {
    // World Year.
    pub year: u32,
    // 1 to 12.
    pub month: u32,
    // 1 to days_per_month.
    pub day: u32,
}

impl CalendarDate {
    // Months since month 1 of year 0, so dates compare as numbers.
    #[inline]
    pub fn ordinal_month(&self) -> u32 {
        self.year * MONTHS_PER_YEAR + self.month - 1
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CalendarEvent {
    NewMonth(CalendarDate),
    NewSeason(Season),
    NewYear(u32),
}

pub type CalendarEvents = SmallVec<[CalendarEvent; 3]>;

// ----------------------------------------------
// GameCalendar
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct GameCalendar {
    settings: CalendarSettings,

    // Whole months since the calendar started, and simulated seconds into the current one.
    // Counted in months so a change to the month length keeps every save's date.
    months_elapsed: u32,
    secs_into_month: f64,
}

impl GameCalendar {
    pub fn new(settings: CalendarSettings) -> Self {
        debug_assert!(settings.validate().is_ok(), "Invalid calendar settings: {:?}", settings.validate());
        Self { settings, months_elapsed: 0, secs_into_month: 0.0 }
    }

    #[inline]
    pub fn settings(&self) -> &CalendarSettings {
        &self.settings
    }

    // Starts over on the first day, with new settings.
    pub fn restart(&mut self, settings: CalendarSettings) {
        *self = Self::new(settings);
    }

    // Advances by `delta_time_secs` of simulated time, a month every `secs_per_month`. Returns the
    // events for each month boundary crossed: the new month, then the new season if it changed,
    // then the new year if the month is the first.
    pub fn advance(&mut self, delta_time_secs: Seconds, secs_per_month: Seconds) -> CalendarEvents {
        debug_assert!(secs_per_month > 0.0);

        let mut events = CalendarEvents::new();
        let secs_per_month = f64::from(secs_per_month);

        self.secs_into_month += f64::from(delta_time_secs.max(0.0));

        while self.secs_into_month >= secs_per_month {
            let previous_season = self.season();

            self.secs_into_month -= secs_per_month;
            self.months_elapsed += 1;

            let date = self.month_start_date();
            events.push(CalendarEvent::NewMonth(date));

            let season = self.season();
            if season != previous_season {
                events.push(CalendarEvent::NewSeason(season));
            }
            if date.month == 1 {
                events.push(CalendarEvent::NewYear(date.year));
            }
        }

        events
    }

    // Moves to the start of `day` (1-based) of `year` and `month`, without events. Fails for a
    // date out of range or before the calendar's start.
    pub fn set_date(&mut self, date: CalendarDate, days_per_month: u32, secs_per_month: Seconds) -> Result<(), String> {
        if !(1..=MONTHS_PER_YEAR).contains(&date.month) {
            return Err(format!("month must be from 1 to {MONTHS_PER_YEAR}, got {}.", date.month));
        }
        if !(1..=days_per_month).contains(&date.day) {
            return Err(format!("day must be from 1 to {days_per_month}, got {}.", date.day));
        }

        let start = self.start_date().ordinal_month();
        let Some(months_elapsed) = date.ordinal_month().checked_sub(start) else {
            return Err(format!(
                "The calendar starts in month {} of year {}; can't go back before it.",
                self.settings.start_month, self.settings.start_year
            ));
        };

        self.months_elapsed = months_elapsed;
        self.secs_into_month = f64::from(date.day - 1) * f64::from(secs_per_month) / f64::from(days_per_month);
        Ok(())
    }

    // ----------------------
    // Date:
    // ----------------------

    // Today's date. `days_per_month` and `secs_per_month` come from `SimConfigs`.
    pub fn date(&self, days_per_month: u32, secs_per_month: Seconds) -> CalendarDate {
        let mut date = self.month_start_date();

        let secs_per_day = f64::from(secs_per_month) / f64::from(days_per_month.max(1));
        let day = (self.secs_into_month / secs_per_day).floor() as u32 + 1;
        date.day = day.clamp(1, days_per_month.max(1));

        date
    }

    #[inline]
    pub fn season(&self) -> Season {
        self.settings.season_of(self.month_start_date().month)
    }

    #[inline]
    pub fn months_elapsed(&self) -> u32 {
        self.months_elapsed
    }

    // Simulated seconds since the calendar started.
    pub fn elapsed_secs(&self, secs_per_month: Seconds) -> f64 {
        f64::from(self.months_elapsed) * f64::from(secs_per_month) + self.secs_into_month
    }

    // The era the current year belongs to, and the year within it (1 for its first year).
    pub fn era(&self) -> Option<(&str, u32)> {
        let year = self.month_start_date().year;
        self.settings.era_of(year).map(|era| (era.name.as_str(), year - era.start_year + 1))
    }

    // The year as the realm counts it: "Long Thái 3", or "Year 3" without an era.
    pub fn year_name(&self) -> YearName<'_> {
        YearName { calendar: self }
    }

    fn start_date(&self) -> CalendarDate {
        CalendarDate { year: self.settings.start_year, month: self.settings.start_month, day: 1 }
    }

    fn month_start_date(&self) -> CalendarDate {
        let ordinal = self.start_date().ordinal_month() + self.months_elapsed;
        CalendarDate { year: ordinal / MONTHS_PER_YEAR, month: ordinal % MONTHS_PER_YEAR + 1, day: 1 }
    }
}

impl Default for GameCalendar {
    fn default() -> Self {
        Self::new(CalendarSettings::default())
    }
}

pub struct YearName<'calendar> {
    calendar: &'calendar GameCalendar,
}

impl fmt::Display for YearName<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.calendar.era() {
            Some((era, year)) => write!(f, "{era} {year}"),
            None => write!(f, "Year {}", self.calendar.month_start_date().year),
        }
    }
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SECS_PER_MONTH: Seconds = 60.0;
    const DAYS_PER_MONTH: u32 = 30;

    fn date(year: u32, month: u32, day: u32) -> CalendarDate {
        CalendarDate { year, month, day }
    }

    fn calendar_at(start_year: u32, start_month: u32) -> GameCalendar {
        GameCalendar::new(CalendarSettings { start_year, start_month, ..CalendarSettings::default() })
    }

    #[test]
    fn test_date_advances_by_days_and_months() {
        let mut calendar = calendar_at(1, 1);
        assert_eq!(calendar.date(DAYS_PER_MONTH, SECS_PER_MONTH), date(1, 1, 1));

        assert!(calendar.advance(3.5, SECS_PER_MONTH).is_empty());
        assert_eq!(calendar.date(DAYS_PER_MONTH, SECS_PER_MONTH), date(1, 1, 2));

        let events = calendar.advance(56.5, SECS_PER_MONTH);
        assert_eq!(events.as_slice(), &[CalendarEvent::NewMonth(date(1, 2, 1))]);
        assert_eq!(calendar.date(DAYS_PER_MONTH, SECS_PER_MONTH), date(1, 2, 1));
        assert_eq!(calendar.months_elapsed(), 1);
        assert!((calendar.elapsed_secs(SECS_PER_MONTH) - 60.0).abs() < 1e-4);
    }

    #[test]
    fn test_events_for_season_and_year() {
        // Month 4, dry; month 5 starts the wet season.
        let mut calendar = calendar_at(1, 4);
        assert_eq!(calendar.season(), Season::Dry);

        let events = calendar.advance(SECS_PER_MONTH, SECS_PER_MONTH);
        assert_eq!(events.as_slice(), &[CalendarEvent::NewMonth(date(1, 5, 1)), CalendarEvent::NewSeason(Season::Wet)]);

        // Month 12 to 1: a new year, still dry.
        let mut calendar = calendar_at(1, 12);
        let events = calendar.advance(SECS_PER_MONTH, SECS_PER_MONTH);
        assert_eq!(events.as_slice(), &[CalendarEvent::NewMonth(date(2, 1, 1)), CalendarEvent::NewYear(2)]);

        // A long step crosses several months, each with its events.
        let mut calendar = calendar_at(1, 10);
        let events = calendar.advance(SECS_PER_MONTH * 4.0, SECS_PER_MONTH);
        assert_eq!(events.as_slice(), &[
            CalendarEvent::NewMonth(date(1, 11, 1)),
            CalendarEvent::NewSeason(Season::Dry),
            CalendarEvent::NewMonth(date(1, 12, 1)),
            CalendarEvent::NewMonth(date(2, 1, 1)),
            CalendarEvent::NewYear(2),
            CalendarEvent::NewMonth(date(2, 2, 1)),
        ]);
    }

    #[test]
    fn test_eras_and_year_names() {
        let settings = CalendarSettings {
            start_year: 2,
            eras: vec![EraDef { name: "First".into(), start_year: 3 }, EraDef { name: "Second".into(), start_year: 10 }],
            ..CalendarSettings::default()
        };
        assert!(settings.validate().is_ok());

        let mut calendar = GameCalendar::new(settings);
        assert_eq!(calendar.era(), None);
        assert_eq!(calendar.year_name().to_string(), "Year 2");

        calendar.set_date(date(4, 6, 1), DAYS_PER_MONTH, SECS_PER_MONTH).unwrap();
        assert_eq!(calendar.year_name().to_string(), "First 2");

        calendar.set_date(date(10, 1, 1), DAYS_PER_MONTH, SECS_PER_MONTH).unwrap();
        assert_eq!(calendar.era(), Some(("Second", 1)));
    }

    #[test]
    fn test_set_date() {
        let mut calendar = calendar_at(3, 6);

        calendar.set_date(date(4, 2, 16), DAYS_PER_MONTH, SECS_PER_MONTH).unwrap();
        assert_eq!(calendar.date(DAYS_PER_MONTH, SECS_PER_MONTH), date(4, 2, 16));
        assert_eq!(calendar.months_elapsed(), 8);

        assert!(calendar.set_date(date(3, 5, 1), DAYS_PER_MONTH, SECS_PER_MONTH).is_err());
        assert!(calendar.set_date(date(4, 13, 1), DAYS_PER_MONTH, SECS_PER_MONTH).is_err());
        assert!(calendar.set_date(date(4, 1, 31), DAYS_PER_MONTH, SECS_PER_MONTH).is_err());
    }

    #[test]
    fn test_settings_validation() {
        let valid = CalendarSettings::default();
        assert!(valid.validate().is_ok());
        assert!(CalendarSettings { start_year: 0, ..valid.clone() }.validate().is_err());
        assert!(CalendarSettings { start_month: 13, ..valid.clone() }.validate().is_err());
        assert!(CalendarSettings { wet_months: vec![0], ..valid.clone() }.validate().is_err());

        let eras = vec![EraDef { name: "B".into(), start_year: 5 }, EraDef { name: "A".into(), start_year: 5 }];
        assert!(CalendarSettings { eras, ..valid }.validate().is_err());
    }
}
