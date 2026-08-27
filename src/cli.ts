#!/usr/bin/env node
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, resolve, sep } from 'node:path';
import { parseArgs } from 'node:util';

const WEB_ROOT = resolve(import.meta.dirname, 'web');
const MIME: Record<string, string> = {
  '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css',
  '.json': 'application/json', '.svg': 'image/svg+xml', '.ico': 'image/x-icon'
};

const HELP = `token-perf

Usage:
  token-perf serve [--port <n>]   serve the web UI (default port 5177)
  token-perf help                 show this help
`;

function serve(port: number) {
  createServer(async (req, res) => {
    const url = new URL(req.url ?? '/', 'http://localhost');
    const file = resolve(join(WEB_ROOT, decodeURIComponent(url.pathname)));
    // reject traversal outside the build output
    const target = file === WEB_ROOT || file.startsWith(WEB_ROOT + sep) ? file : WEB_ROOT;
    for (const candidate of [target, join(target, 'index.html'), join(WEB_ROOT, 'index.html')]) {
      try {
        const body = await readFile(candidate);
        res.writeHead(200, { 'content-type': MIME[extname(candidate)] ?? 'application/octet-stream' });
        res.end(body);
        return;
      } catch { /* try next */ }
    }
    res.writeHead(404).end('not found');
  }).listen(port, () => console.log(`token-perf: http://localhost:${port}`));
}

const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: { port: { type: 'string' }, help: { type: 'boolean', short: 'h' } }
});

if (positionals[0] === 'serve' && !values.help) {
  const port = Number(values.port ?? 5177);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    console.error(`invalid --port: ${values.port}`);
    process.exit(1);
  }
  serve(port);
} else {
  console.log(HELP);
}
