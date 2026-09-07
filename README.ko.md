# token-perf

[English](README.md)

코딩 에이전트가 **어디서 토큰을 낭비하는지** 로컬 로그만으로 찾아내는 도구.
집계가 아니라 귀속이 목적이다 — "이번 달 얼마 썼나"가 아니라 "저 툴 하나가 세션 전체에 얼마를 물렸나".

## 왜 이게 필요한가

긴 세션의 비용은 출력이 아니라 **캐시 재읽기**다. 필자 로그의 세션 하나:

```
호출 793회 · 출력 786.7K · 캐시 재읽기 243.3M
```

한 번 컨텍스트에 들어간 토큰은 남은 모든 호출에서 다시 청구된다.
그래서 6번째 호출에서 13K짜리 툴 결과 하나가, 세션이 끝날 때까지 8M을 물린다.
token-perf 는 그 **잔류 비용(residual)** 을 툴별로 귀속해 순위를 매긴다.

측정은 usage 차분으로 한다:

```
context(n) = input + cache_write + cache_read
grew(n)    = context(n) - context(n-1) - output(n-1)   # 그 사이 컨텍스트에 들어온 토큰
residual(n) = grew(n) × (다음 컴팩션 전까지 남은 호출 수)  # 컨텍스트가 접히면 거기서 끝난다
```

문자 수 추정과 달리 이미지·바이너리가 섞여도 정확하다. 서버가 실제로 센 값이기 때문이다.

## 저장소

읽은 세션은 `$XDG_DATA_HOME/token-perf/token-perf.db`(기본 `~/.local/share/…`)의 SQLite 에
쌓인다. Claude Code 는 트랜스크립트를 기본 30일 뒤 지우지만(`cleanupPeriodDays`), 여기 남은
세션은 원본이 사라져도 그대로 분석된다. 실행할 때마다 변경된 파일의 자란 부분만 다시 읽는다
(필자 환경에서 649파일 첫 스캔 ~2초, 이후 ~0.03초). usage 블록의 원본 JSON 도 함께 저장하므로,
나중에 필요해진 필드는 재스캔 없이 `json_extract` 로 꺼낼 수 있다.

