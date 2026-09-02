use std::cmp::Reverse;

use clap::{Parser, Subcommand};
use rust_i18n::t;
use token_perf_core::{common::analyze, store::Store, sync_and_load};

// `--help` stays English: clap doc comments are compile-time literals.
rust_i18n::i18n!("locales", fallback = "en");

const LANGS: [&str; 3] = ["en", "ko", "ja"];

#[derive(Parser)]
#[command(
    name = "token-perf",
    version,
    about = "Find where your tokens leak, from local agent logs"
)]
struct Cli {
    /// Interface language (en, ko, ja). Saved for later runs.
    #[arg(long, global = true)]
    lang: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List sessions, most re-billed cache first
    Sessions {
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// Include subagent sessions (top-level only by default)
        #[arg(long)]
        all: bool,
    },
    /// Attribute one session's waste to the tools that caused it (defaults to the latest)
    Report {
        session: Option<String>,
        #[arg(long, default_value_t = 15)]
        top: usize,
    },
    /// Aggregate every session, most re-billed first
    Summary {
        /// Earliest session date to include (YYYY-MM-DD)
        #[arg(long)]
        since: Option<String>,
        /// Latest session date to include (YYYY-MM-DD)
        #[arg(long)]
        until: Option<String>,
    },
    /// Start the web UI and open it in a browser
    Serve {
        /// Port to bind. Omit and the OS picks a free one
        #[arg(long)]
        port: Option<u16>,
        /// Do not open a browser
        #[arg(long)]
        no_open: bool,
    },
}

/// Format only: month and day ranges, not whether the day exists in that month.
fn valid_date(v: &str) -> bool {
    let b = v.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    if !([0, 1, 2, 3, 5, 6, 8, 9].iter()).all(|&i| b[i].is_ascii_digit()) {
        return false;
    }
    let two = |i: usize| (b[i] - b'0') * 10 + (b[i + 1] - b'0');
    (1..=12).contains(&two(5)) && (1..=31).contains(&two(8))
}

/// ISO 8601 sorts lexicographically, so the date prefix compares as a date.
/// Comparing the prefix and not the whole stamp keeps `until` inclusive.
fn in_range(started_at: &str, since: Option<&str>, until: Option<&str>) -> bool {
    let Some(day) = started_at.get(..10) else {
        return since.is_none() && until.is_none();
    };
    since.is_none_or(|s| day >= s) && until.is_none_or(|u| day <= u)
}

fn supported(code: &str) -> Option<&'static str> {
    let short = code.split(['_', '.', '-']).next().unwrap_or_default();
    LANGS.into_iter().find(|l| *l == short)
}

fn resolve_lang(flag: Option<&str>, saved: Option<&str>, env: Option<&str>) -> &'static str {
    flag.or(saved).or(env).and_then(supported).unwrap_or("en")
}

