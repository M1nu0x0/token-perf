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
  baseline: (baseline: string, billed: string) =>
    `Baseline context ${baseline} → billed ${billed} across the session`,
  cacheWrites: (write5m: string, write1h: string) =>
    `Cache writes — 5 min ${write5m} · 1 hour ${write1h} (1 hour costs twice the input rate)`,
  failedCalls: (n: number) => `${n} failed or interrupted calls`,
  toolResidual: 'Residual cost per tool',
  pickSession: 'Pick a session.',
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
  baseline: (baseline, billed) => `기저 컨텍스트 ${baseline} → 세션 전체에서 ${billed} 청구`,
  cacheWrites: (write5m, write1h) =>
    `캐시 쓰기 — 5분 ${write5m} · 1시간 ${write1h} (1시간은 입력 단가의 2배)`,
  failedCalls: (n) => `실패·중단된 호출 ${n}회`,
  toolResidual: '툴별 잔류 비용',
  pickSession: '세션을 고르세요.',
};

const ja: typeof en = {
  tabTldr: '3行まとめ',
  tabSessions: 'セッション',
  lead: (sessions, calls) => `セッション ${sessions} 件、呼び出し ${calls} 回を見てみました。`,
  cacheRead: (pct, output, read) =>
    `トークンの <b>${pct}%</b> が、すでに読んだ内容をもう一度読むために使われています。` +
    `新しく話したのは ${output} だけなのに、読み直したのは ${read} です。`,
  lengthHeading: '会話が長いほど、同じものを何度も買うことになります',
  lengthIntro: '一度コンテキストに入った内容は、会話が終わるまで呼び出しのたびに運ばれ続けます。',
  lengthCompare: (short, long) =>
    ` 短い会話では ${short} 回、長い会話では <b>${long} 回</b>も買い直している計算です。`,
  times: (n) => `${n} 回`,
  bucket: ({ min, max }) =>
    max === null ? `${min} 回以上` : min === 0 ? `${max} 回以下` : `${min} 〜 ${max} 回`,
  culpritHeading: '会話を膨らませた犯人',
  culpritTail:
    ' — ツールが会話に押し込んだ量のうち、この割合を占めています。出力の大きいツールを呼ぶ回数を減らすか、出力そのものを切り詰めれば、すぐに減ります。',
  topHeading: '最も高くついた会話 3 件',
  soloHeading: '一度にひとつずつ',
  soloLine: (pct) =>
    `ツールを使ったメッセージの <b>${pct}%</b> が、1つしか呼んでいません。` +
    'まとめて呼べばその分だけ往復が減り、往復が減れば読み直す量も減ります。',
  sessionsHeading: 'セッション',
  byCacheRead: 'キャッシュ再読み取り順',
  includeSubagents: 'サブエージェントを含む',
  subagent: 'サブエージェント',
  depth: (n) => ` · 深さ ${n}`,
  totals: (calls, output, read) =>
    `呼び出し ${calls} 回 · 出力 ${output} · キャッシュ再読み取り <b>${read}</b>`,
  baseline: (baseline, billed) => `ベースラインコンテキスト ${baseline} → セッション全体で ${billed} が請求されています`,
  cacheWrites: (write5m, write1h) =>
    `キャッシュ書き込み — 5分 ${write5m} · 1時間 ${write1h} (1時間は入力単価の2倍)`,
  failedCalls: (n) => `失敗または中断した呼び出し ${n} 回`,
  toolResidual: 'ツール別の持ち越しコスト',
  pickSession: 'セッションを選んでください。',
};

const all: Record<string, typeof en> = { en, ko, ja };

export const messages = (lang: string) => all[lang] ?? en;
