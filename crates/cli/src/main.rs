mod lang;
mod table;

use std::cmp::Reverse;

use clap::{Parser, Subcommand};
use rust_i18n::t;
use table::Align;
use token_perf_core::{
    common::{analyze, pricing},
    sync_and_load,
};

rust_i18n::i18n!("locales", fallback = "en");

/// Help text is built while parsing, so the locale is set before `Cli::parse`
/// and the strings are looked up at runtime instead of coming from doc comments.
fn h(key: &str) -> String {
    t!(key).to_string()
}

#[derive(Parser)]
#[command(
    name = "token-perf",
    version,
    about = h("help.about")
)]
struct Cli {
    #[arg(long, global = true, help = h("help.lang"))]
    lang: Option<String>,
    #[arg(long, global = true, help = h("help.rescan"))]
    rescan: bool,
    #[arg(long, global = true, help = h("help.json"))]
    json: bool,
    #[arg(long, global = true, help = h("help.cost"))]
    cost: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = h("help.sessions"))]
    Sessions {
        #[arg(long, default_value_t = 20, help = h("help.sessions_top"))]
        top: usize,
        #[arg(long, help = h("help.sessions_all"))]
        all: bool,
        // Values start with '-' for ascending, so they must not read as flags.
        #[arg(long, allow_hyphen_values = true, value_parser = parse_sort, help = h("help.sessions_sort"))]
        sort: Option<Sort>,
        #[arg(long, help = h("help.sessions_grep"))]
        grep: Option<String>,
    },
    #[command(about = h("help.report"))]
    Report {
        #[arg(help = h("help.report_session"))]
        session: Option<String>,
        #[arg(long, default_value_t = 15, help = h("help.report_top"))]
        top: usize,
    },
    #[command(about = h("help.summary"))]
    Summary {
        #[arg(long, help = h("help.since"))]
        since: Option<String>,
        #[arg(long, help = h("help.until"))]
        until: Option<String>,
    },
    #[cfg(feature = "serve")]
    #[command(about = h("help.serve"))]
    Serve {
        #[arg(long, help = h("help.port"))]
        port: Option<u16>,
        #[arg(long, help = h("help.no_open"))]
        no_open: bool,
    },
    #[command(about = h("help.config"))]
    Config {
        #[arg(long, value_parser = clap::builder::BoolishValueParser::new(), help = h("help.pretty"))]
        pretty: Option<bool>,
    },
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum SortField {
    Date,
    Calls,
    CacheRead,
    SubRead,
    Output,
    Cost,
}

#[derive(Clone, PartialEq, Debug)]
struct Sort {
    field: SortField,
    asc: bool,
}

/// Same field names and `-` prefix as the web table's sort links.
fn parse_sort(value: &str) -> Result<Sort, String> {
    let (asc, name) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value),
    };
    let field = match name {
        "date" => SortField::Date,
        "calls" => SortField::Calls,
        "cache_read" => SortField::CacheRead,
        "sub_read" => SortField::SubRead,
        "output" => SortField::Output,
        "cost" => SortField::Cost,
        _ => return Err(t!("bad_sort", value = name).to_string()),
    };
    Ok(Sort { field, asc })
}

fn sort_reports(reports: &mut [analyze::Report], sort: &Sort) {
    // An unpriced session sorts below every priced one, as it does on the web.
    let cost = |r: &analyze::Report| pricing::sum([r.cost, r.subagents.cost]).unwrap_or(-1.0);
    let by = |a: &analyze::Report, b: &analyze::Report| match sort.field {
        SortField::Date => a.started_at.cmp(&b.started_at),
        SortField::Calls => a.call_count.cmp(&b.call_count),
        SortField::CacheRead => a.totals.cache_read.cmp(&b.totals.cache_read),
        SortField::SubRead => a
            .subagents
            .totals
            .cache_read
            .cmp(&b.subagents.totals.cache_read),
        SortField::Output => a.totals.output.cmp(&b.totals.output),
        SortField::Cost => cost(a).total_cmp(&cost(b)),
    };
    reports.sort_by(|a, b| if sort.asc { by(a, b) } else { by(b, a) });
}

/// Title, project and id searched as one string, exactly like the web search box.
fn matches_grep(fields: [&str; 3], query: &str) -> bool {
    fields.join(" ").to_lowercase().contains(&query.to_lowercase())
}

/// The 10% of calls that grew the context most: the cut is the 10th-from-the-top
/// value, the same one the web growth chart marks with dots.
fn spike_cut(grew: &[u64]) -> u64 {
    let mut sorted = grew.to_vec();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    let rank = (sorted.len() as f64 * 0.1).ceil() as usize;
    sorted.get(rank.saturating_sub(1)).copied().unwrap_or(0)
}

