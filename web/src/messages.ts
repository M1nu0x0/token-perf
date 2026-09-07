// The server decides the language (/api/config); this file only holds the words.
export type Bucket = { min: number; max: number | null };

const en = {
  tabTldr: 'TL;DR',
  tabSessions: 'Sessions',
  lead: (sessions: number, calls: string) => `Looked over ${sessions} sessions and ${calls} calls.`,
  cacheRead: (pct: string, output: string, read: string) =>
    `<b>${pct}%</b> of your tokens went to re-reading what had already been read. ` +
    `You only said ${output} of new things, but re-read ${read}.`,
  lengthHeading: 'The longer the conversation, the more often you buy the same thing',
  lengthIntro: 'Whatever comes in once is carried along on every call until the conversation ends.',
  lengthCompare: (short: string, long: string) =>
    ` Short conversations buy it back ${short} times, long ones <b>${long} times</b>.`,
  times: (n: string) => `${n}x`,
  bucket: ({ min, max }: Bucket) =>
    max === null ? `${min}+ calls` : min === 0 ? `${max} calls or fewer` : `${min}–${max} calls`,
  culpritHeading: 'What made it big',
  culpritTail:
    ' — that is their share of everything tools pushed into the conversation. ' +
    'Call the big-output tools less, or trim their output, and it drops right away.',
  topHeading: 'The three most expensive conversations',
  soloHeading: 'One at a time',
  soloLine: (pct: string) =>
    `<b>${pct}%</b> of the messages that used a tool called exactly one. ` +
    'Calling several at once cuts the round trips, and fewer round trips means less re-reading.',
  sessionsHeading: 'Sessions',
  byCacheRead: 'by cache re-read',
  includeSubagents: 'include subagents',
  subagent: 'subagent',
  depth: (n: number) => ` · depth ${n}`,
  totals: (calls: number, output: string, read: string) =>
    `${calls} calls · ${output} output · cache re-read <b>${read}</b>`,
  subagents: (count: number, calls: number, output: string, read: string) =>
    `Subagents (${count}) — ${calls} calls · ${output} output · cache re-read <b>${read}</b>`,
  withSubagents: (calls: number, read: string) =>
    `With subagents — ${calls} calls · cache re-read <b>${read}</b>`,
  baseline: (baseline: string, billed: string) =>
    `Baseline context ${baseline} → billed ${billed} across the session`,
  cacheWrites: (write5m: string, write1h: string) =>
    `Cache writes — 5 min ${write5m} · 1 hour ${write1h} (1 hour costs twice the input rate)`,
  failedCalls: (n: number) => `${n} failed or interrupted calls`,
  toolResidual: 'Residual cost per tool',
  cardCacheRead: 'Cache re-read',
  cardAmplification: 'Long conversations',
  cardCost: 'At list price',
  costNote: 'Anthropic list rates; — when a model has no known price',
  usageHeading: 'Where the tokens went',
  labelInput: 'input',
  labelCacheWrite: 'cache write',
  labelCacheRead: 'cache re-read',
  labelOutput: 'output',
  searchPlaceholder: 'filter by title, project or id',
  noMatch: 'No sessions match.',
  sessionCount: (n: number) => `${n} sessions`,
  loading: 'Reading…',
  retry: 'Retry',
  noSessions: 'No sessions yet — run `token-perf sync` first.',
  notFound: 'No such session.',
  backToSessions: 'All sessions',
  parentSession: 'parent session',
  cardCalls: 'Calls',
  cardSubCost: 'Subagents',
  growthHeading: 'How the context grew',
  growthSummary: (calls: number, from: string, to: string) =>
    `Context over ${calls} calls: ${from} at the start, ${to} at the end.`,
  spikeNote: 'Dots mark the calls in the top 10% of growth; hover one for its tools.',
  subagentsHeading: 'Subagents',
  errorKinds: 'Errors',
};

