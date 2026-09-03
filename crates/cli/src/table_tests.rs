use super::{Align, render_ascii};

#[test]
fn a_short_name_still_right_aligns_flush_with_a_longer_header() {
    let out = render_ascii(
        "  ",
        &["TOOL", "CALLS", "RESIDUAL"],
        &[Align::Left, Align::Right, Align::Right],
        &[vec!["Read".into(), "5".into(), "146.8K".into()]],
        false,
    );
    let mut lines = out.lines();
    let header = lines.next().unwrap();
    let row = lines.next().unwrap();
    assert_eq!(header.len(), row.len());
    assert!(
        header.len() < 30,
        "header line grew back to a fixed width: {header:?}"
    );
}

#[test]
fn a_trailing_column_is_appended_unpadded() {
    let out = render_ascii(
        "  ",
        &["CALL", "RESIDUAL", "TOOLS"],
        &[Align::Left, Align::Right],
        &[vec!["11".into(), "119.4K".into(), "Bash, Read".into()]],
        true,
    );
    assert_eq!(out.lines().nth(1).unwrap(), "  11     119.4K  Bash, Read");
}
