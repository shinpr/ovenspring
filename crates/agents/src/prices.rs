//! A run's cost at API prices, from the tokens it used: the same measure for every CLI, whatever
//! the CLI reports itself or however the owner pays for it.

use std::collections::BTreeMap;

use serde::Deserialize;

const PRICES: &str = include_str!("prices.toml");

/// USD per million tokens.
#[derive(Debug, Clone, Copy, Deserialize)]
struct Price {
    input: f64,
    cache_read: f64,
    cache_write: f64,
    output: f64,
}

/// Tokens a run used; `input` excludes what was read from or written to the cache.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

/// What `tokens` cost on `model` at its API price; `None` for a model the table has no price for,
/// and for a run that ended before reporting any tokens, whose cost is unknown rather than nothing.
/// OpenCode names a model with its provider (`anthropic/claude-opus-5-5`).
pub fn cost(model: &str, tokens: Tokens) -> Option<f64> {
    if tokens == Tokens::default() {
        return None;
    }
    let table: BTreeMap<String, Price> = toml::from_str(PRICES).ok()?;
    let name = model.rsplit('/').next().unwrap_or(model);
    let p = table.get(name)?;
    let per = |n: u64, usd: f64| millions(n) * usd;
    Some(
        per(tokens.input, p.input)
            + per(tokens.cache_read, p.cache_read)
            + per(tokens.cache_write, p.cache_write)
            + per(tokens.output, p.output),
    )
}

/// `n` in millions, exactly for any token count a run reaches.
fn millions(n: u64) -> f64 {
    let high = u32::try_from(n >> 32).unwrap_or(u32::MAX);
    let low = u32::try_from(n & u64::from(u32::MAX)).unwrap_or(u32::MAX);
    (f64::from(high) * 4_294_967_296.0 + f64::from(low)) / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_is_priced_by_its_model_and_each_kind_of_token() {
        let tokens = Tokens {
            input: 1_000_000,
            output: 100_000,
            cache_read: 2_000_000,
            cache_write: 0,
        };
        let close = |a: Option<f64>, b: f64| a.is_some_and(|a| (a - b).abs() < 1e-9);
        // gpt-6.2-sol: 2.0 + 2 x 0.10 + 0.1 x 10.0.
        assert!(close(cost("gpt-6.2-sol", tokens), 3.2));
        let input = Tokens {
            input: 1_000_000,
            ..Tokens::default()
        };
        assert!(close(cost("anthropic/claude-opus-5-5", input), 4.0));
        assert_eq!(cost("unknown-model", tokens), None);
        assert_eq!(cost("gpt-6.2-sol", Tokens::default()), None);
    }
}
