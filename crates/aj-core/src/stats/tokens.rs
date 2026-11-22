//! Token usage aggregation for the analytics dashboard: totals, cache hit rate, and the
//! breakdown by model and by agent. Pure logic over rows already filtered by the store.

use std::collections::HashMap;

use aj_ir::{AgentId, Usage};
use aj_store::TokenAggregateRow;

#[derive(Debug, Clone, PartialEq)]
pub struct TokenStats {
    /// Number of model calls.
    pub calls: u64,
    pub usage: Usage,
    /// Cache reads as a share of all input-side tokens (input + cache read + cache write).
    pub cache_hit_pct: f64,
    /// Sorted by total tokens, largest first.
    pub models: Vec<TokenModelStat>,
    pub agents: Vec<TokenAgentStat>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenModelStat {
    pub agent: AgentId,
    /// `None` when the source did not name the model.
    pub model: Option<String>,
    pub calls: u64,
    pub usage: Usage,
    pub share_pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenAgentStat {
    pub agent: AgentId,
    pub calls: u64,
    pub usage: Usage,
    pub share_pct: f64,
}

pub fn add_usage(acc: &mut Usage, u: &Usage) {
    acc.input_tokens += u.input_tokens;
    acc.output_tokens += u.output_tokens;
    acc.cache_read_tokens += u.cache_read_tokens;
    acc.cache_write_tokens += u.cache_write_tokens;
}

pub fn cache_hit_pct(u: &Usage) -> f64 {
    let input_side = u.input_tokens + u.cache_read_tokens + u.cache_write_tokens;
    super::pct(u.cache_read_tokens as f64, input_side as f64)
}

pub fn compute_token_stats(rows: &[TokenAggregateRow]) -> TokenStats {
    let mut calls = 0u64;
    let mut usage = Usage::default();
    let mut by_model: HashMap<(AgentId, Option<String>), (u64, Usage)> = HashMap::new();
    let mut by_agent: HashMap<AgentId, (u64, Usage)> = HashMap::new();
    for r in rows {
        calls += r.calls;
        add_usage(&mut usage, &r.usage);
        let m = by_model
            .entry((r.agent.clone(), r.model.clone()))
            .or_default();
        m.0 += r.calls;
        add_usage(&mut m.1, &r.usage);
        let a = by_agent.entry(r.agent.clone()).or_default();
        a.0 += r.calls;
        add_usage(&mut a.1, &r.usage);
    }
    let total = usage.total() as f64;

    let mut models: Vec<TokenModelStat> = by_model
        .into_iter()
        .map(|((agent, model), (calls, usage))| TokenModelStat {
            agent,
            model,
            calls,
            usage,
            share_pct: super::pct(usage.total() as f64, total),
        })
        .collect();
    models.sort_by(|a, b| {
        b.usage
            .total()
            .cmp(&a.usage.total())
            .then_with(|| a.agent.as_str().cmp(b.agent.as_str()))
            .then_with(|| a.model.cmp(&b.model))
    });

    let mut agents: Vec<TokenAgentStat> = by_agent
        .into_iter()
        .map(|(agent, (calls, usage))| TokenAgentStat {
            agent,
            calls,
            usage,
            share_pct: super::pct(usage.total() as f64, total),
        })
        .collect();
    agents.sort_by(|a, b| {
        b.usage
            .total()
            .cmp(&a.usage.total())
            .then_with(|| a.agent.as_str().cmp(b.agent.as_str()))
    });

    TokenStats {
        calls,
        cache_hit_pct: cache_hit_pct(&usage),
        usage,
        models,
        agents,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        date: &str,
        agent: AgentId,
        model: Option<&str>,
        calls: u64,
        u: (u64, u64, u64, u64),
    ) -> TokenAggregateRow {
        TokenAggregateRow {
            local_date: date.into(),
            agent,
            model: model.map(str::to_string),
            calls,
            usage: Usage {
                input_tokens: u.0,
                output_tokens: u.1,
                cache_read_tokens: u.2,
                cache_write_tokens: u.3,
            },
        }
    }

    #[test]
    fn totals_cache_rate_and_breakdowns() {
        let rows = [
            row(
                "2026-11-10",
                AgentId::CLAUDE,
                Some("opus"),
                3,
                (100, 50, 800, 50),
            ),
            row("2026-11-11", AgentId::CLAUDE, Some("opus"), 1, (0, 0, 0, 0)),
            row("2026-11-11", AgentId::CODEX, None, 2, (200, 100, 0, 0)),
        ];
        let s = compute_token_stats(&rows);
        assert_eq!(s.calls, 6);
        assert_eq!(s.usage.total(), 1300);
        // 800 cache reads out of 100 + 800 + 50 + 200 input-side tokens.
        assert!((s.cache_hit_pct - 800.0 / 1150.0 * 100.0).abs() < 1e-9);
        assert_eq!(s.models.len(), 2);
        assert_eq!(s.models[0].model.as_deref(), Some("opus"));
        assert_eq!(s.models[0].calls, 4);
        assert_eq!(s.models[1].model, None);
        assert_eq!(s.agents[0].agent, AgentId::CLAUDE);
        assert!((s.agents[1].share_pct - 300.0 / 1300.0 * 100.0).abs() < 1e-9);
    }

    #[test]
    fn empty_rows_give_zeroes() {
        let s = compute_token_stats(&[]);
        assert_eq!((s.calls, s.usage.total(), s.cache_hit_pct), (0, 0, 0.0));
        assert!(s.models.is_empty() && s.agents.is_empty());
    }
}
