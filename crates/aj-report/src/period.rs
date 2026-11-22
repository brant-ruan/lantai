//! Report periods: ISO weeks, calendar months and their keys, dates and titles. Pure logic.

use anyhow::{bail, Context, Result};
use chrono::{Datelike, Duration, IsoWeek, NaiveDate, Weekday};

/// An ISO week (Monday to Sunday), keyed like `2026-W47`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Week {
    monday: NaiveDate,
}

impl Week {
    /// The week a date falls in.
    pub fn containing(date: NaiveDate) -> Self {
        let iso: IsoWeek = date.iso_week();
        Self {
            monday: NaiveDate::from_isoywd_opt(iso.year(), iso.week(), Weekday::Mon)
                .expect("an ISO week always has a Monday"),
        }
    }

    /// Parse `2026-W47` (also `2026-w47` and `2026W47`).
    pub fn parse(key: &str) -> Result<Self> {
        let k = key.trim().to_ascii_uppercase();
        let (year, week) = k
            .split_once('W')
            .with_context(|| format!("a week looks like 2026-W47, got {key}"))?;
        let year: i32 = year
            .trim_end_matches('-')
            .parse()
            .with_context(|| format!("a week looks like 2026-W47, got {key}"))?;
        let week: u32 = week
            .parse()
            .with_context(|| format!("a week looks like 2026-W47, got {key}"))?;
        let monday = NaiveDate::from_isoywd_opt(year, week, Weekday::Mon)
            .with_context(|| format!("{key} is not a week of {year}"))?;
        Ok(Self { monday })
    }

    pub fn key(&self) -> String {
        let iso = self.monday.iso_week();
        format!("{}-W{:02}", iso.year(), iso.week())
    }

    pub fn monday(&self) -> NaiveDate {
        self.monday
    }

    pub fn sunday(&self) -> NaiveDate {
        self.monday + Duration::days(6)
    }

    /// The seven dates, Monday first.
    pub fn dates(&self) -> Vec<NaiveDate> {
        (0..7).map(|i| self.monday + Duration::days(i)).collect()
    }

    /// The calendar month this week belongs to: the month containing its Thursday
    /// (the month with at least four days of the week).
    pub fn month(&self) -> Month {
        Month::containing(self.monday + Duration::days(3))
    }

    /// The canonical report title: `# 2026-W47 · 11-16 ~ 11-22`.
    pub fn title(&self) -> String {
        format!(
            "# {} \u{00b7} {} ~ {}",
            self.key(),
            self.monday.format("%m-%d"),
            self.sunday().format("%m-%d")
        )
    }
}

/// A calendar month, keyed like `2026-11`. Its weeks are the ISO weeks whose
/// Thursday falls in this month (the weeks with at least four days in it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Month {
    year: i32,
    month: u32,
}

impl Month {
    /// The calendar month a date falls in.
    pub fn containing(date: NaiveDate) -> Self {
        Self {
            year: date.year(),
            month: date.month(),
        }
    }

    /// Parse `2026-11` (accepts single-digit months like `2026-5` as well).
    pub fn parse(key: &str) -> Result<Self> {
        let k = key.trim();
        let (year_str, month_str) = k
            .split_once('-')
            .with_context(|| format!("a month looks like 2026-11, got {key}"))?;
        if year_str.len() != 4
            || !year_str.bytes().all(|b| b.is_ascii_digit())
            || !(1..=2).contains(&month_str.len())
            || !month_str.bytes().all(|b| b.is_ascii_digit())
        {
            bail!("a month looks like 2026-11, got {key}");
        }
        let year: i32 = year_str
            .parse()
            .with_context(|| format!("a month looks like 2026-11, got {key}"))?;
        let month: u32 = month_str
            .parse()
            .with_context(|| format!("a month looks like 2026-11, got {key}"))?;
        if NaiveDate::from_ymd_opt(year, month, 1).is_none() {
            bail!("{key} is not a valid month");
        }
        Ok(Self { year, month })
    }

    pub fn key(&self) -> String {
        format!("{:04}-{:02}", self.year, self.month)
    }

