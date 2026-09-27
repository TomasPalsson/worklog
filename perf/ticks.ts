// T2b/T2c: prove the transcript-collector tick skip (collectors/
// claude_transcript_cache.rs) doesn't change DB rows or CLI stdout.
// Registered from perf/bench.ts; see perf/README.md.

import {
  existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, truncateSync, utimesSync,
} from "node:fs";
import {
  PERF_DIR, cloneFixture, dumpDb, execCli, hermeticEnv, maskText, mkRundir, reportScenario, timedIteration,
} from "./lib";

const TICK_DAY = "2026-09-25";
const DAY_ARGS = ["day", "--day", TICK_DAY, "--no-serve"];

async function cloneDir(src: string, dest: string): Promise<void> {
  mkdirSync(dest, { recursive: true });
  const proc = Bun.spawn(["cp", "-c", "-R", src, dest], { stdout: "ignore", stderr: "pipe" });
  const code = await proc.exited;
  if (code !== 0) {
    const err = await new Response(proc.stderr).text();
    throw new Error(`cp -c -R ${src} -> ${dest} failed (${code}): ${err}`);
  }
}

async function tick(bin: string, dataDir: string, home: string): Promise<{ stdout: string; code: number }> {
  return execCli(bin, DAY_ARGS, { ...hermeticEnv(dataDir), HOME: home });
}

function oracleFor(
  dataDir: string, res: { stdout: string; code: number }, wallStart: number, wallEnd: number, rundir: string,
): string {
  const dbPath = `${dataDir}/worklog.db`;
  const dump = existsSync(dbPath) ? dumpDb(dbPath) : "(no db)";
  return maskText(`EXIT ${res.code}\nSTDOUT:\n${res.stdout}\nDB:\n${dump}`, wallStart, wallEnd, rundir);
}

// ---- T2b: same clone, two consecutive ticks, time only the second -------
// Baseline re-reads + re-parses every transcript on both ticks; the
// tick-skip candidate should do that work only on the first.
export async function runT2b(
  binPath: string, basePath: string | undefined, runsOverride: number | undefined,
): Promise<void> {
  const runs = runsOverride ?? 3;
  const candMs: number[] = [];
  const baseMs: number[] = [];
  let candOracle: string | undefined;
  let baseOracle: string | undefined;

  for (let i = 0; i < runs; i++) {
    const rundir = mkRundir("T2b");
    await cloneFixture("data", rundir);
    const dataDir = `${rundir}/data`;
    try {
      await tick(binPath, dataDir, `${PERF_DIR}/home`); // tick 1, untimed
      const c = await timedIteration(() => tick(binPath, dataDir, `${PERF_DIR}/home`)); // tick 2, timed
      candMs.push(c.ms);
      if (i === 0) candOracle = oracleFor(dataDir, c.result, c.wallStart, c.wallEnd, rundir);
    } finally {
      rmSync(rundir, { recursive: true, force: true });
    }

    if (!basePath) continue;
    const baseRundir = mkRundir("T2b-base");
    await cloneFixture("data", baseRundir);
    const baseDataDir = `${baseRundir}/data`;
    try {
      await tick(basePath, baseDataDir, `${PERF_DIR}/home`);
      const b = await timedIteration(() => tick(basePath, baseDataDir, `${PERF_DIR}/home`));
      baseMs.push(b.ms);
      if (i === 0) baseOracle = oracleFor(baseDataDir, b.result, b.wallStart, b.wallEnd, baseRundir);
    } finally {
      rmSync(baseRundir, { recursive: true, force: true });
    }
  }

  console.log(`ran T2b: ${runs} run(s) (+1 warm-up tick) per binary`);
  reportScenario("T2b", candMs, baseMs, candOracle, baseOracle);
}

// ---- T2c: tick 1 on a truncated tree, tick 2 on the full tree -----------
// A truncated-then-replaced file has a different size/mtime, so the
// fingerprint must miss on tick 2 and fully reprocess it — same final DB as
// always-fully-reprocessing (baseline).
const TRUNC_CUTOFF_MS = new Date(2026, 8, 24, 0, 0, 0).getTime(); // 2026-09-24 local, matches fixture.sh's SINCE

function* walkJsonl(dir: string): Generator<string> {
  if (!existsSync(dir)) return;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const p = `${dir}/${entry.name}`;
    if (entry.isDirectory()) yield* walkJsonl(p);
    else if (entry.isFile() && entry.name.endsWith(".jsonl")) yield p;
  }
}

// Builds `${PERF_DIR}/home-trunc`: an APFS clone of the fixture home whose
// .jsonl files touched since the fixture's SINCE cutoff are truncated at
// the last newline before 70% of their size, mtime set 60s earlier.
async function buildTruncatedHome(): Promise<string> {
  const dest = `${PERF_DIR}/home-trunc`;
  rmSync(dest, { recursive: true, force: true });
  await cloneDir(`${PERF_DIR}/home`, dest);
  for (const path of walkJsonl(`${dest}/.claude`)) {
    const st = statSync(path);
    if (st.mtimeMs < TRUNC_CUTOFF_MS) continue;
    const buf = readFileSync(path);
    const cutoff = Math.floor(buf.length * 0.7);
    const nl = buf.lastIndexOf(0x0a, cutoff);
    truncateSync(path, nl < 0 ? 0 : nl + 1);
    utimesSync(path, st.atime, new Date(st.mtimeMs - 60_000));
  }
  return dest;
}

export async function runT2c(
  binPath: string, basePath: string | undefined, runsOverride: number | undefined,
): Promise<void> {
  const runs = runsOverride ?? 3;
  const truncHome = await buildTruncatedHome();
  const fullHome = `${PERF_DIR}/home`;

  const candMs: number[] = [];
  const baseMs: number[] = [];
  let candOracle: string | undefined;
  let baseOracle: string | undefined;

  for (let i = 0; i < runs; i++) {
    const rundir = mkRundir("T2c");
    await cloneFixture("data", rundir);
    const dataDir = `${rundir}/data`;
    try {
      await tick(binPath, dataDir, truncHome); // tick 1: truncated tree, untimed
      const c = await timedIteration(() => tick(binPath, dataDir, fullHome)); // tick 2: full tree, timed
      candMs.push(c.ms);
      if (i === 0) candOracle = oracleFor(dataDir, c.result, c.wallStart, c.wallEnd, rundir);
    } finally {
      rmSync(rundir, { recursive: true, force: true });
    }

    if (!basePath) continue;
    const baseRundir = mkRundir("T2c-base");
    await cloneFixture("data", baseRundir);
    const baseDataDir = `${baseRundir}/data`;
    try {
      await tick(basePath, baseDataDir, truncHome);
      const b = await timedIteration(() => tick(basePath, baseDataDir, fullHome));
      baseMs.push(b.ms);
      if (i === 0) baseOracle = oracleFor(baseDataDir, b.result, b.wallStart, b.wallEnd, baseRundir);
    } finally {
      rmSync(baseRundir, { recursive: true, force: true });
    }
  }

  rmSync(truncHome, { recursive: true, force: true });
  console.log(`ran T2c: ${runs} run(s) (truncated tick 1 + full tick 2) per binary`);
  reportScenario("T2c", candMs, baseMs, candOracle, baseOracle);
}
