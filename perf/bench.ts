#!/usr/bin/env bun
// Timing + behaviour eval for the worklog Rust binary. See perf/README.md.
// Usage: bun perf/bench.ts --bin <candidate> [--base <baseline>] [--only T1,T4]
//        [--runs N] [--json <out.json>] [--selftest]
// Requires env PERF_DIR (fixture root built by perf/fixture.sh).

import { Database } from "bun:sqlite";
import {
  rmSync, readdirSync, statSync, existsSync, chmodSync, writeFileSync,
} from "node:fs";
import {
  PERF_DIR, RUNS_ROOT, hermeticEnv, mkRundir, cloneFixture, timedIteration, execCli,
  readonlyDb, dumpDb, maskText, printDiff, daemonFetch, waitHealthy, spawnDaemon, stopDaemon,
  reportScenario, printTable, jsonRows,
} from "./lib";
import { computeT6bBlockId, buildT6bScenario } from "./details";
import { runT2b, runT2c } from "./ticks";

const DAY = "2026-09-22";
const WEEK_START = 21; // Mon 2026-09-21 .. 2026-09-27

// ---- cleanup registry: killed/removed on normal teardown AND on process exit ----
type CleanupTarget = { proc?: { kill(sig?: number): void }; dir?: string };
const cleanupTargets: CleanupTarget[] = [];
process.on("exit", () => {
  for (const t of cleanupTargets) {
    if (t.proc) { try { t.proc.kill(9); } catch {} }
    if (t.dir) { try { rmSync(t.dir, { recursive: true, force: true }); } catch {} }
  }
});

// ---- CLI scenarios ----
type CliScenario = {
  id: string; kind: "cli"; defaultRuns: number; cloneSrc: "data" | "empty";
  buildArgs: () => string[]; stdin?: () => string;
  extra?: (rundir: string) => Record<string, unknown>;
};

function firstWorkDirName(): string {
  const dir = `${PERF_DIR}/home/Desktop/Work`;
  const names = readdirSync(dir, { withFileTypes: true })
    .filter((d) => d.isDirectory()).map((d) => d.name).sort();
  if (names.length === 0) throw new Error(`no Work dirs found under ${dir}`);
  return names[0];
}

const CLI_SCENARIOS: Record<string, CliScenario> = {
  T1: {
    id: "T1", kind: "cli", defaultRuns: 30, cloneSrc: "data",
    buildArgs: () => ["hook-run"],
    stdin: () => JSON.stringify({
      hook_event_name: "SessionStart", session_id: "perf-s1",
      cwd: `${PERF_DIR}/home/Desktop/Work/${firstWorkDirName()}`, source: "startup",
    }),
  },
  T2: {
    id: "T2", kind: "cli", defaultRuns: 5, cloneSrc: "data",
    buildArgs: () => ["day", "--day", "2026-09-25", "--no-serve"],
  },
  T3: {
    id: "T3", kind: "cli", defaultRuns: 3, cloneSrc: "empty",
    buildArgs: () => ["day", "--day", "2026-09-25", "--no-serve"],
    extra: (rundir) => {
      const dbPath = `${rundir}/data/worklog.db`;
      const db = new Database(dbPath);
      try {
        db.run("PRAGMA wal_checkpoint(TRUNCATE);");
        const events = (db.query("SELECT count(*) as n FROM events").get() as { n: number }).n;
        return { db_bytes: statSync(dbPath).size, events };
      } finally {
        db.close();
      }
    },
  },
  T8a: { id: "T8a", kind: "cli", defaultRuns: 20, cloneSrc: "data", buildArgs: () => ["summary", "--day", DAY] },
  T8b: { id: "T8b", kind: "cli", defaultRuns: 10, cloneSrc: "data", buildArgs: () => ["week", "--day", DAY] },
  T8c: { id: "T8c", kind: "cli", defaultRuns: 20, cloneSrc: "data", buildArgs: () => ["block", "list", "--day", DAY] },
  T8d: { id: "T8d", kind: "cli", defaultRuns: 10, cloneSrc: "data", buildArgs: () => ["infer", "--day", DAY] },
  T9: { id: "T9", kind: "cli", defaultRuns: 3, cloneSrc: "data", buildArgs: () => ["db", "purge", "--days", "3"] },
};

async function execAndCapture(
  sc: CliScenario, bin: string, rundir: string, stdinText: string | undefined, captureOracle: boolean,
): Promise<{ oracleRaw?: string; extra?: Record<string, unknown> }> {
  const { stdout, code } = await execCli(bin, sc.buildArgs(), hermeticEnv(`${rundir}/data`), stdinText);
  if (!captureOracle) return {};
  const dbPath = `${rundir}/data/worklog.db`;
  const dump = existsSync(dbPath) ? dumpDb(dbPath) : "(no db)";
  return { oracleRaw: `EXIT ${code}\nSTDOUT:\n${stdout}\nDB:\n${dump}`, extra: sc.extra?.(rundir) };
}

