// T10 decide: time the Verdict classifier on one day's browser/Slack events and prove
// answers are unchanged.  bun perf/decide.ts --url http://127.0.0.1:9334
//   [--day 2026-09-24] [--concurrency 1] [--record out.json | --compare golden.json] [--passes 2]
// Pass 1 = cold tick, later passes = repeat ticks (the 15-min job asks again).
import { Database } from "bun:sqlite";
import { readdirSync, readFileSync, statSync, writeFileSync } from "fs";

const arg = (k: string, d?: string) => {
  const i = process.argv.indexOf(`--${k}`);
  return i > 0 ? process.argv[i + 1] : d;
};
const url = arg("url", "http://127.0.0.1:9334")!;
const day = arg("day", "2026-09-24")!;
const conc = Number(arg("concurrency", "1"));
const passes = Number(arg("passes", "2"));
const perf = process.env.PERF_DIR!;
const db = new Database(`${perf}/data/worklog.db`, { readonly: true });

const work = `${perf}/home/Desktop/Work`;
const dirs = readdirSync(work).filter((n) => statSync(`${work}/${n}`).isDirectory());
const pinned = db.query("SELECT folder FROM billing_folder_map").all().map((r: any) => r.folder as string);
const options = [...new Set([...pinned, ...dirs])].sort();
const states = (
  db
    .query(
      `SELECT source, title, details, container FROM events
        WHERE source IN ('firefox','slack') AND substr(started_at,1,10) = ?1 ORDER BY started_at, id`,
    )
    .all(day) as any[]
).map((r) => ({ source: r.source, title: r.title, details: r.details, container: r.container }));

async function classify(state: object): Promise<string> {
  const r = await fetch(`${url}/classify`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ state, options }),
  });
  return `${r.status} ${await r.text()}`;
}

async function pass(): Promise<{ ms: number; answers: string[] }> {
  const answers: string[] = new Array(states.length);
  let next = 0;
  const t = performance.now();
  await Promise.all(
    Array.from({ length: conc }, async () => {
      while (next < states.length) {
        const i = next++;
        answers[i] = await classify(states[i]);
      }
    }),
  );
  return { ms: performance.now() - t, answers };
}

const runs = [];
for (let p = 0; p < passes; p++) runs.push(await pass());
const answers = runs[0].answers;
const stable = runs.every((r) => r.answers.every((a, i) => a === answers[i]));
const report: any = {
  day, events: states.length, options: options.length, concurrency: conc,
  pass_s: runs.map((r) => +(r.ms / 1000).toFixed(3)), stable_across_passes: stable,
};
const rec = arg("record");
if (rec) writeFileSync(rec, JSON.stringify(answers, null, 1));
const cmp = arg("compare");
if (cmp) {
  const golden: string[] = JSON.parse(readFileSync(cmp, "utf8"));
  report.mismatches = golden.filter((g, i) => g !== answers[i]).length;
  report.oracle = report.mismatches === 0 && golden.length === answers.length ? "PASS" : "FAIL";
}
console.log(JSON.stringify(report));