/// Positions of the spikes to show: `--top` cuts the smallest ones, never the
/// late ones, and what is left goes back in call order like the web curve.
fn top_spikes(grew: &[u64], top: usize) -> Vec<usize> {
    let cut = spike_cut(grew);
    let mut picked: Vec<usize> = (0..grew.len())
        .filter(|&i| grew[i] > 0 && grew[i] >= cut)
        .collect();
    picked.sort_by_key(|&i| Reverse(grew[i]));
    picked.truncate(top);
    picked.sort_unstable();
    picked
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
    // Before parsing, so --help comes out in the user's language.
    let argv_lang = lang::argv_lang(std::env::args());
    let saved = token_perf_core::store::Store::open_default()
        .ok()
        .and_then(|s| s.setting("lang").ok().flatten());
    rust_i18n::set_locale(lang::resolve_lang(
        argv_lang.as_deref(),
        saved.as_deref(),
        lang::process_lang().as_deref(),
    ));

    let cli = Cli::parse();
    let (flag, unknown) = lang::split_lang_flag(cli.lang.as_deref());

    let json = cli.json;
    let show_cost = cli.cost;
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
    #[cfg(feature = "serve")]
    let needs_sessions = !matches!(cli.command, Command::Serve { .. } | Command::Config { .. });
    #[cfg(not(feature = "serve"))]
    let needs_sessions = !matches!(cli.command, Command::Config { .. });
    if sessions.is_empty() && needs_sessions {
        eprintln!("{}", t!("no_sessions"));
        std::process::exit(1);
    }

    let pretty_flag = match &cli.command {
        Command::Config { pretty } => *pretty,
        _ => None,
    };
    let pretty = table::apply_pretty(&store, pretty_flag);

    match cli.command {
        Command::Sessions {
            top,
            all,
            sort,
            grep,
        } => {
            let mut reports: Vec<_> = sessions
                .iter()
                .filter(|s| all || s.parent.is_none())
                .map(|s| analyze::rollup(s, &sessions))
                .collect();
            if let Some(query) = &grep {
                reports.retain(|r| matches_grep([&r.title, &r.project, &r.session], query));
            }
            match &sort {
                Some(sort) => sort_reports(&mut reports, sort),
                None => reports
                    .sort_by_key(|r| Reverse(r.totals.cache_read + r.subagents.totals.cache_read)),
            }
            if json {
                return emit_json(&reports[..reports.len().min(top)]);
            }

            let rows: Vec<Vec<String>> = reports
                .iter()
                .take(top)
                .map(|r| {
                    let mut row = vec![
                        short(&r.session).to_string(),
                        r.started_at.get(..10).unwrap_or("").to_string(),
                        r.call_count.to_string(),
                        human(r.totals.cache_read),
                        // Blank, not 0: most sessions spawn nothing.
                        match r.subagents.count {
                            0 => String::new(),
                            _ => human(r.subagents.totals.cache_read),
                        },
                        human(r.totals.output),
                        basename(&r.project).to_string(),
                        format!("{}{}", if r.parent.is_some() { "↳ " } else { "" }, r.title),
                    ];
                    if show_cost {
                        let cost = pricing::sum([r.cost, r.subagents.cost]);
                        row.insert(6, dollars(cost));
                    }
                    row
                })
                .collect();
            let mut headers = vec![
                "ID", "DATE", "CALLS", "CACHE_RD", "SUB_RD", "OUTPUT", "PROJECT", "TITLE",
            ];
            let mut aligns = vec![
                Align::Left,
                Align::Left,
                Align::Right,
                Align::Right,
                Align::Right,
                Align::Right,
                Align::Left,
            ];
            if show_cost {
                headers.insert(6, "COST");
                aligns.insert(6, Align::Right);
            }
            table::print_table(pretty, "", &headers, &aligns, &rows, true);
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

            let r = analyze::rollup(target, &sessions);
            if json {
                return emit_json(&r);
            }
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
            if show_cost {
                println!("{}", t!("cost_line", cost = dollars(r.cost)));
            }
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

            if r.subagents.count > 0 {
                let sub = &r.subagents;
                let mut all = r.totals;
                all += sub.totals;
                println!("{}", t!("subagents", count = sub.count));
                println!(
                    "  {}",
                    t!(
                        "totals",
                        calls = sub.call_count,
                        output = human(sub.totals.output),
                        cache_read = human(sub.totals.cache_read),
                        cache_write = human(sub.totals.cache_write())
                    )
                );
                println!(
                    "  {}",
                    t!(
                        "subagents_total",
                        calls = r.call_count + sub.call_count,
                        output = human(all.output),
                        cache_read = human(all.cache_read),
                        cache_write = human(all.cache_write())
                    )
                );
                println!();
                let model_rows: Vec<Vec<String>> = sub
                    .by_model
                    .iter()
                    .map(|m| {
                        vec![
                            m.model.clone(),
                            m.calls.to_string(),
                            human(m.totals.cache_read),
                            human(m.totals.cache_write()),
                            human(m.totals.output),
                        ]
                    })
                    .collect();
                table::print_table(
                    pretty,
                    "  ",
                    &["MODEL", "CALLS", "CACHE_RD", "CACHE_WR", "OUTPUT"],
                    &[
                        Align::Left,
                        Align::Right,
                        Align::Right,
                        Align::Right,
                        Align::Right,
                    ],
                    &model_rows,
                    false,
                );
                println!();

                println!("{}", t!("subagents_top"));
                let subagent = t!("subagent");
                let child_rows: Vec<Vec<String>> = sub
                    .children
                    .iter()
                    .take(top)
                    .map(|c| {
                        let mut row = vec![
                            short(&c.session).to_string(),
                            c.call_count.to_string(),
                            human(c.totals.cache_read),
                            human(c.residual),
                        ];
                        if show_cost {
                            row.push(dollars(c.cost));
                        }
                        row.push(c.agent_type.clone().unwrap_or_else(|| subagent.to_string()));
                        row
                    })
                    .collect();
                let mut headers = vec!["ID", "CALLS", "CACHE_RD", "RESIDUAL", "AGENT"];
                let mut aligns = vec![Align::Left, Align::Right, Align::Right, Align::Right];
                if show_cost {
                    headers.insert(4, "COST");
                    aligns.push(Align::Right);
                }
                table::print_table(pretty, "  ", &headers, &aligns, &child_rows, true);
                println!();
            }

            println!("{}", t!("tool_residual"));
            let tool_rows: Vec<Vec<String>> = r
                .tools
                .iter()
                .take(top)
                .map(|t| {
                    let mut row = vec![
                        t.name.clone(),
                        t.calls.to_string(),
                        human(t.added),
                        human(t.residual),
                    ];
                    if show_cost {
                        row.push(dollars(t.residual_cost));
                    }
                    row
                })
                .collect();
            let mut headers = vec!["TOOL", "CALLS", "ADDED", "RESIDUAL"];
            let mut aligns = vec![Align::Left, Align::Right, Align::Right, Align::Right];
            if show_cost {
                headers.push("COST");
                aligns.push(Align::Right);
            }
            table::print_table(pretty, "  ", &headers, &aligns, &tool_rows, false);
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
            println!();

            println!("{}", t!("spike_calls"));
            let grew: Vec<u64> = r.calls.iter().map(|c| c.grew_by).collect();
            let spike_rows: Vec<Vec<String>> = top_spikes(&grew, top)
                .into_iter()
                .map(|i| &r.calls[i])
                .map(|c| {
                    vec![
                        c.index.to_string(),
                        human(c.grew_by),
                        human(c.context),
                        c.tools.join(", "),
                    ]
                })
                .collect();
            table::print_table(
                pretty,
                "  ",
                &["CALL", "GREW", "CONTEXT", "TOOLS"],
                &[Align::Left, Align::Right, Align::Right],
                &spike_rows,
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
            if json {
                return emit_json(&d);
            }
            println!(
                "{}",
                t!("summary_lead", sessions = d.sessions, calls = d.calls)
            );
            if show_cost {
                println!("{}", t!("summary_cost", cost = dollars(d.cost)));
            }
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
                        // Blank, not 0: most sessions spawn nothing.
                        match s.sub_calls {
                            0 => String::new(),
                            _ => human(s.sub_residual),
                        },
                        s.title.clone(),
                    ]
                })
                .collect();
            table::print_table(
                pretty,
                "  ",
                &["ID", "CALLS", "RESIDUAL", "SUB", "TITLE"],
                &[Align::Left, Align::Right, Align::Right, Align::Right],
                &session_rows,
                true,
            );
            println!();

            println!(
                "{}",
                t!("summary_solo", pct = format!("{:.0}", d.solo_tool_pct))
            );
        }

        #[cfg(feature = "serve")]
        Command::Serve { port, no_open } => serve(sessions, lang, port.unwrap_or(0), !no_open),

        Command::Config { .. } => {
            println!("lang    {lang}");
            println!("pretty  {}", if pretty { "on" } else { "off" });
        }
    }
}

#[cfg(feature = "serve")]
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

#[cfg(feature = "serve")]
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

/// A subagent id is `agent-` plus hex, so cutting at 8 leaves every one of them
/// reading `agent-a`. The prefix stays: the id has to paste back into `report`.
/// Unknown model somewhere in the sum → "-", never a number that looks complete.
fn dollars(cost: Option<f64>) -> String {
    match cost {
        Some(c) if c < 0.01 && c > 0.0 => "<$0.01".to_string(),
        Some(c) => format!("${c:.2}"),
        None => "-".to_string(),
    }
}

fn emit_json(value: &(impl serde::Serialize + ?Sized)) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("serializable")
    );
}

fn short(id: &str) -> &str {
    let cut = if id.starts_with("agent-") { 14 } else { 8 };
    id.get(..cut).unwrap_or(id)
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
