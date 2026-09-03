mod lang;
mod table;

use std::cmp::Reverse;

use clap::{Parser, Subcommand};
use rust_i18n::t;
use table::Align;
use token_perf_core::{common::analyze, sync_and_load};

// `--help` stays English: clap doc comments are compile-time literals.
rust_i18n::i18n!("locales", fallback = "en");

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
    /// Re-read every transcript from the start instead of only what is new
    #[arg(long, global = true)]
    rescan: bool,
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
    /// Show persisted settings, or change one
    Config {
        /// Draw tables with Unicode borders instead of plain aligned columns (on/off)
        #[arg(long, value_parser = clap::builder::BoolishValueParser::new())]
        pretty: Option<bool>,
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

fn main() {
    let cli = Cli::parse();
    let (flag, unknown) = lang::split_lang_flag(cli.lang.as_deref());
    rust_i18n::set_locale(lang::resolve_lang(
        flag,
        None,
        lang::process_lang().as_deref(),
    ));

    let (store, sessions, scanned) = match sync_and_load(cli.rescan) {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("{}", t!("store_open_failed", error = e));
            std::process::exit(1);
        }
    };
    let lang = lang::apply_lang(&store, flag);
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
    // serve and config must start with no sessions: CI has no ~/.claude at all.
    if sessions.is_empty() && !matches!(cli.command, Command::Serve { .. } | Command::Config { .. })
    {
        eprintln!("{}", t!("no_sessions"));
        std::process::exit(1);
    }

    let pretty_flag = match &cli.command {
        Command::Config { pretty } => *pretty,
        _ => None,
    };
    let pretty = table::apply_pretty(&store, pretty_flag);

    match cli.command {
        Command::Sessions { top, all } => {
            let mut reports: Vec<_> = sessions
                .iter()
                .filter(|s| all || s.parent.is_none())
                .map(analyze::session)
                .collect();
            reports.sort_by_key(|r| Reverse(r.totals.cache_read));

            let rows: Vec<Vec<String>> = reports
                .iter()
                .take(top)
                .map(|r| {
                    vec![
                        short(&r.session).to_string(),
                        r.started_at.get(..10).unwrap_or("").to_string(),
                        r.call_count.to_string(),
                        human(r.totals.cache_read),
                        human(r.totals.output),
                        basename(&r.project).to_string(),
                        format!("{}{}", if r.parent.is_some() { "↳ " } else { "" }, r.title),
                    ]
                })
                .collect();
            table::print_table(
                pretty,
                "",
                &[
                    "ID", "DATE", "CALLS", "CACHE_RD", "OUTPUT", "PROJECT", "TITLE",
                ],
                &[
                    Align::Left,
                    Align::Left,
                    Align::Right,
                    Align::Right,
                    Align::Right,
                    Align::Left,
                ],
                &rows,
                true,
            );
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
            let tool_rows: Vec<Vec<String>> = r
                .tools
                .iter()
                .take(top)
                .map(|t| {
                    vec![
                        t.name.clone(),
                        t.calls.to_string(),
                        human(t.added),
                        human(t.residual),
                    ]
                })
                .collect();
            table::print_table(
                pretty,
                "  ",
                &["TOOL", "CALLS", "ADDED", "RESIDUAL"],
                &[Align::Left, Align::Right, Align::Right, Align::Right],
                &tool_rows,
                false,
            );
            println!();

            println!("{}", t!("worst_calls"));
            let mut worst: Vec<_> = r.calls.iter().collect();
            worst.sort_by_key(|c| Reverse(c.residual));
            let call_rows: Vec<Vec<String>> = worst
                .iter()
                .take(top)
                .map(|c| {
                    vec![
                        c.index.to_string(),
                        human(c.context),
                        human(c.grew_by),
                        human(c.residual),
                        c.tools.join(", "),
                    ]
                })
                .collect();
            table::print_table(
                pretty,
                "  ",
                &["CALL", "CONTEXT", "GREW", "RESIDUAL", "TOOLS"],
                &[Align::Left, Align::Right, Align::Right, Align::Right],
                &call_rows,
                true,
            );
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
            let length_rows: Vec<Vec<String>> = d
                .amplification
                .iter()
                .map(|b| {
                    let label = match b.max {
                        Some(max) => format!("{}-{}", b.min, max),
                        None => format!("{}+", b.min),
                    };
                    vec![label, b.sessions.to_string(), format!("{:.1}x", b.ratio)]
                })
                .collect();
            table::print_table(
                pretty,
                "  ",
                &["LENGTH", "SESSIONS", "RATIO"],
                &[Align::Left, Align::Right, Align::Right],
                &length_rows,
                false,
            );
            println!();

            println!("{}", t!("summary_tools"));
            let tool_rows: Vec<Vec<String>> = d
                .top_tools
                .iter()
                .map(|t| vec![t.name.clone(), human(t.added), format!("{:.0}%", t.pct)])
                .collect();
            table::print_table(
                pretty,
                "  ",
                &["TOOL", "ADDED", "SHARE"],
                &[Align::Left, Align::Right, Align::Right],
                &tool_rows,
                false,
            );
            println!();

            println!("{}", t!("summary_top_sessions"));
            let session_rows: Vec<Vec<String>> = d
                .top_sessions
                .iter()
                .map(|s| {
                    vec![
                        short(&s.session).to_string(),
                        s.call_count.to_string(),
                        human(s.residual),
                        s.title.clone(),
                    ]
                })
                .collect();
            table::print_table(
                pretty,
                "  ",
                &["ID", "CALLS", "RESIDUAL", "TITLE"],
                &[Align::Left, Align::Right, Align::Right],
                &session_rows,
                true,
            );
            println!();

            println!(
                "{}",
                t!("summary_solo", pct = format!("{:.0}", d.solo_tool_pct))
            );
        }

        Command::Serve { port, no_open } => serve(sessions, lang, port.unwrap_or(0), !no_open),

        Command::Config { .. } => {
            println!("lang    {lang}");
            println!("pretty  {}", if pretty { "on" } else { "off" });
        }
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
#[path = "main_tests.rs"]
mod tests;
