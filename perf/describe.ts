// T11 describe (PAID): replay captured estimator prompts through the real `claude -p`.
//   bun perf/describe.ts --captures <dir from perf/shim> --variant base|lean
//     [--n 8] [--concurrency 1] [--out results.json]
// base = the exact args worklog sent; lean = same args + flags that drop tools, MCP,
// user/project settings (hooks, plugins, CLAUDE.md), skills and session persistence;
// fast = lean + MAX_THINKING_TOKENS=0 (no extended thinking).
import { readFileSync, readdirSync, writeFileSync, mkdtempSync } from "fs";
import { tmpdir } from "os";

const arg = (k: string, d?: string) => {
  const i = process.argv.indexOf(`--${k}`);
  return i > 0 ? process.argv[i + 1] : d;
};
const dir = arg("captures")!;
const variant = arg("variant", "base")!;
const n = Number(arg("n", "8"));
const conc = Number(arg("concurrency", "1"));
const LEAN = ["--tools", "", "--strict-mcp-config", "--setting-sources", "", "--disable-slash-commands", "--no-session-persistence"];
const cwd = mkdtempSync(`${tmpdir()}/wl-describe-`);

const ids = readdirSync(dir).filter((f) => f.endsWith(".stdin")).map((f) => f.slice(0, -6)).sort().slice(0, n);

async function run(id: string) {
  const args: string[] = JSON.parse(readFileSync(`${dir}/${id}.args.json`, "utf8"));
  const t = performance.now();
  const p = Bun.spawn(["claude", ...args, ...(variant === "base" ? [] : LEAN)], {
    cwd,
    stdin: Bun.file(`${dir}/${id}.stdin`),
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, WORKLOG_HOOK_SUPPRESS: "1", ...(variant === "fast" ? { MAX_THINKING_TOKENS: "0" } : {}) },
  });
  const out = await new Response(p.stdout).text();
  await p.exited;
  const ms = performance.now() - t;
  let j: any = {};
  try { j = JSON.parse(out); } catch { j = { is_error: true, raw: out.slice(0, 300) }; }
  return { id, ms: Math.round(ms), cost: j.total_cost_usd, turns: j.num_turns, is_error: j.is_error ?? true, output: j.structured_output ?? null };
}

const results: any[] = new Array(ids.length);
let next = 0;
const t0 = performance.now();
await Promise.all(Array.from({ length: conc }, async () => {
  while (next < ids.length) { const i = next++; results[i] = await run(ids[i]); }
}));
const wall = performance.now() - t0;
const ok = results.filter((r) => !r.is_error && r.output);
const med = (xs: number[]) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
console.log(JSON.stringify({
  variant, n: ids.length, concurrency: conc, wall_s: +(wall / 1000).toFixed(1),
  median_call_s: +(med(results.map((r) => r.ms)) / 1000).toFixed(2),
  cost_usd: +results.reduce((s, r) => s + (r.cost ?? 0), 0).toFixed(4), ok: ok.length,
}));
const out = arg("out");
if (out) writeFileSync(out, JSON.stringify(results, null, 1));