const ko: typeof en = {
  tabTldr: '세 줄 요약',
  tabSessions: '세션',
  lead: (sessions, calls) => `세션 ${sessions}개, 호출 ${calls}회를 훑어봤어요.`,
  cacheRead: (pct, output, read) =>
    `당신 토큰의 <b>${pct}%</b>는 이미 읽은 걸 다시 읽는 데 쓰였어요. ` +
    `새로 말한 건 ${output}밖에 안 되는데, 다시 읽은 건 ${read}이에요.`,
  lengthHeading: '대화가 길수록 같은 걸 여러 번 삽니다',
  lengthIntro: '한 번 들어온 내용은 대화가 끝날 때까지 매번 다시 실려 갑니다.',
  lengthCompare: (short, long) =>
    ` 짧은 대화는 ${short}번, 긴 대화는 <b>${long}번</b> 다시 사는 셈이에요.`,
  times: (n) => `${n}번`,
  bucket: ({ min, max }) =>
    max === null ? `${min}회 이상` : min === 0 ? `${max}회 이하` : `${min}~${max}회`,
  culpritHeading: '덩치를 키운 범인',
  culpritTail:
    ' — 툴이 대화에 밀어 넣은 양 중 이만큼을 차지해요. 결과가 큰 툴을 덜 부르거나 잘라 쓰면 바로 줄어듭니다.',
  topHeading: '제일 비쌌던 대화 셋',
  soloHeading: '한 번에 하나씩',
  soloLine: (pct) =>
    `툴을 쓴 메시지의 <b>${pct}%</b>가 툴을 딱 하나만 불렀어요. ` +
    '한 번에 여러 개를 같이 부르면 그만큼 왕복이 줄고, 왕복이 줄면 다시 읽는 양도 줄어요.',
  sessionsHeading: '세션',
  byCacheRead: '캐시 재읽기 순',
  includeSubagents: '서브에이전트 포함',
  subagent: '서브에이전트',
  depth: (n) => ` · 깊이 ${n}`,
  totals: (calls, output, read) =>
    `호출 ${calls}회 · 출력 ${output} · 캐시 재읽기 <b>${read}</b>`,
  subagents: (count, calls, output, read) =>
    `서브에이전트 ${count}개 — 호출 ${calls}회 · 출력 ${output} · 캐시 재읽기 <b>${read}</b>`,
  withSubagents: (calls, read) =>
    `서브에이전트 포함 — 호출 ${calls}회 · 캐시 재읽기 <b>${read}</b>`,
  baseline: (baseline, billed) => `기저 컨텍스트 ${baseline} → 세션 전체에서 ${billed} 청구`,
  cacheWrites: (write5m, write1h) =>
    `캐시 쓰기 — 5분 ${write5m} · 1시간 ${write1h} (1시간은 입력 단가의 2배)`,
  failedCalls: (n) => `실패·중단된 호출 ${n}회`,
  toolResidual: '툴별 잔류 비용',
  cardCacheRead: '캐시 재읽기',
  cardAmplification: '긴 대화',
  cardCost: '정가 환산',
  costNote: '앤트로픽 정가 기준 · 가격을 모르는 모델이 섞이면 —',
  usageHeading: '토큰이 어디로 갔나',
  labelInput: '입력',
  labelCacheWrite: '캐시 쓰기',
  labelCacheRead: '캐시 재읽기',
  labelOutput: '출력',
  searchPlaceholder: '제목·프로젝트·ID 검색',
  noMatch: '조건에 맞는 세션이 없어요.',
  sessionCount: (n) => `세션 ${n}개`,
  loading: '읽는 중…',
  retry: '다시 시도',
  noSessions: '아직 세션이 없어요 — `token-perf sync`를 먼저 돌려보세요.',
  notFound: '그런 세션이 없어요.',
  backToSessions: '세션 목록',
  parentSession: '상위 세션',
  cardCalls: '호출',
  cardSubCost: '서브에이전트',
  growthHeading: '컨텍스트가 자란 모양',
  growthSummary: (calls, from, to) =>
    `호출 ${calls}회 동안의 컨텍스트: 처음 ${from}, 마지막 ${to}.`,
  spikeNote: '점은 증가량 상위 10%인 호출이에요. 점에 마우스를 올리면 그때 쓴 툴이 보여요.',
  subagentsHeading: '서브에이전트',
  errorKinds: '오류',
};

const all: Record<string, typeof en> = { en, ko };

export const messages = (lang: string) => (Object.hasOwn(all, lang) ? all[lang] : en);
