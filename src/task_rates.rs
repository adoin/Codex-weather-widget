//! Per-session rolling output throughput from timestamped Codex events.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;

const FRESH_SECONDS: i64 = 30;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct TaskActivity {
    pub id: String,
    pub running: bool,
    pub updated_at: Option<DateTime<Utc>>,
    pub output_tokens: u64,
    previous_output: Option<u64>,
    anchor: Option<DateTime<Utc>>,
    last_output_at: Option<DateTime<Utc>>,
    samples: VecDeque<Sample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sample {
    at: DateTime<Utc>,
    tokens: u64,
    seconds: f64,
}

impl TaskActivity {
    pub fn observe(&mut self, payload: &Value, at: DateTime<Utc>) {
        if self.updated_at.is_some_and(|last| at < last) {
            return;
        }
        match payload.get("type").and_then(Value::as_str) {
            Some("task_started") => {
                self.running = true;
                self.output_tokens = 0;
                self.samples.clear();
                self.last_output_at = None;
                self.anchor = Some(at);
                self.updated_at = Some(at);
            }
            Some("task_complete" | "turn_aborted") => {
                self.running = false;
                self.updated_at = Some(at);
            }
            Some("token_count") => {
                let Some(total) = payload
                    .pointer("/info/total_token_usage/output_tokens")
                    .and_then(Value::as_u64)
                else {
                    return;
                };
                let last = payload
                    .pointer("/info/last_token_usage/output_tokens")
                    .and_then(Value::as_u64);
                let delta = match self.previous_output {
                    Some(previous) if total >= previous => total - previous,
                    // First observation may contain a replayed cumulative total.
                    _ => last.unwrap_or(0).min(total),
                };
                self.previous_output = Some(total);
                if delta == 0 {
                    return;
                }
                self.running = true;
                self.output_tokens = self.output_tokens.saturating_add(delta);
                if let Some(anchor) = self.anchor {
                    let seconds = (at - anchor).num_milliseconds() as f64 / 1000.0;
                    // No denominator or zero timestamp resolution means no estimate.
                    if seconds > 0.0 {
                        self.samples.push_back(Sample {
                            at,
                            tokens: delta,
                            seconds,
                        });
                    }
                }
                self.anchor = Some(at);
                self.last_output_at = Some(at);
                self.updated_at = Some(at);
                while self
                    .samples
                    .front()
                    .is_some_and(|s| (at - s.at).num_seconds() > FRESH_SECONDS)
                {
                    self.samples.pop_front();
                }
            }
            _ => {}
        }
    }

    pub fn tps(&self, now: DateTime<Utc>) -> Option<f64> {
        let age = (now - self.last_output_at?).num_milliseconds();
        if !self.running || !(0..=FRESH_SECONDS * 1000).contains(&age) {
            return None;
        }
        let (tokens, seconds) = self
            .samples
            .iter()
            .filter(|s| (now - s.at).num_seconds() <= FRESH_SECONDS)
            .fold((0u64, 0.0), |(n, t), s| {
                (n.saturating_add(s.tokens), t + s.seconds)
            });
        (tokens > 0 && seconds > 0.0).then_some(tokens as f64 / seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use serde_json::json;
    fn count(output: u64, last: u64) -> Value {
        json!({"type":"token_count","info":{"total_token_usage":{"output_tokens":output,"input_tokens":999999},"last_token_usage":{"output_tokens":last}}})
    }
    #[test]
    fn sessions_are_independent_and_input_tokens_are_not_throughput() {
        let at = Utc.timestamp_opt(1800000000, 0).unwrap();
        let mut a = TaskActivity::default();
        let mut b = TaskActivity::default();
        for x in [&mut a, &mut b] {
            x.observe(&json!({"type":"task_started"}), at);
        }
        a.observe(&count(10000, 100), at + Duration::seconds(2));
        b.observe(&count(500, 20), at + Duration::seconds(2));
        assert_eq!(a.tps(at + Duration::seconds(2)), Some(50.0));
        assert_eq!(b.tps(at + Duration::seconds(2)), Some(10.0));
        a.observe(&count(10000, 100), at + Duration::seconds(3));
        assert_eq!(a.output_tokens, 100);
        assert_eq!(a.tps(at + Duration::seconds(40)), None);
        b.observe(&json!({"type":"task_complete"}), at + Duration::seconds(3));
        assert_eq!(b.tps(at + Duration::seconds(3)), None);
    }
    #[test]
    fn resets_and_new_turns_do_not_create_lifetime_token_spikes() {
        let at = Utc.timestamp_opt(1800000000, 0).unwrap();
        let mut a = TaskActivity::default();
        a.observe(&count(10000, 100), at);
        assert_eq!(a.tps(at), None);
        a.observe(&json!({"type":"task_started"}), at + Duration::seconds(1));
        a.observe(&count(40, 40), at + Duration::seconds(3));
        assert_eq!(a.tps(at + Duration::seconds(3)), Some(20.0));
        a.observe(&count(40, 40), at + Duration::seconds(3));
        assert_eq!(a.output_tokens, 40);
    }
}
