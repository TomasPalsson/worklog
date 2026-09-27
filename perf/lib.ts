// Pure helpers for perf/bench.ts: hermetic env/fixture setup, timing, CLI
// execution, DB dump oracle, masking, daemon HTTP helpers, and reporting.
// See perf/README.md.

import { Database } from "bun:sqlite";
import { mkdirSync, writeFileSync } from "node:fs";

export const PERF_DIR = process.env.PERF_DIR;
if (!PERF_DIR) {
  console.error("PERF_DIR env var is required (fixture root from perf/fixture.sh)");
  process.exit(1);
}
export const RUNS_ROOT = `${PERF_DIR}/runs`;
mkdirSync(RUNS_ROOT, { recursive: true });

// ---- hermetic env + fixture clone ----
export function hermeticEnv(dataDir: string): Record<string, string> {
  const env: Record<string, string> = {
    WORKLOG_HOME: dataDir,
    HOME: `${PERF_DIR}/home`,
    WORKLOG_SECRETS_FILE: `${dataDir}/secrets.json`,
    WORKLOG_ESTIMATOR_PROVIDER: "litellm",
    WORKLOG_PRUNE_ENABLED: "false",
    PATH: "/usr/bin:/bin",
  };
  if (process.env.TZ) env.TZ = process.env.TZ;
  return env;
}

let seq = 0;
export function mkRundir(tag: string): string {
  return `${RUNS_ROOT}/${tag}-${process.pid}-${seq++}-${Math.random().toString(36).slice(2, 8)}`;
}

export async function cloneFixture(src: "data" | "empty", rundir: string): Promise<void> {
  mkdirSync(rundir, { recursive: true });
  const proc = Bun.spawn(["cp", "-c", "-R", `${PERF_DIR}/${src}`, `${rundir}/data`], {
    stdout: "ignore", stderr: "pipe",
  });
  const code = await proc.exited;
  if (code !== 0) {
    const err = await new Response(proc.stderr).text();
    throw new Error(`cp -c -R ${src} -> ${rundir}/data failed (${code}): ${err}`);
  }
}

// ---- timing ----
export async function timedIteration<T>(
  fn: () => Promise<T>,
): Promise<{ result: T; ms: number; wallStart: number; wallEnd: number }> {
  const wallStart = Date.now();
  const t0 = performance.now();
  const result = await fn();
  const t1 = performance.now();
  return { result, ms: t1 - t0, wallStart, wallEnd: Date.now() };
}

export function stats(arr: number[]): { min: number; median: number; p95: number; runs: number } {
  const s = [...arr].sort((a, b) => a - b);
  const median = s.length % 2 === 1 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
  const p95 = s[Math.min(s.length - 1, Math.ceil(0.95 * s.length) - 1)];
  return { min: s[0], median, p95, runs: arr.length };
}

// ---- CLI process execution (hard timeout kills a hung process rather than hanging the bench) ----
const CLI_TIMEOUT_MS = 60_000;

export async function execCli(
  bin: string, args: string[], env: Record<string, string>, stdinText?: string,
): Promise<{ stdout: string; code: number }> {
  const proc = Bun.spawn([bin, ...args], {
    env, stdout: "pipe", stderr: "ignore", stdin: stdinText !== undefined ? "pipe" : "ignore",
  });
  if (stdinText !== undefined) {
    (proc.stdin as any).write(stdinText);
    (proc.stdin as any).end();
  }
  const timer = setTimeout(() => { try { proc.kill(9); } catch {} }, CLI_TIMEOUT_MS);
  const [stdout, code] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
  clearTimeout(timer);
  return { stdout, code };
}

// ---- DB dump oracle ----
function quoteIdent(name: string): string {
  return `"${name.replace(/"/g, '""')}"`;
}

// immutable=1: a plain readonly open of a WAL-mode db that a writer already
// closed (no -wal/-shm left) throws SQLITE_CANTOPEN (reproduces w/ stock sqlite3 CLI too).
export function readonlyDb(path: string): Database {
  return new Database(`file:${path}?mode=ro&immutable=1`, { readonly: true });
}

