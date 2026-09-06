//! Anthropic list prices per million tokens. Cache writes are 1.25x (5m) and
//! 2x (1h) the input rate, reads 0.1x, except where a model says otherwise.

use super::model::Usage;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    pub input: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub cache_read: f64,
    pub output: f64,
}

const fn standard(input: f64, output: f64) -> Price {
    Price {
        input,
        cache_write_5m: input * 1.25,
        cache_write_1h: input * 2.0,
        cache_read: input * 0.1,
        output,
    }
}

// Longest prefix first: "claude-fable-5" would otherwise swallow "claude-fable-5-1".
// ponytail: static table, edit on a price change; a fetch from the Models API
// if this drifts too often.
const PRICES: &[(&str, Price)] = &[
    (
        "claude-fable-5-1",
        Price {
            cache_read: 0.25,
            ..standard(10.0, 50.0)
        },
    ),
    (
        "claude-mythos-5-1",
        Price {
            cache_read: 0.25,
            ..standard(10.0, 50.0)
        },
    ),
    ("claude-fable-5", standard(10.0, 50.0)),
    ("claude-mythos-5", standard(10.0, 50.0)),
    ("claude-opus-5", standard(5.0, 25.0)),
    ("claude-opus-4-8", standard(5.0, 25.0)),
    ("claude-opus-4-7", standard(5.0, 25.0)),
    ("claude-opus-4-6", standard(5.0, 25.0)),
    ("claude-sonnet-5", standard(2.0, 10.0)),
    ("claude-sonnet-4-6", standard(3.0, 15.0)),
    ("claude-haiku-4-5", standard(1.0, 5.0)),
];

pub fn price(model: &str) -> Option<Price> {
    PRICES
        .iter()
        .find(|(prefix, _)| model.starts_with(prefix))
        .map(|(_, p)| *p)
}

impl Price {
    /// Dollars for one call's usage.
    pub fn cost(&self, u: &Usage) -> f64 {
        (u.input as f64 * self.input
            + u.cache_write_5m as f64 * self.cache_write_5m
            + u.cache_write_1h as f64 * self.cache_write_1h
            + u.cache_read as f64 * self.cache_read
            + u.output as f64 * self.output)
            / 1e6
    }

    /// Dollars to re-read `tokens` once.
    pub fn reread(&self, tokens: u64) -> f64 {
        tokens as f64 * self.cache_read / 1e6
    }
}

/// `None` as soon as one part is unknown: a partial sum reads as the whole.
pub fn sum(parts: impl IntoIterator<Item = Option<f64>>) -> Option<f64> {
    parts.into_iter().try_fold(0.0, |acc, p| Some(acc + p?))
}

#[cfg(test)]
#[path = "pricing/tests.rs"]
mod tests;