/// POSIX: LC_ALL overrides LANG.
fn env_lang(lc_all: Option<&str>, lang: Option<&str>) -> Option<String> {
    lc_all
        .or(lang)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn process_lang() -> Option<String> {
    env_lang(
        std::env::var("LC_ALL").ok().as_deref(),
        std::env::var("LANG").ok().as_deref(),
    )
}

/// Set twice so a store failure still reports in a sensible language.
fn apply_lang(store: &Store, flag: Option<&str>) -> String {
    let saved = store.setting("lang").ok().flatten();
    let lang = resolve_lang(flag, saved.as_deref(), process_lang().as_deref());
    if flag.is_some() {
        store.set_setting("lang", lang).ok();
    }
    rust_i18n::set_locale(lang);
    lang.to_string()
}

/// A typo must not erase a stored preference: an unknown code is dropped, not saved.
fn split_lang_flag(flag: Option<&str>) -> (Option<&str>, Option<&str>) {
    match flag {
        Some(code) if supported(code).is_none() => (None, Some(code)),
        code => (code, None),
    }
}

fn main() {
    let cli = Cli::parse();
    let (flag, unknown) = split_lang_flag(cli.lang.as_deref());
    rust_i18n::set_locale(resolve_lang(flag, None, process_lang().as_deref()));

    let (store, sessions, scanned) = match sync_and_load() {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("{}", t!("store_open_failed", error = e));
            std::process::exit(1);
        }
    };
    let lang = apply_lang(&store, flag);
    if let Some(code) = unknown {
        eprintln!("{}", t!("unknown_lang", code = code));
    }
    if scanned.files_read > 0 {
        eprintln!(
            "{}",
            t!(
                "scanned",
                seen = scanned.files_seen,
                read = scanned.files_read,
                calls = scanned.calls_written
            )
        );
    }
    if scanned.files_skipped > 0 {
        eprintln!("{}", t!("skipped", count = scanned.files_skipped));
    }
    // serve must start with no sessions: CI has no ~/.claude at all.
    if sessions.is_empty() && !matches!(cli.command, Command::Serve { .. }) {
        eprintln!("{}", t!("no_sessions"));
        std::process::exit(1);
    }

    match cli.command {
        Command::Sessions { top, all } => {
            let mut reports: Vec<_> = sessions
                .iter()
                .filter(|s| all || s.parent.is_none())
                .map(analyze::session)
                .collect();
            reports.sort_by_key(|r| Reverse(r.totals.cache_read));

            println!(
                "{:<8} {:<10} {:>6} {:>10} {:>10}  {:<24} TITLE",
                "ID", "DATE", "CALLS", "CACHE_RD", "OUTPUT", "PROJECT"
            );
            for r in reports.iter().take(top) {
                println!(
                    "{:<8} {:<10} {:>6} {:>10} {:>10}  {:<24} {}{}",
                    short(&r.session),
                    r.started_at.get(..10).unwrap_or(""),
                    r.call_count,
                    human(r.totals.cache_read),
                    human(r.totals.output),
                    basename(&r.project),
                    if r.parent.is_some() { "↳ " } else { "" },
                    r.title
                );
            }
        }

        Command::Report { session, top } => {
            let latest = |only_top: bool| {
                sessions
                    .iter()
                    .filter(|s| !only_top || s.parent.is_none())
                    .max_by(|a, b| a.started_at.cmp(&b.started_at))
            };
            let target = match &session {
                // A short id can prefix a longer one; an exact hit is not ambiguous.
                Some(id) if sessions.iter().any(|s| s.id == *id) => {
                    sessions.iter().find(|s| s.id == *id)
                }
                Some(id) => {
                    let matches: Vec<_> =
                        sessions.iter().filter(|s| s.id.starts_with(id)).collect();
                    if matches.len() > 1 {
                        eprintln!("{}", t!("ambiguous", prefix = id));
                        for s in matches {
                            eprintln!("  {}  {}", s.id, s.title);
                        }
                        std::process::exit(1);
                    }
                    matches.into_iter().next()
                }
                // Subagents are not the default target unless they are all there is.
                None => latest(true).or_else(|| latest(false)),
            };
            let Some(target) = target else {
                eprintln!("{}", t!("not_found"));
                std::process::exit(1);
            };

            let r = analyze::session(target);
            println!("session  {}  ({})", r.session, r.source);
            if !r.title.is_empty() {
                println!("title    {}", r.title);
            }
            if let Some(parent) = &r.parent {
                let subagent = t!("subagent");
                let kind = r.agent_type.as_deref().unwrap_or(&subagent);
                let depth = r
                    .spawn_depth
                    .map_or(String::new(), |d| t!("depth", depth = d).into_owned());
                println!("parent   {parent}  ({kind}{depth})");
                if let Some(tool_use) = &r.parent_tool_use_id {
                    println!("spawned  {tool_use}");
                }
            }
            println!("project  {}", r.project);
            println!("started  {}", r.started_at);
            println!();
            println!(
                "{}",
                t!(
                    "totals",
                    calls = r.call_count,
                    output = human(r.totals.output),
                    cache_read = human(r.totals.cache_read),
                    cache_write = human(r.totals.cache_write())
                )
            );
            println!(
                "{}",
                t!(
                    "baseline",
                    baseline = human(r.baseline),
                    billed = human(r.baseline_billed)
                )
            );
            if r.totals.cache_write_1h > 0 {
                println!(
                    "{}",
                    t!(
                        "cache_writes",
                        write_5m = human(r.totals.cache_write_5m),
                        write_1h = human(r.totals.cache_write_1h)
                    )
                );
            }
            if r.failed_calls > 0 {
                let kinds = match r.error_kinds.is_empty() {
                    true => String::new(),
                    false => format!(" ({})", r.error_kinds.join(", ")),
                };
                println!(
                    "{}",
                    t!("failed_calls", count = r.failed_calls, kinds = kinds)
                );
            }
            println!();

            println!("{}", t!("tool_residual"));
            println!(
                "  {:<38} {:>6} {:>10} {:>10}",
                "TOOL", "CALLS", "ADDED", "RESIDUAL"
            );
            for t in r.tools.iter().take(top) {
                println!(
                    "  {:<38} {:>6} {:>10} {:>10}",
                    t.name,
                    t.calls,
                    human(t.added),
                    human(t.residual)
                );
            }
            println!();

            println!("{}", t!("worst_calls"));
            println!(
                "  {:<6} {:>10} {:>10} {:>10}  TOOLS",
                "CALL", "CONTEXT", "GREW", "RESIDUAL"
            );
            let mut worst: Vec<_> = r.calls.iter().collect();
            worst.sort_by_key(|c| Reverse(c.residual));
            for c in worst.iter().take(top) {
                println!(
                    "  {:<6} {:>10} {:>10} {:>10}  {}",
                    c.index,
                    human(c.context),
                    human(c.grew_by),
                    human(c.residual),
                    c.tools.join(", ")
                );
            }
        }

        Command::Summary { since, until } => {
            for value in [&since, &until].into_iter().flatten() {
                if !valid_date(value) {
                    eprintln!("{}", t!("bad_date", value = value));
                    std::process::exit(1);
                }
            }
            let mut sessions = sessions;
            sessions.retain(|s| in_range(&s.started_at, since.as_deref(), until.as_deref()));
            if sessions.is_empty() {
                println!("{}", t!("summary_empty"));
                return;
            }

            let d = analyze::tldr(&sessions);
            println!(
                "{}",
                t!("summary_lead", sessions = d.sessions, calls = d.calls)
            );
            println!(
                "{}",
                t!(
                    "summary_cache_read",
                    pct = format!("{:.0}", d.cache_read_pct),
                    output = human(d.totals.output),
                    cache_read = human(d.totals.cache_read)
                )
            );
            println!();

            println!("{}", t!("summary_length"));
            println!("  {:<38} {:>10} {:>7}", "LENGTH", "SESSIONS", "RATIO");
            for b in &d.amplification {
                let label = match b.max {
                    Some(max) => format!("{}-{}", b.min, max),
                    None => format!("{}+", b.min),
                };
                println!("  {:<38} {:>10} {:>6.1}x", label, b.sessions, b.ratio);
            }
            println!();

            println!("{}", t!("summary_tools"));
            println!("  {:<38} {:>10} {:>7}", "TOOL", "ADDED", "SHARE");
            for t in &d.top_tools {
                println!("  {:<38} {:>10} {:>6.0}%", t.name, human(t.added), t.pct);
            }
            println!();

            println!("{}", t!("summary_top_sessions"));
            println!("  {:<8} {:>6} {:>10}  TITLE", "ID", "CALLS", "RESIDUAL");
            for s in &d.top_sessions {
                println!(
                    "  {:<8} {:>6} {:>10}  {}",
                    short(&s.session),
                    s.call_count,
                    human(s.residual),
                    s.title
                );
            }
            println!();

            println!(
                "{}",
                t!("summary_solo", pct = format!("{:.0}", d.solo_tool_pct))
            );
        }

        Command::Serve { port, no_open } => serve(sessions, lang, port.unwrap_or(0), !no_open),
    }
}

