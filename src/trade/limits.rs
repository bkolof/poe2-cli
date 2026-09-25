//! The trade site's per-IP rate limits, kept on disk because every poe2
//! command is a separate process.
//!
//! Each response announces its policy and rules, e.g.
//! `X-Rate-Limit-Ip: 5:10:60,15:60:300`: at most 5 requests per 10 seconds,
//! with a 60 second lockout when exceeded. Before a request, the limiter
//! waits until every rule has room. Exceeding a rule locks the IP out, so it
//! never gambles on one.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Longer waits fail instead of leaving the command hanging.
const MAX_WAIT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Policy {
    Search,
    Fetch,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub max: u32,
    pub period_secs: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PolicyState {
    rules: Vec<Rule>,
    /// Request times, in unix seconds.
    hits: Vec<f64>,
    /// Unix seconds until which the site locked this IP out.
    blocked_until: f64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Limits {
    policies: HashMap<Policy, PolicyState>,
}

impl Limits {
    pub fn load() -> Result<Self> {
        let path = path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        // A corrupt file only loses the history; the site's headers restore the rules.
        Ok(serde_json::from_str(&fs::read_to_string(path)?).unwrap_or_default())
    }

    pub fn save(&self) -> Result<()> {
        fs::write(path()?, serde_json::to_string(self)?)?;
        Ok(())
    }

    /// Wait until a request fits every rule, then count it.
    pub fn acquire(&mut self, policy: Policy) -> Result<()> {
        let wait = self.wait_for(policy, now());

        if wait > MAX_WAIT {
            bail!(
                "the trade site's rate limit needs a {}s break; try again later",
                wait.as_secs()
            );
        }

        if !wait.is_zero() {
            eprintln!(
                "Waiting {:.0}s for the trade site's rate limit...",
                wait.as_secs_f64().ceil()
            );
            thread::sleep(wait);
        }

        self.state(policy).hits.push(now());
        self.save()
    }

    /// Learn the rules and any lockout from a response's headers.
    pub fn update(
        &mut self,
        policy: Policy,
        rules: Option<&str>,
        state: Option<&str>,
        retry_after: Option<&str>,
    ) {
        let entry = self.state(policy);

        if let Some(rules) = rules.and_then(parse_rules) {
            entry.rules = rules;
        }

        // The state header's third field is the remaining lockout in seconds.
        let locked = state
            .into_iter()
            .flat_map(|s| s.split(','))
            .filter_map(|rule| rule.split(':').nth(2)?.parse::<f64>().ok())
            .chain(retry_after.and_then(|r| r.trim().parse::<f64>().ok()))
            .fold(0.0, f64::max);

        if locked > 0.0 {
            entry.blocked_until = entry.blocked_until.max(now() + locked);
        }
    }

    fn wait_for(&mut self, policy: Policy, now: f64) -> Duration {
        let state = self.state(policy);
        let longest = state.rules.iter().map(|r| r.period_secs).max().unwrap_or(0) as f64;
        state.hits.retain(|&hit| now - hit < longest);

        let mut ready = state.blocked_until.max(now);

        for rule in &state.rules {
            let period = rule.period_secs as f64;
            let mut recent: Vec<f64> = state
                .hits
                .iter()
                .copied()
                .filter(|&hit| now - hit < period)
                .collect();
            recent.sort_by(f64::total_cmp);

            if recent.len() >= rule.max as usize {
                // The request can go once enough of the oldest hits leave the window.
                let freeing = recent[recent.len() - rule.max as usize];
                ready = ready.max(freeing + period + 1.0);
            }
        }

        Duration::from_secs_f64(ready - now)
    }

    fn state(&mut self, policy: Policy) -> &mut PolicyState {
        self.policies.entry(policy).or_insert_with(|| PolicyState {
            rules: default_rules(policy),
            ..Default::default()
        })
    }
}

/// Conservative rules until the site's headers say otherwise.
fn default_rules(policy: Policy) -> Vec<Rule> {
    let rules: &[(u32, u64)] = match policy {
        Policy::Search => &[(5, 10), (15, 60), (30, 300)],
        Policy::Fetch => &[(12, 4), (16, 12), (50, 300)],
    };
    rules
        .iter()
        .map(|&(max, period_secs)| Rule { max, period_secs })
        .collect()
}

/// `5:10:60,15:60:300` into rules; the lockout part is not needed.
fn parse_rules(header: &str) -> Option<Vec<Rule>> {
    header
        .split(',')
        .map(|rule| {
            let mut parts = rule.trim().split(':');
            Some(Rule {
                max: parts.next()?.parse().ok()?,
                period_secs: parts.next()?.parse().ok()?,
            })
        })
        .collect()
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or_default()
}

fn path() -> Result<PathBuf> {
    let dir = dirs::cache_dir()
        .context("cannot determine the user cache directory")?
        .join("poe2");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("trade-limits.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(rules: &[(u32, u64)], hits: &[f64]) -> Limits {
        let mut limits = Limits::default();
        let state = limits.state(Policy::Search);
        state.rules = rules
            .iter()
            .map(|&(max, period_secs)| Rule { max, period_secs })
            .collect();
        state.hits = hits.to_vec();
        limits
    }

    #[test]
    fn allows_requests_under_the_limit() {
        let mut limits = limits(&[(5, 10)], &[100.0, 101.0]);

        assert_eq!(limits.wait_for(Policy::Search, 102.0), Duration::ZERO);
    }

    #[test]
    fn waits_for_the_oldest_hit_to_leave_the_window() {
        let mut limits = limits(&[(2, 10)], &[100.0, 105.0]);

        // The hit at 100 leaves the 10s window at 110; one extra second of margin.
        assert_eq!(
            limits.wait_for(Policy::Search, 106.0),
            Duration::from_secs(5)
        );
    }

    #[test]
    fn respects_a_lockout() {
        let mut limits = limits(&[(5, 10)], &[]);
        limits.state(Policy::Search).blocked_until = 160.0;

        assert_eq!(
            limits.wait_for(Policy::Search, 100.0),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn learns_rules_and_lockouts_from_headers() {
        let mut limits = Limits::default();

        limits.update(
            Policy::Fetch,
            Some("12:4:10,16:12:300"),
            Some("1:4:0,17:12:300"),
            None,
        );

        let state = limits.state(Policy::Fetch);
        assert_eq!(
            state.rules,
            vec![
                Rule {
                    max: 12,
                    period_secs: 4
                },
                Rule {
                    max: 16,
                    period_secs: 12
                }
            ]
        );
        assert!(state.blocked_until > now() + 290.0);
    }
}