## 설치

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/M1nu0x0/token-perf/releases/latest/download/token-perf-installer.sh | sh
```

윈도우는 PowerShell 에서:

```powershell
irm https://github.com/M1nu0x0/token-perf/releases/latest/download/token-perf-installer.ps1 | iex
```

두 스크립트 모두 별도의 툴체인이 필요 없다. 설치 위치는 `$CARGO_HOME/bin`(기본 `~/.cargo/bin`)이고
셸 rc 에 그 디렉터리를 `PATH` 로 추가하므로, 새 셸을 열거나 rc 를 source 해야 `token-perf` 가 잡힌다.

직접 내려받으려면 [최신 릴리스](https://github.com/M1nu0x0/token-perf/releases/latest)에서
`token-perf-aarch64-apple-darwin.tar.xz`, `-x86_64-apple-darwin.tar.xz`,
`-x86_64-unknown-linux-gnu.tar.xz`, `-aarch64-unknown-linux-gnu.tar.xz`,
`token-perf-x86_64-pc-windows-msvc.zip` 중 플랫폼에 맞는 것을 고른다. 풀면 `token-perf` 바이너리가
든 디렉터리가 나오고, 각 아카이브 옆에는 `.sha256` 이 함께 올라간다:

```sh
shasum -a 256 -c token-perf-aarch64-apple-darwin.tar.xz.sha256
```

Homebrew: 준비 중.

두 스크립트로 설치했다면 스스로 업데이트할 수 있다:

```sh
token-perf upgrade          # --check 는 새 버전이 있는지 확인만 한다
```

그 밖의 설치본(직접 푼 아카이브, 패키지 매니저, `cargo install`)은 install receipt 가 없어서
`upgrade` 가 그 사실을 알리고 아무것도 바꾸지 않는다.

소스에서 빌드하는 방법은 [빌드](#빌드)에 있다.

## 쓰는 법

```sh
token-perf sessions          # 세션 목록, 캐시 재읽기 순
token-perf report [세션ID]   # 툴별 잔류 비용 (생략 시 최근 세션)
token-perf serve             # 웹 UI (OS 가 고른 빈 포트, 주소는 실행하면 출력된다)
token-perf serve --port 5177 # 포트 고정 (--no-open 이면 브라우저를 안 띄운다)
token-perf config --pretty on # 이후로 표에 테두리를 그린다. `config` 만 치면 현재 설정을 보여준다
token-perf report --json      # sessions/report/summary 를 JSON 으로. 스크립트에서 쓸 때
token-perf sessions --cost    # Anthropic 정가로 환산한 달러 열 추가 (report, summary 도 됨)
```

UI 를 고칠 때는 `token-perf serve --port 5177` 를 띄워 둔 채 `cd web && npm run dev` —
vite 개발 서버가 `/api` 를 5177 로 프록시한다.

서브에이전트는 부모의 툴 호출 하나이므로 그 토큰도 부모의 청구서에 들어간다. `sessions` 는
`SUB_RD` 열로 따로 보여주면서 둘을 합쳐 정렬하고, `summary` 는 부모 행에 얹으며, 부모를 대상으로
한 `report` 에는 모델별 분리와 가장 많이 다시 읽은 자식을 담은 `서브에이전트` 절이 붙는다.

### 언어

en · ko 를 지원한다. CLI 출력과 웹 UI 가 같은 언어를 쓴다.

```sh
token-perf --lang ko sessions   # 이번 실행부터 한국어, 설정에 저장돼 다음 실행에도 유지된다
```

결정 순서는 `--lang` → 저장된 설정 → `LC_ALL`/`LANG` → `en`. 지원하지 않는 코드를 주면
그 플래그는 무시하고(저장도 하지 않는다) 경고만 한 줄 내보낸다 — 오타 하나로 저장해 둔
언어를 잃지 않는다. `--help` 는 영어로 남는다.

## 빌드

직접 빌드하려면:

```sh
(cd web && npm ci && npm run build)   # UI 를 먼저 빌드해야 바이너리에 embed 된다
cargo build --release
```

UI 빌드에 Node 가 필요하다. 빠뜨리면 릴리스 빌드가 실패한다 — UI 없는 바이너리는
실행하면 404 만 내므로 조용히 나가지 못하게 막아 뒀다.

## 구조

```
crates/core/     소스 무관 모델·분석(common/) + 에이전트별 파서(sources/) + axum 라우터
crates/cli/      clap 기반 CLI
web/             Svelte UI. 빌드 산출물이 core 에 embed 된다
```

새 에이전트를 붙이려면 `sources/` 에 `Source` 를 구현하고 `sources::all()` 에 한 줄 추가하면 된다.
분석은 정규화된 모델 위에서 돌기 때문에 손댈 필요가 없다.

현재 지원: Claude Code (`~/.claude/projects/**/*.jsonl`).

## 트랜스크립트를 읽을 때의 함정

공식 문서는 이 파일 형식을 **내부 구현이며 버전마다 바뀔 수 있다**고 명시한다
(`code.claude.com/docs/en/claude-directory`). 필자 로그에서 확인한 함정들 — 수치는
모두 그 표본 기준이다:

- **한 응답이 여러 줄로 쪼개진다.** 같은 `message.id` 아래 누적 스냅샷과 추가 청크가
  섞여 들어온다. 첫 줄만 읽으면 툴의 72%와 출력 토큰의 절반을 놓치고, 전부 더하면
  토큰이 2배가 된다. **마지막 줄이 최종 usage 를 갖고**, 툴은 모든 줄에서 모으되
  `tool_use.id` 로 중복을 걸러야 한다.
- **빈 usage 스텁**이 먼저 들어온다. 호출로 세면 컨텍스트 계열이 0으로 내려앉는다.
- **캐시 쓰기는 TTL 별로 단가가 다르다.** `cache_creation.ephemeral_1h_input_tokens` 는
  입력 단가의 2배, 5분짜리는 1.25배. 평탄 필드만 읽으면 구분할 수 없다.
- **툴 출력은 세 번 잘린다.** 전체 → `toolUseResult.stdout`(30K) → `tool_result` 본문.
  컨텍스트에 들어간 건 마지막 것뿐이라 `persistedOutputSize` 를 쓰면 틀린다.
- **서브에이전트는 별도 파일**이다 (`<session>/subagents/agent-*.jsonl`). 옆의
  `.meta.json` 이 부모의 어느 툴 호출에서 나왔는지 알려준다.
- **기본 30일 후 삭제된다** (`cleanupPeriodDays`).

## 라이선스

MIT