export function dumpDb(path: string): string {
  const db = readonlyDb(path);
  try {
    const schema = db.query("SELECT type,name,sql FROM sqlite_master ORDER BY type,name").all();
    const lines: string[] = schema.map((row) => `SCHEMA\t${JSON.stringify(row)}`);
    const tables = db.query(
      "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    ).all() as { name: string }[];
    for (const t of tables) {
      const cols = (db.query(`PRAGMA table_info(${quoteIdent(t.name)})`).all() as { name: string }[])
        .map((c) => c.name);
      const rows = db.query(
        `SELECT * FROM ${quoteIdent(t.name)} ORDER BY ${cols.map(quoteIdent).join(",")}`,
      ).all();
      for (const r of rows) lines.push(`${t.name}\t${JSON.stringify(r)}`);
    }
    return lines.join("\n");
  } finally {
    db.close();
  }
}

// ---- masking: only values generated during [wallStart-2s, wallEnd+2s] are replaced ----
const ISO_RE = /\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?/g;
const INT_RE = /\b(\d{13}|\d{10})\b/g;

export function maskText(text: string, wallStart: number, wallEnd: number, rundir: string, port?: number): string {
  let out = rundir ? text.split(rundir).join("<RUN>") : text;
  if (port !== undefined) out = out.split(String(port)).join("<PORT>");
  const lo = wallStart - 2000, hi = wallEnd + 2000;
  out = out.replace(ISO_RE, (m) => {
    const t = Date.parse(m);
    return !Number.isNaN(t) && t >= lo && t <= hi ? "<NOW>" : m;
  });
  return out.replace(INT_RE, (m) => {
    const asMs = m.length === 13 ? Number(m) : Number(m) * 1000;
    return asMs >= lo && asMs <= hi ? "<NOW>" : m;
  });
}

function sha256(text: string): string {
  return new Bun.CryptoHasher("sha256").update(text).digest("hex");
}

// Multiset difference: an inserted line (e.g. a new index in SCHEMA) shows as one line,
// not as every later line shifting.
export function printDiff(id: string, baseText: string, candText: string): void {
  const count = new Map<string, number>();
  for (const l of baseText.split("\n")) count.set(l, (count.get(l) ?? 0) + 1);
  const onlyCand: string[] = [];
  for (const l of candText.split("\n")) {
    const n = count.get(l) ?? 0;
    if (n > 0) count.set(l, n - 1); else onlyCand.push(l);
  }
  const onlyBase = [...count].flatMap(([l, n]) => Array(n).fill(l) as string[]);
  console.log(`--- oracle mismatch for ${id}: ${onlyBase.length} line(s) only in base, ${onlyCand.length} only in cand (first 20 each) ---`);
  for (const l of onlyBase.slice(0, 20)) console.log(`- ${l}`);
  for (const l of onlyCand.slice(0, 20)) console.log(`+ ${l}`);
}

// PERF_IGNORE=<regex>: drop intended-difference lines (e.g. a new index) from both oracles
// before comparing; such a match reports PASS* so it is never silent.
const IGNORE = process.env.PERF_IGNORE ? new RegExp(process.env.PERF_IGNORE) : undefined;
function withoutIgnored(text: string): string {
  return IGNORE ? text.split("\n").filter((l) => !IGNORE.test(l)).join("\n") : text;
}

// ---- daemon HTTP helpers ----
export async function daemonFetch(port: number, path: string): Promise<{ status: number; body: string }> {
  const res = await fetch(`http://127.0.0.1:${port}${path}`);
  return { status: res.status, body: await res.text() };
}

export async function waitHealthy(port: number, timeoutMs = 20_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`http://127.0.0.1:${port}/health`);
      if (res.status === 200) { await res.text(); return; }
    } catch {}
    await Bun.sleep(100);
  }
  throw new Error(`daemon on port ${port} did not become healthy within ${timeoutMs}ms`);
}

export function spawnDaemon(bin: string, rundir: string, port: number) {
  return Bun.spawn([bin, "daemon", "--socket", `${rundir}/api.sock`, "--tcp", `127.0.0.1:${port}`], {
    env: hermeticEnv(`${rundir}/data`), stdout: "ignore", stderr: "ignore",
  });
}

