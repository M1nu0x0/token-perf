use std::cmp::Reverse;

use clap::{Parser, Subcommand};
use token_perf_core::{common::analyze, sync_and_load};

#[derive(Parser)]
#[command(
    name = "token-perf",
    version,
    about = "로컬 에이전트 로그에서 토큰이 어디로 새는지 찾는다"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 세션 목록을 캐시 재청구량이 큰 순으로 보여준다
    Sessions {
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// 서브에이전트 세션까지 포함한다 (기본은 최상위 세션만)
        #[arg(long)]
        all: bool,
    },
    /// 한 세션의 낭비를 툴별로 귀속해 보여준다 (생략하면 가장 최근 세션)
    Report {
        session: Option<String>,
        #[arg(long, default_value_t = 15)]
        top: usize,
    },
    /// 웹 UI를 띄우고 브라우저로 연다
    Serve {
        /// 붙일 포트. 생략하면 OS 가 비어 있는 포트를 골라 준다
        #[arg(long)]
        port: Option<u16>,
        /// 브라우저를 자동으로 열지 않는다
        #[arg(long)]
        no_open: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let (sessions, scanned) = match sync_and_load() {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("저장소를 열지 못했습니다: {e}");
            std::process::exit(1);
        }
    };
    if scanned.files_read > 0 {
        eprintln!(
            "트랜스크립트 {}개 중 {}개에서 호출 {}건을 새로 읽었습니다.",
            scanned.files_seen, scanned.files_read, scanned.calls_written
        );
    }
    if scanned.files_skipped > 0 {
        eprintln!("{}개는 읽지 못해 건너뛰었습니다.", scanned.files_skipped);
    }
    // serve must start with no sessions: CI has no ~/.claude at all.
    if sessions.is_empty() && !matches!(cli.command, Command::Serve { .. }) {
        eprintln!("세션이 없습니다. ~/.claude/projects 를 확인하세요.");
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
                        eprintln!("'{id}' 로 시작하는 세션이 여럿입니다:");
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
                eprintln!("해당 세션을 찾지 못했습니다.");
                std::process::exit(1);
            };

            let r = analyze::session(target);
            println!("session  {}  ({})", r.session, r.source);
            if !r.title.is_empty() {
                println!("title    {}", r.title);
            }
            if let Some(parent) = &r.parent {
                let kind = r.agent_type.as_deref().unwrap_or("서브에이전트");
                let depth = r
                    .spawn_depth
                    .map_or(String::new(), |d| format!(" · 깊이 {d}"));
                println!("parent   {parent}  ({kind}{depth})");
                if let Some(tool_use) = &r.parent_tool_use_id {
                    println!("spawned  {tool_use}");
                }
            }
            println!("project  {}", r.project);
            println!("started  {}", r.started_at);
            println!();
            println!(
                "호출 {}회 · 출력 {} · 캐시 재읽기 {} · 캐시 쓰기 {}",
                r.call_count,
                human(r.totals.output),
                human(r.totals.cache_read),
                human(r.totals.cache_write())
            );
            println!(
                "기저 컨텍스트 {} (시스템 프롬프트·툴 정의·규칙 파일) → 세션 전체에서 {} 청구",
                human(r.baseline),
                human(r.baseline_billed)
            );
            if r.totals.cache_write_1h > 0 {
                println!(
                    "캐시 쓰기 내역 — 5분 {} · 1시간 {} (1시간 캐시는 입력 단가의 2배)",
                    human(r.totals.cache_write_5m),
                    human(r.totals.cache_write_1h)
                );
            }
            if r.failed_calls > 0 {
                let kinds = match r.error_kinds.is_empty() {
                    true => String::new(),
                    false => format!(" ({})", r.error_kinds.join(", ")),
                };
                println!("실패·중단된 호출 {}회{kinds}", r.failed_calls);
            }
            println!();

            println!("툴별 잔류 비용 (컨텍스트에 남아 재청구된 토큰)");
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

            println!("컨텍스트를 가장 크게 불린 호출");
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

        Command::Serve { port, no_open } => serve(sessions, port.unwrap_or(0), !no_open),
    }
}

fn serve(sessions: Vec<token_perf_core::Session>, port: u16, open: bool) {
    let count = sessions.len();
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

    // The callback fires after the listener is open, so the browser is never refused.
    let result = runtime.block_on(token_perf_core::server::serve(sessions, port, |bound| {
        let url = format!("http://localhost:{bound}");
        println!("token-perf: {url}  (세션 {count}개)");
        println!("멈추려면 Ctrl-C");
        if open {
            open_browser(&url);
        }
    }));

    if let Err(e) = result {
        eprintln!("서버 시작 실패: {e}");
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