fn serve(sessions: Vec<token_perf_core::Session>, lang: String, port: u16, open: bool) {
    let count = sessions.len();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

    // The callback fires after the listener is open, so the browser is never refused.
    let result = runtime.block_on(token_perf_core::server::serve(
        sessions,
        lang,
        port,
        |bound| {
            let url = format!("http://localhost:{bound}");
            println!("{}", t!("serving", url = url, count = count));
            println!("{}", t!("stop_hint"));
            if open {
                open_browser(&url);
            }
        },
    ));

    if let Err(e) = result {
        eprintln!("{}", t!("serve_failed", error = e));
        std::process::exit(1);
    }
}

fn open_browser(url: &str) {
    let mut command = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    } else if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        // start eats its first argument as a window title, hence the empty string.
        c.args(["/C", "start", "", url]);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    command.spawn().ok();
}

fn short(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn human(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}K", n as f64 / 1e3),
        1_000_000..=999_999_999 => format!("{:.1}M", n as f64 / 1e6),
        _ => format!("{:.1}B", n as f64 / 1e9),
    }
}

#[cfg(test)]
mod tests {
    use super::{env_lang, in_range, resolve_lang, split_lang_flag, valid_date};

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
        // Format only: the calendar itself is not checked.
        assert!(valid_date("2026-02-31"));
        assert!(!valid_date("2026-13-99"));
        assert!(!valid_date("2026-00-01"));
        assert!(!valid_date("2026-8-1"));
        assert!(!valid_date("2026/08/01"));
        assert!(!valid_date("2026-08-01T00:00:00Z"));
        assert!(!valid_date(""));
    }

    #[test]
    fn the_flag_beats_the_setting_and_the_setting_beats_the_environment() {
        assert_eq!(resolve_lang(Some("ja"), Some("ko"), Some("en_US")), "ja");
        assert_eq!(resolve_lang(None, Some("ko"), Some("en_US")), "ko");
        assert_eq!(resolve_lang(None, None, Some("ja_JP.UTF-8")), "ja");
        assert_eq!(resolve_lang(None, None, None), "en");
    }

    #[test]
    fn an_unsupported_code_falls_back_to_english_without_trying_the_next_source() {
        assert_eq!(resolve_lang(None, Some("fr"), Some("ko_KR")), "en");
        assert_eq!(resolve_lang(None, None, Some("C")), "en");
        assert_eq!(resolve_lang(None, None, Some("zh_CN.UTF-8")), "en");
    }

    #[test]
    fn an_unsupported_flag_is_dropped_but_kept_for_the_warning() {
        assert_eq!(split_lang_flag(Some("fr")), (None, Some("fr")));
        assert_eq!(split_lang_flag(Some("ja")), (Some("ja"), None));
        assert_eq!(split_lang_flag(None), (None, None));
    }

    #[test]
    fn lc_all_outranks_lang() {
        assert_eq!(
            env_lang(Some("ja_JP.UTF-8"), Some("ko_KR")).as_deref(),
            Some("ja_JP.UTF-8")
        );
        assert_eq!(env_lang(None, Some("ko_KR")).as_deref(), Some("ko_KR"));
        assert_eq!(env_lang(None, None), None);
        // An empty LC_ALL is a set variable, so LANG never gets a turn.
        assert_eq!(env_lang(Some(""), Some("ko_KR")), None);
    }

    #[test]
    fn a_locale_carries_a_region_and_an_encoding() {
        assert_eq!(resolve_lang(None, None, Some("ko_KR.UTF-8")), "ko");
        assert_eq!(resolve_lang(Some("en-US"), None, None), "en");
    }
}
