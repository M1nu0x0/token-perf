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

#[test]
fn a_leading_minus_flips_the_sort_and_a_typo_is_an_error() {
    use super::{SortField, Sort, parse_sort};
    assert_eq!(
        parse_sort("calls"),
        Ok(Sort {
            field: SortField::Calls,
            asc: false
        })
    );
    assert_eq!(
        parse_sort("-calls"),
        Ok(Sort {
            field: SortField::Calls,
            asc: true
        })
    );
    for field in ["date", "cache_read", "sub_read", "output", "cost"] {
        assert!(parse_sort(field).is_ok(), "{field}");
    }
    assert!(parse_sort("residual").is_err());
    assert!(parse_sort("-").is_err());
    assert!(parse_sort("").is_err());
}

#[test]
fn grep_matches_any_of_title_project_and_id_ignoring_case() {
    use super::matches_grep;
    let row = ["Ship the CLI", "/Users/me/Code/token-perf", "30ee1d12-cd17"];
    assert!(matches_grep(row, "token"));
    assert!(matches_grep(row, "CLI"));
    assert!(matches_grep(row, "ship"));
    assert!(matches_grep(row, "30EE1D12"));
    assert!(!matches_grep(row, "nothing"));
}

#[test]
fn the_spike_cut_is_the_tenth_percentile_from_the_top() {
    use super::spike_cut;
    let grew: Vec<u64> = (1..=20).collect();
    // 20 calls → ceil(2.0) = the 2nd largest, so 20 and 19 are spikes.
    assert_eq!(spike_cut(&grew), 19);
    // 5 calls → ceil(0.5) = the largest alone.
    assert_eq!(spike_cut(&[3, 9, 1, 4, 5]), 9);
    assert_eq!(spike_cut(&[7]), 7);
    assert_eq!(spike_cut(&[]), 0);
}

#[test]
fn top_keeps_the_biggest_spikes_but_shows_them_in_call_order() {
    use super::top_spikes;
    // 12 calls all above the cut (cut = 2nd largest = 12 would drop most), so
    // give them equal-ish sizes: every one is a candidate, --top picks 3.
    let grew: Vec<u64> = vec![10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10];
    assert_eq!(top_spikes(&grew, 3), vec![0, 1, 2], "ties keep call order");
    let grew = vec![5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 9, 7];
    // cut = 2nd largest = 7, so the two late ones are the only candidates.
    assert_eq!(top_spikes(&grew, 3), vec![10, 11]);
    let grew = vec![9, 1, 8, 0, 7, 1, 6, 0, 5, 4, 3, 2];
    // cut = 2nd largest = 8; --top 3 keeps 9/8, still ascending by index.
    assert_eq!(top_spikes(&grew, 3), vec![0, 2]);
    assert_eq!(top_spikes(&grew, 1), vec![0], "the biggest, not the first");
    assert!(top_spikes(&[0, 0, 0], 3).is_empty(), "a zero never spikes");
    assert!(top_spikes(&[], 3).is_empty());
}
