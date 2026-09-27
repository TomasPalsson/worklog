// Behaviour oracle for the SLICE S1 tool-input trim (perf/README.md "Next"):
// events.raw_json for claude_tool rows is the declared, intended diff, but
// toolPreview (web/lib/detailText.ts) — the only consumer of `input` — must
// render every row exactly the same before and after. Cold-ingests the T3
// fixture (perf/bench.ts CLI_SCENARIOS.T3: empty DB, `day --day 2026-09-25`)
// with both binaries and compares claude_tool rows by source_id.
// Run: PERF_DIR=<fixture> CAND_BIN=<candidate> BASE_BIN=<baseline> \
//   bun test perf/trim.test.ts
import { describe, test, expect } from "bun:test";
import { rmSync, statSync } from "node:fs";
import { toolPreview } from "../web/lib/detailText";
import { PERF_DIR, cloneFixture, hermeticEnv, execCli, readonlyDb } from "./lib";

const CAND_BIN = process.env.CAND_BIN;
const BASE_BIN = process.env.BASE_BIN;
const DAY = "2026-09-25"; // matches perf/bench.ts CLI_SCENARIOS.T3

async function coldIngest(bin: string, tag: string): Promise<{ dbPath: string; dir: string }> {
  const dir = `${PERF_DIR}/runs/trim-oracle-${tag}-${Date.now()}`;
  await cloneFixture("empty", dir); // T3's cloneSrc
  const dataDir = `${dir}/data`;
  const { code } = await execCli(bin, ["day", "--day", DAY, "--no-serve"], hermeticEnv(dataDir));
  if (code !== 0) throw new Error(`${bin} day --day ${DAY} exited ${code}`);
  return { dbPath: `${dataDir}/worklog.db`, dir };
}

type ToolRow = { tool: string; input: unknown; raw_json: string };

function claudeToolRows(dbPath: string): Map<string, ToolRow> {
  const db = readonlyDb(dbPath);
  try {
    const rows = db
      .query("SELECT source_id, raw_json FROM events WHERE source = 'claude_tool'")
      .all() as { source_id: string; raw_json: string }[];
    const out = new Map<string, ToolRow>();
    for (const r of rows) {
      const raw = JSON.parse(r.raw_json);
      out.set(r.source_id, { tool: raw.tool, input: raw.input, raw_json: r.raw_json });
    }
    return out;
  } finally {
    db.close();
  }
}

describe.skipIf(!CAND_BIN || !BASE_BIN)("SLICE S1 tool-input trim oracle", () => {
  test("toolPreview renders every claude_tool row identically, base vs candidate", async () => {
    const base = await coldIngest(BASE_BIN!, "base");
    const cand = await coldIngest(CAND_BIN!, "cand");
    try {
      const baseRows = claudeToolRows(base.dbPath);
      const candRows = claudeToolRows(cand.dbPath);
      expect(candRows.size).toBeGreaterThan(0);
      expect(candRows.size).toBe(baseRows.size);

      let baseRawBytes = 0;
      let candRawBytes = 0;
      for (const [id, baseRow] of baseRows) {
        const candRow = candRows.get(id);
        expect(candRow, `missing claude_tool row ${id} in candidate`).toBeDefined();
        expect(toolPreview(candRow!.tool, candRow!.input)).toBe(toolPreview(baseRow.tool, baseRow.input));
        baseRawBytes += Buffer.byteLength(baseRow.raw_json);
        candRawBytes += Buffer.byteLength(candRow!.raw_json);
      }

      const baseDbBytes = statSync(base.dbPath).size;
      const candDbBytes = statSync(cand.dbPath).size;
      console.log(`T3 DB bytes: base ${baseDbBytes} -> cand ${candDbBytes}`);
      console.log(`claude_tool raw_json bytes: base ${baseRawBytes} -> cand ${candRawBytes}`);
      expect(candRawBytes).toBeLessThan(baseRawBytes);
    } finally {
      rmSync(base.dir, { recursive: true, force: true });
      rmSync(cand.dir, { recursive: true, force: true });
    }
  });
});
