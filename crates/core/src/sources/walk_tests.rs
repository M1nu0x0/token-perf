use super::walk;
use std::os::unix::fs::symlink;

#[test]
fn walk_terminates_on_symlinked_directory_cycle() {
    let root = std::env::temp_dir().join(format!("token-perf-walk-cycle-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cycle_link = root.join("back_to_root");
    symlink(&root, &cycle_link).unwrap();

    let mut out = Vec::new();
    walk(&root, "jsonl", &mut out);

    std::fs::remove_file(&cycle_link).ok();
    std::fs::remove_dir_all(&root).ok();

    assert!(out.is_empty());
}

#[test]
fn walk_skips_jsonl_reachable_only_through_symlinked_dir() {
    let root = std::env::temp_dir().join(format!(
        "token-perf-walk-symlink-only-{}",
        std::process::id()
    ));
    let real_dir =
        std::env::temp_dir().join(format!("token-perf-walk-real-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&real_dir).unwrap();
    std::fs::write(real_dir.join("session.jsonl"), b"{}").unwrap();
    let link = root.join("linked");
    symlink(&real_dir, &link).unwrap();

    let mut out = Vec::new();
    walk(&root, "jsonl", &mut out);

    std::fs::remove_file(&link).ok();
    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_dir_all(&real_dir).ok();

    assert!(out.is_empty());
}