// clone (untimed) + run + cleanup, all together — only for untimed warm-up calls
// and selftest, where clone time doesn't need to be excluded from anything.
async function cloneAndRun(
  sc: CliScenario, bin: string, stdinText: string | undefined, captureOracle: boolean,
): Promise<{ rundir: string; oracleRaw?: string; extra?: Record<string, unknown> }> {
  const rundir = mkRundir(sc.id);
  try {
    await cloneFixture(sc.cloneSrc, rundir);
    return { rundir, ...(await execAndCapture(sc, bin, rundir, stdinText, captureOracle)) };
  } finally {
    rmSync(rundir, { recursive: true, force: true });
  }
}

// clone (untimed), then time only the exec+capture span — cp -c -R clone time
// must not count against the scenario's measured latency.
async function timedCliRun(
  sc: CliScenario, bin: string, stdinText: string | undefined, captureOracle: boolean,
): Promise<{
  ms: number; wallStart: number; wallEnd: number; rundir: string;
  oracleRaw?: string; extra?: Record<string, unknown>;
}> {
  const rundir = mkRundir(sc.id);
  await cloneFixture(sc.cloneSrc, rundir);
  try {
    const t = await timedIteration(() => execAndCapture(sc, bin, rundir, stdinText, captureOracle));
    return { ms: t.ms, wallStart: t.wallStart, wallEnd: t.wallEnd, rundir, ...t.result };
  } finally {
    rmSync(rundir, { recursive: true, force: true });
  }
}

// ---- daemon scenarios ----
type DaemonResp = Record<string, { status: number; body: string }>;
type DaemonScenario = { id: string; kind: "daemon"; defaultRuns: number; run: (port: number) => Promise<DaemonResp> };

function computeHeaviestBlockId(): number {
  const db = readonlyDb(`${PERF_DIR}/data/worklog.db`);
  try {
    const row = db.query(
      `SELECT b.id as id FROM blocks b JOIN block_events be ON be.block_id=b.id
       WHERE b.day=? GROUP BY b.id ORDER BY count(*) DESC, b.id LIMIT 1`,
    ).get(DAY) as { id: number } | null;
    if (!row) throw new Error(`no block with events found for day ${DAY}`);
    return row.id;
  } finally {
    db.close();
  }
}

function buildDaemonScenarios(blockId: number): Record<string, DaemonScenario> {
  const days7 = Array.from({ length: 7 }, (_, i) => `2026-09-${WEEK_START + i}`);
  return {
    T4: {
      id: "T4", kind: "daemon", defaultRuns: 30,
      run: async (port) => {
        const [d1, tickets] = await Promise.all(
          [daemonFetch(port, `/days/${DAY}`), daemonFetch(port, "/tickets")],
        );
        const [routed, billing, elsewhere] = await Promise.all([
          daemonFetch(port, `/days/${DAY}/routed?include_hidden=true`),
          daemonFetch(port, "/billing/registry"),
          daemonFetch(port, `/days/${DAY}/elsewhere`),
        ]);
        return {
          [`GET /days/${DAY}`]: d1, "GET /tickets": tickets,
          [`GET /days/${DAY}/routed?include_hidden=true`]: routed,
          "GET /billing/registry": billing, [`GET /days/${DAY}/elsewhere`]: elsewhere,
        };
      },
    },
    T5: {
      id: "T5", kind: "daemon", defaultRuns: 20,
      run: async (port) => {
        const results = await Promise.all(days7.map((d) => daemonFetch(port, `/days/${d}`)));
        return Object.fromEntries(days7.map((d, i) => [`GET /days/${d}`, results[i]]));
      },
    },
    T6: {
      id: "T6", kind: "daemon", defaultRuns: 20,
      run: async (port) => {
        const [details, events] = await Promise.all([
          daemonFetch(port, `/blocks/${blockId}/details`), daemonFetch(port, `/blocks/${blockId}/events`),
        ]);
        return { [`GET /blocks/${blockId}/details`]: details, [`GET /blocks/${blockId}/events`]: events };
      },
    },
    T6b: buildT6bScenario(computeT6bBlockId()),
    T7: {
      id: "T7", kind: "daemon", defaultRuns: 30,
      run: async (port) => ({ [`GET /export/${DAY}`]: await daemonFetch(port, `/export/${DAY}`) }),
    },
  };
}

function serializeDaemonResp(resp: DaemonResp): string {
  return Object.keys(resp).sort().map((k) => `${k}\t${resp[k].status}\n${resp[k].body}`).join("\n===\n");
}

