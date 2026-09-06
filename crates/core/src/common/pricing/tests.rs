use super::{price, sum};
use crate::common::model::Usage;

#[test]
fn the_longest_prefix_wins() {
    assert_eq!(price("claude-fable-5-1").unwrap().cache_read, 0.25);
    assert_eq!(price("claude-fable-5").unwrap().cache_read, 1.0);
    assert_eq!(price("claude-haiku-4-5-20251001").unwrap().input, 1.0);
    assert!(price("gpt-4").is_none());
}

#[test]
fn a_call_is_priced_per_field() {
    let u = Usage {
        input: 1_000_000,
        cache_write_5m: 1_000_000,
        cache_write_1h: 1_000_000,
        cache_read: 1_000_000,
        output: 1_000_000,
        thinking: 0,
    };
    let p = price("claude-opus-5").unwrap();
    assert_eq!(p.cost(&u), 5.0 + 6.25 + 10.0 + 0.5 + 25.0);
    assert_eq!(p.reread(2_000_000), 1.0);
}

#[test]
fn one_unknown_part_makes_the_sum_unknown() {
    assert_eq!(sum([Some(1.0), Some(2.0)]), Some(3.0));
    assert_eq!(sum([Some(1.0), None]), None);
    assert_eq!(sum([]), Some(0.0));
}