    pub fn first_day(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.year, self.month, 1).expect("Month always holds a valid month")
    }

    pub fn last_day(&self) -> NaiveDate {
        let (next_year, next_month) = if self.month == 12 {
            (self.year + 1, 1)
        } else {
            (self.year, self.month + 1)
        };
        NaiveDate::from_ymd_opt(next_year, next_month, 1).expect("next month is always valid")
            - Duration::days(1)
    }

    /// The ISO weeks belonging to this month (Monday-first chronological order).
    pub fn weeks(&self) -> Vec<Week> {
        let mut out = Vec::new();
        let mut d = self.first_day();
        let end = self.last_day();
        while d <= end {
            if d.weekday() == Weekday::Thu {
                out.push(Week::containing(d));
            }
            d += Duration::days(1);
        }
        out
    }

    /// The canonical report title: `# 2026-11 · W45 ~ W48`.
    pub fn title(&self) -> String {
        let weeks = self.weeks();
        let first = weeks.first().expect("a month has at least four Thursdays");
        let last = weeks.last().expect("a month has at least four Thursdays");
        format!(
            "# {} \u{00b7} W{:02} ~ W{:02}",
            self.key(),
            first.monday().iso_week().week(),
            last.monday().iso_week().week(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeks_parse_format_and_span_monday_to_sunday() {
        let w = Week::parse("2026-w47").unwrap();
        assert_eq!(w.key(), "2026-W47");
        assert_eq!(w.monday().to_string(), "2026-11-16");
        assert_eq!(w.dates().len(), 7);
        assert_eq!(w.title(), "# 2026-W47 \u{00b7} 11-16 ~ 11-22");
        assert_eq!(Week::containing("2026-11-22".parse().unwrap()), w);
        assert_eq!(
            Week::containing("2026-11-23".parse().unwrap()).key(),
            "2026-W48"
        );
        // The ISO year can differ from the calendar year at the edges.
        assert_eq!(
            Week::containing("2027-01-01".parse().unwrap()).key(),
            "2026-W53"
        );
        assert_eq!(Week::parse("2026W05").unwrap().key(), "2026-W05");
        assert!(Week::parse("2026-11-16").is_err());
        assert!(Week::parse("2026-W60").is_err());
    }

    #[test]
    fn months_parse_span_thursday_owned_weeks_and_format_title() {
        let m = Month::parse("2026-11").unwrap();
        assert_eq!(m.key(), "2026-11");
        assert_eq!(m.first_day().to_string(), "2026-11-01");
        assert_eq!(m.last_day().to_string(), "2026-11-30");
        assert_eq!(Month::parse("2026-5").unwrap().key(), "2026-05");
        assert_eq!(
            Month::parse("2026-12").unwrap().last_day().to_string(),
            "2026-12-31"
        );
        assert_eq!(
            Month::parse("2024-02").unwrap().last_day().to_string(),
            "2024-02-29"
        );

        // 2026-11 Thursdays: Nov 5 (W45), Nov 12 (W46), Nov 19 (W47), Nov 26 (W48).
        // 2026-W44 runs Oct 26 .. Nov 1 (Thu Oct 29 -> October).
        // 2026-W49 runs Nov 30 .. Dec 6 (Thu Dec 3 -> December).
        let week_keys: Vec<String> = m.weeks().iter().map(|w| w.key()).collect();
        assert_eq!(week_keys, ["2026-W45", "2026-W46", "2026-W47", "2026-W48"]);
        assert_eq!(m.title(), "# 2026-11 \u{00b7} W45 ~ W48");

        assert_eq!(Week::parse("2026-W44").unwrap().month().key(), "2026-10");
        assert_eq!(Week::parse("2026-W45").unwrap().month(), m);
        assert_eq!(Week::parse("2026-W49").unwrap().month().key(), "2026-12");

        // 2027-W53 starts 2026-12-28 (Thu 2026-12-31) -> 2026-12! Wait: 2026-12-31 is Thursday, so 2026-W53 belongs to 2026-12.
        assert_eq!(Week::parse("2026-W53").unwrap().month().key(), "2026-12");

        assert!(Month::parse("2026-13").is_err());
        assert!(Month::parse("2026-00").is_err());
        assert!(Month::parse("2026-W47").is_err());
        assert!(Month::parse("2026-11-16").is_err());
    }
}