// ---- CLI scenario driver ----
async function runCliFlow(
  sc: CliScenario, binPath: string, basePath: string | undefined, runsOverride: number | undefined,
): Promise<void> {
  const runs = runsOverride ?? sc.defaultRuns;
  const stdinText = sc.stdin ? sc.stdin() : undefined;

  await cloneAndRun(sc, binPath, stdinText, false); // untimed warm-up
  if (basePath) await cloneAndRun(sc, basePath, stdinText, false);

  const candMs: number[] = [], baseMs: number[] = [];
  let candOracle: string | undefined, baseOracle: string | undefined;
  let candExtra: Record<string, unknown> | undefined, baseExtra: Record<string, unknown> | undefined;

  for (let i = 0; i < runs; i++) {
    const captureThis = i === 0;
    const c = await timedCliRun(sc, binPath, stdinText, captureThis);
    candMs.push(c.ms);
    if (captureThis) {
      candOracle = maskText(c.oracleRaw!, c.wallStart, c.wallEnd, c.rundir);
      candExtra = c.extra;
    }
    if (basePath) {
      const b = await timedCliRun(sc, basePath, stdinText, captureThis);
      baseMs.push(b.ms);
      if (captureThis) {
        baseOracle = maskText(b.oracleRaw!, b.wallStart, b.wallEnd, b.rundir);
        baseExtra = b.extra;
      }
    }
  }

  console.log(`ran ${sc.id}: ${runs} run(s) (+1 warm-up) per binary`);
  reportScenario(sc.id, candMs, baseMs, candOracle, baseOracle, candExtra, baseExtra);
}

// ---- daemon scenario driver: one daemon per binary shared across all selected T4-T7 ----
async function runDaemonScenarios(
  ids: string[], binPath: string, basePath: string | undefined, runsOverride: number | undefined, blockId: number,
): Promise<void> {
  if (ids.length === 0) return;
  const scenarios = buildDaemonScenarios(blockId);

  const candRun = mkRundir("daemon-cand");
  await cloneFixture("data", candRun);
  const candProc = spawnDaemon(binPath, candRun, 19323);
  cleanupTargets.push({ proc: candProc, dir: candRun });
  await waitHealthy(19323);

  let baseProc: ReturnType<typeof spawnDaemon> | undefined;
  let baseRun: string | undefined;
  if (basePath) {
    baseRun = mkRundir("daemon-base");
    await cloneFixture("data", baseRun);
    baseProc = spawnDaemon(basePath, baseRun, 19324);
    cleanupTargets.push({ proc: baseProc, dir: baseRun });
    await waitHealthy(19324);
  }

  try {
    for (const id of ids) {
      const sc = scenarios[id];
      const runs = runsOverride ?? sc.defaultRuns;
      for (let i = 0; i < 3; i++) await sc.run(19323); // warm-up
      if (basePath) for (let i = 0; i < 3; i++) await sc.run(19324);

      const candMs: number[] = [], baseMs: number[] = [];
      let candOracle: string | undefined, baseOracle: string | undefined;
      for (let i = 0; i < runs; i++) {
        const c = await timedIteration(() => sc.run(19323));
        candMs.push(c.ms);
        if (i === 0) candOracle = maskText(serializeDaemonResp(c.result), c.wallStart, c.wallEnd, candRun, 19323);
        if (basePath) {
          const b = await timedIteration(() => sc.run(19324));
          baseMs.push(b.ms);
          if (i === 0) baseOracle = maskText(serializeDaemonResp(b.result), b.wallStart, b.wallEnd, baseRun!, 19324);
        }
      }
      console.log(`ran ${id}: ${runs} run(s) (+3 warm-up) per binary`);
      reportScenario(id, candMs, baseMs, candOracle, baseOracle);
    }
  } finally {
    if (baseProc) await stopDaemon(baseProc);
    await stopDaemon(candProc);
    rmSync(candRun, { recursive: true, force: true });
    if (baseRun) rmSync(baseRun, { recursive: true, force: true });
  }
}

// ---- selftest ----
async function captureOracleOnce(
  sc: CliScenario, binPath: string, basePath: string,
): Promise<{ candOracle: string; baseOracle: string }> {
  const stdinText = sc.stdin ? sc.stdin() : undefined;
  const c = await timedIteration(() => cloneAndRun(sc, binPath, stdinText, true));
  const b = await timedIteration(() => cloneAndRun(sc, basePath, stdinText, true));
  return {
    candOracle: maskText(c.result.oracleRaw!, c.wallStart, c.wallEnd, c.result.rundir),
    baseOracle: maskText(b.result.oracleRaw!, b.wallStart, b.wallEnd, b.result.rundir),
  };
}

