//! Classification and totals for Lantai's own state (report coverage, digest cache). Pure logic.

use aj_ir::AgentKind;
use chrono::NaiveDate;

/// Whether a slice's digest can be reused as is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestState {
    /// The cached digest matches the slice's current input.
    Fresh,
    /// An older digest exists, but the slice gained turns or the model or prompt version
    /// changed since: generating updates or rewrites it.
    Behind,
    /// Never digested.
    Missing,
}

pub fn digest_state(cached_valid: bool, has_old_digest: bool) -> DigestState {
    match (cached_valid, has_old_digest) {
        (true, _) => DigestState::Fresh,
        (false, true) => DigestState::Behind,
        (false, false) => DigestState::Missing,
    }
}

/// Slices counted by digest state; ignored slices need no digest and are counted apart.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DigestTally {
    pub fresh: u64,
    pub behind: u64,
    pub missing: u64,
    pub ignored: u64,
}

impl DigestTally {
    pub fn add(&mut self, state: DigestState) {
        match state {
            DigestState::Fresh => self.fresh += 1,
            DigestState::Behind => self.behind += 1,
            DigestState::Missing => self.missing += 1,
        }
    }

    pub fn merge(&mut self, other: &DigestTally) {
        self.fresh += other.fresh;
        self.behind += other.behind;
        self.missing += other.missing;
        self.ignored += other.ignored;
    }

    /// Slices that take part in reports.
    pub fn active(&self) -> u64 {
        self.fresh + self.behind + self.missing
    }
}

/// Report state of one period, as drawn in the coverage strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellStatus {
    /// The period has slices that take part in reports, but no report.
    None,
    Draft,
    Final,
    /// Every slice of the period is ignored and there is no report.
    Ignored,
}

impl CellStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Draft => "draft",
            Self::Final => "final",
            Self::Ignored => "ignored",
        }
    }
}

/// A report that exists counts whatever its slices are; otherwise a period whose slices are
/// all ignored needs none.
pub fn cell_status(report_status: Option<&str>, active_slices: u64) -> CellStatus {
    match report_status {
        Some("final") => CellStatus::Final,
        Some(_) => CellStatus::Draft,
        None if active_slices == 0 => CellStatus::Ignored,
        None => CellStatus::None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeriodCell {
    /// Date, week key or month key.
    pub key: String,
    /// Days the period covers: a week runs Monday to Sunday, a month over its weeks.
    pub first_day: NaiveDate,
    pub last_day: NaiveDate,
    pub status: CellStatus,
    /// The report's inputs changed after it was generated.
    pub stale: bool,
    /// Projects whose sections an update would write again.
    pub stale_projects: Vec<String>,
    pub versions: u64,
    /// Some version was saved by hand.
    pub edited: bool,
}

/// Totals of one report kind over the cells of the window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KindTotals {
    /// Periods that need a report (not entirely ignored, or reported anyway).
    pub periods: u64,
    pub reports: u64,
    pub finals: u64,
    pub stale: u64,
    pub edited: u64,
}

pub fn kind_totals(cells: &[PeriodCell]) -> KindTotals {
    let mut t = KindTotals::default();
    for c in cells {
        if c.status == CellStatus::Ignored {
            continue;
        }
        t.periods += 1;
        if matches!(c.status, CellStatus::Draft | CellStatus::Final) {
            t.reports += 1;
        }
        t.finals += u64::from(c.status == CellStatus::Final);
        t.stale += u64::from(c.stale);
        t.edited += u64::from(c.edited);
    }
    t
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayDigest {
    pub date: String,
    pub tally: DigestTally,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDigest {
    pub agent: AgentKind,
    pub tally: DigestTally,
}

/// Lantai's own state over a window of days.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfStats {
    /// The window: `since`, or the first active day when there is none, to `until`.
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Days, weeks and months with slices, oldest first.
    pub days: Vec<PeriodCell>,
    pub weeks: Vec<PeriodCell>,
    pub months: Vec<PeriodCell>,
    pub daily: KindTotals,
    pub weekly: KindTotals,
    pub monthly: KindTotals,
    pub digest_days: Vec<DayDigest>,
    pub digest: DigestTally,
    /// Per agent, in display order; agents without slices are left out.
    pub agents: Vec<AgentDigest>,
    /// Slices too long for one request, digested in parts.
    pub split_slices: u64,
    /// Incremental digest updates applied since the slices' last full digests.
    pub incremental_updates: u64,
    /// Cached project task clusters.
    pub project_briefs: u64,
}

/// Agents in display order with their tallies, dropping agents without slices.
pub fn agent_rows(mut tallies: Vec<(AgentKind, DigestTally)>) -> Vec<AgentDigest> {
    tallies.retain(|(_, t)| t.active() + t.ignored > 0);
    tallies.sort_by_key(|(a, _)| AgentKind::ALL.iter().position(|k| k == a));
    tallies
        .into_iter()
        .map(|(agent, tally)| AgentDigest { agent, tally })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(status: CellStatus, stale: bool, edited: bool) -> PeriodCell {
        let day = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
        PeriodCell {
            key: "k".into(),
            first_day: day,
            last_day: day,
            status,
            stale,
            stale_projects: Vec::new(),
            versions: 1,
            edited,
        }
    }

    #[test]
    fn a_digest_is_fresh_behind_or_missing() {
        assert_eq!(digest_state(true, true), DigestState::Fresh);
        assert_eq!(digest_state(false, true), DigestState::Behind);
        assert_eq!(digest_state(false, false), DigestState::Missing);
    }

    #[test]
    fn existing_reports_count_even_when_every_slice_is_ignored() {
        assert_eq!(cell_status(Some("final"), 0), CellStatus::Final);
        assert_eq!(cell_status(Some("draft"), 3), CellStatus::Draft);
        assert_eq!(cell_status(None, 0), CellStatus::Ignored);
        assert_eq!(cell_status(None, 2), CellStatus::None);
    }

    #[test]
    fn totals_leave_out_ignored_periods() {
        let t = kind_totals(&[
            cell(CellStatus::Final, false, true),
            cell(CellStatus::Draft, true, false),
            cell(CellStatus::None, false, false),
            cell(CellStatus::Ignored, false, false),
        ]);
        assert_eq!(
            t,
            KindTotals {
                periods: 3,
                reports: 2,
                finals: 1,
                stale: 1,
                edited: 1
            }
        );
    }

    #[test]
    fn agents_follow_display_order_and_empty_ones_drop() {
        let busy = DigestTally {
            fresh: 1,
            ..Default::default()
        };
        let rows = agent_rows(vec![
            (AgentKind::Codex, busy),
            (AgentKind::Dsh, DigestTally::default()),
            (AgentKind::Claude, busy),
        ]);
        let agents: Vec<_> = rows.iter().map(|r| r.agent).collect();
        assert_eq!(agents, [AgentKind::Claude, AgentKind::Codex]);
    }
}
