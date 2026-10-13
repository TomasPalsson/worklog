// T051 check: every heartbeat raster has the same left and right edge on real data.
// Run: bun hb-align.mjs
import puppeteer from "puppeteer-core";
const browser = await puppeteer.launch({
  executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  headless: true,
  userDataDir: "/Users/tomas/.claude/jobs/491be4f8/tmp/pt/profile",
  defaultViewport: { width: 1280, height: 1800 },
});
const page = await browser.newPage();
await page.goto("http://127.0.0.1:3344/stats?range=30d", { waitUntil: "domcontentloaded", timeout: 180000 });
await page.waitForSelector(".hb-raster", { timeout: 180000 });
// $$eval is puppeteer's querySelectorAll+map with a fixed local function, not JS eval of a string.
const edges = await page.$$eval(".hb-raster", (els) => els.map((e) => { const r = e.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.right)]; }));
console.log("rasters:", edges.length, "distinct edges:", JSON.stringify([...new Set(edges.map(String))]));
await browser.close();