async function runSelftest(basePath: string | undefined): Promise<number> {
  if (!basePath) { console.error("--selftest requires --base"); return 1; }
  let ok = true;

  const r1 = await captureOracleOnce(CLI_SCENARIOS.T8a, basePath, basePath);
  if (r1.candOracle !== r1.baseOracle) {
    console.error("SELFTEST (a) FAILED: base vs base on T8a produced different oracles");
    printDiff("T8a-selftest-a", r1.baseOracle, r1.candOracle);
    ok = false;
  } else {
    console.log("SELFTEST (a) ok: base vs base on T8a matches");
  }

  const mutantPath = `${RUNS_ROOT}/mutant.sh`;
  writeFileSync(mutantPath, [
    "#!/bin/bash",
    `"${basePath}" "$@"`,
    "rc=$?",
    'if [ -n "$WORKLOG_HOME" ] && [ -f "$WORKLOG_HOME/worklog.db" ]; then',
    '  /usr/bin/sqlite3 "$WORKLOG_HOME/worklog.db" "UPDATE blocks SET description = coalesce(description,\'\') || \'!\' WHERE id = (SELECT min(id) FROM blocks)"',
    "fi",
    "exit $rc",
    "",
  ].join("\n"));
  chmodSync(mutantPath, 0o755);
  const r2 = await captureOracleOnce(CLI_SCENARIOS.T8a, mutantPath, basePath);
  if (r2.candOracle === r2.baseOracle) {
    console.error("SELFTEST (b) FAILED: mutant vs base on T8a produced identical oracles (no teeth)");
    ok = false;
  } else {
    console.log("SELFTEST (b) ok: mutant vs base on T8a correctly differs");
  }

  const r3 = await captureOracleOnce(CLI_SCENARIOS.T1, basePath, basePath);
  if (r3.candOracle !== r3.baseOracle) {
    console.error("SELFTEST (c) FAILED: T1 base vs base produced different oracles (masking broken)");
    printDiff("T1-selftest-c", r3.baseOracle, r3.candOracle);
    ok = false;
  } else {
    console.log("SELFTEST (c) ok: T1 base vs base matches despite hook timestamps");
  }

  console.log(ok ? "SELFTEST OK" : "SELFTEST FAILED");
  return ok ? 0 : 1;
}

// ---- arg parsing ----
type Args = { bin?: string; base?: string; only?: string[]; runs?: number; json?: string; selftest: boolean };

function parseArgs(argv: string[]): Args {
  const out: Args = { selftest: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--bin") out.bin = argv[++i];
    else if (a === "--base") out.base = argv[++i];
    else if (a === "--only") out.only = argv[++i].split(",").map((s) => s.trim());
    else if (a === "--runs") out.runs = Number(argv[++i]);
    else if (a === "--json") out.json = argv[++i];
    else if (a === "--selftest") out.selftest = true;
    else { console.error(`unknown arg: ${a}`); process.exit(1); }
  }
  return out;
}

const ALL_CLI_IDS = ["T1", "T2", "T3", "T8a", "T8b", "T8c", "T8d", "T9"];
const ALL_DAEMON_IDS = ["T4", "T5", "T6", "T6b", "T7"];
// T2b/T2c (perf/ticks.ts): extra proof for T2, not their own user story —
// reachable via --only but excluded from the no-flags default sweep.
const TICK_IDS = ["T2b", "T2c"];
const ALL_IDS = ["T1", "T2", "T3", "T4", "T5", "T6", "T6b", "T7", "T8a", "T8b", "T8c", "T8d", "T9", ...TICK_IDS];

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));

  if (args.selftest) process.exit(await runSelftest(args.base));

  if (!args.bin) { console.error("--bin is required (unless --selftest)"); process.exit(1); }

  for (const id of args.only ?? []) {
    if (!ALL_IDS.includes(id)) { console.error(`unknown scenario id in --only: ${id}`); process.exit(1); }
  }

  const selected = args.only ? ALL_IDS.filter((id) => args.only!.includes(id)) : ALL_IDS.filter((id) => !TICK_IDS.includes(id));
  const cliIds = selected.filter((id) => ALL_CLI_IDS.includes(id));
  const daemonIds = selected.filter((id) => ALL_DAEMON_IDS.includes(id));
  const tickIds = selected.filter((id) => TICK_IDS.includes(id));

  for (const id of cliIds) await runCliFlow(CLI_SCENARIOS[id], args.bin, args.base, args.runs);

  if (daemonIds.length > 0) {
    await runDaemonScenarios(daemonIds, args.bin, args.base, args.runs, computeHeaviestBlockId());
  }

  if (tickIds.includes("T2b")) await runT2b(args.bin, args.base, args.runs);
  if (tickIds.includes("T2c")) await runT2c(args.bin, args.base, args.runs);

  printTable();
  if (args.json) {
    writeFileSync(args.json, JSON.stringify(jsonRows, null, 2));
    console.log(`\nwrote ${args.json}`);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
