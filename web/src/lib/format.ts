// Display formatting only; the numbers themselves come from the server.
export const human = (n: number) =>
  n >= 1e9 ? (n / 1e9).toFixed(1) + 'B'
  : n >= 1e6 ? (n / 1e6).toFixed(1) + 'M'
  : n >= 1e3 ? (n / 1e3).toFixed(1) + 'K'
  : String(n);

/// `null` means the model has no known price: show a dash, never a made-up 0.
export const dollars = (c: number | null) =>
  c === null ? '—' : c > 0 && c < 0.01 ? '<$0.01' : `$${c.toFixed(2)}`;

export const pct = (n: number) => `${n.toFixed(0)}%`;