export async function stopDaemon(proc: { kill(sig?: number): void; exited: Promise<number> }): Promise<void> {
  try { proc.kill(); } catch {}
  const exitedInTime = await Promise.race([proc.exited.then(() => true), Bun.sleep(2000).then(() => false)]);
  if (!exitedInTime) {
    try { proc.kill(9); } catch {}
    await proc.exited;
  }
}

// ---- reporting ----
type ResultRow = {
  id: string; baseMedian?: number; candMedian: number; candP95: number; oracleLabel: string;
};
export const jsonRows: Record<string, unknown>[] = [];
const tableRows: ResultRow[] = [];

export function reportScenario(
  id: string, candMs: number[], baseMs: number[],
  candOracle: string | undefined, baseOracle: string | undefined,
  candExtra?: Record<string, unknown>, baseExtra?: Record<string, unknown>,
): void {
  const candStats = stats(candMs);
  const baseStats = baseMs.length ? stats(baseMs) : undefined;
  const candHash = candOracle !== undefined ? sha256(candOracle) : undefined;
  const baseHash = baseOracle !== undefined ? sha256(baseOracle) : undefined;

  let pass: boolean | undefined;
  let oracleLabel: string;
  if (candOracle !== undefined && baseOracle !== undefined) {
    pass = withoutIgnored(candOracle) === withoutIgnored(baseOracle);
    oracleLabel = !pass ? "FAIL" : candOracle === baseOracle ? "PASS" : "PASS*";
    if (!pass) {
      printDiff(id, baseOracle, candOracle);
      writeFileSync(`${RUNS_ROOT}/oracle-${id}-base.txt`, baseOracle);
      writeFileSync(`${RUNS_ROOT}/oracle-${id}-cand.txt`, candOracle);
      console.log(`full oracles saved: ${RUNS_ROOT}/oracle-${id}-{base,cand}.txt`);
    }
  } else {
    oracleLabel = candHash ? candHash.slice(0, 12) : "-";
    if (candHash) console.log(`${id} oracle sha256: ${candHash}`);
  }

  if (candExtra || baseExtra) {
    const parts = [baseExtra && `base ${JSON.stringify(baseExtra)}`, candExtra && `cand ${JSON.stringify(candExtra)}`]
      .filter(Boolean);
    console.log(`${id} extra: ${parts.join(" | ")}`);
  }

  tableRows.push({ id, baseMedian: baseStats?.median, candMedian: candStats.median, candP95: candStats.p95, oracleLabel });
  jsonRows.push({
    id, bin: "cand", min_ms: candStats.min, median_ms: candStats.median, p95_ms: candStats.p95,
    runs: candStats.runs, oracle_sha256: candHash, pass: pass ?? true, extra: candExtra,
  });
  if (baseStats) {
    jsonRows.push({
      id, bin: "base", min_ms: baseStats.min, median_ms: baseStats.median, p95_ms: baseStats.p95,
      runs: baseStats.runs, oracle_sha256: baseHash, pass: pass ?? true, extra: baseExtra,
    });
  }
}

export function printTable(): void {
  const headers = ["id", "base median", "cand median", "speedup", "cand p95", "oracle"];
  const rows = tableRows.map((r) => [
    r.id,
    r.baseMedian !== undefined ? `${r.baseMedian.toFixed(1)}ms` : "-",
    `${r.candMedian.toFixed(1)}ms`,
    r.baseMedian !== undefined ? `${(r.baseMedian / r.candMedian).toFixed(2)}x` : "-",
    `${r.candP95.toFixed(1)}ms`,
    r.oracleLabel,
  ]);
  const widths = headers.map((h, i) => Math.max(h.length, ...rows.map((r) => r[i].length)));
  const fmt = (r: string[]) => r.map((c, i) => c.padEnd(widths[i])).join(" | ");
  console.log("");
  console.log(fmt(headers));
  console.log(widths.map((w) => "-".repeat(w)).join("-|-"));
  for (const r of rows) console.log(fmt(r));
}
