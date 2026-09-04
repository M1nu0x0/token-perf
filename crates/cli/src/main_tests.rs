use super::{in_range, valid_date};

#[test]
fn a_range_includes_both_of_its_end_days_whole() {
    let day = |d: &str| format!("{d}T23:59:59.999Z");
    assert!(in_range(
        &day("2026-08-01"),
        Some("2026-08-01"),
        Some("2026-08-31")
    ));
    assert!(in_range(
        &day("2026-08-31"),
        Some("2026-08-01"),
        Some("2026-08-31")
    ));
    assert!(!in_range(&day("2026-07-31"), Some("2026-08-01"), None));
    assert!(!in_range(&day("2026-09-01"), None, Some("2026-08-31")));
}

#[test]
fn without_bounds_everything_stays_but_a_bound_drops_the_undated() {
    assert!(in_range("", None, None));
    assert!(in_range("2026-08", None, None));
    assert!(!in_range("", Some("2026-08-01"), None));
    assert!(!in_range("2026-08", None, Some("2026-08-31")));
}

#[test]
fn a_date_is_ten_characters_of_yyyy_mm_dd() {
    assert!(valid_date("2026-08-01"));
    assert!(valid_date("2026-02-31"));
    assert!(!valid_date("2026-13-99"));
    assert!(!valid_date("2026-00-01"));
    assert!(!valid_date("2026-8-1"));
    assert!(!valid_date("2026/08/01"));
    assert!(!valid_date("2026-08-01T00:00:00Z"));
    assert!(!valid_date(""));
}

#[test]
fn a_subagent_id_keeps_enough_to_tell_two_apart() {
    use super::short;
    assert_eq!(short("30ee1d12-cd17-4f02-809e-4785c774e22b"), "30ee1d12");
    assert_eq!(short("agent-adc935a5f207eb2f6"), "agent-adc935a5");
    assert_eq!(short("agent-a"), "agent-a", "shorter than the cut");
}
