// CHK001 /stats: the Topics section renders on real data (sandbox daemon on a DB copy).
// Run: bun chk001-stats.mjs <out-dir> [range]
import puppeteer from "puppeteer-core";
const OUT = process.argv[2];
const RANGE = process.argv[3] || "7d";
const browser = await puppeteer.launch({
  executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  headless: true,
  userDataDir: "/Users/tomas/.claude/jobs/491be4f8/tmp/pt/profile",
  defaultViewport: { width: 1280, height: 1800 },
});
const page = await browser.newPage();
const errors = [];
page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
page.on("pageerror", (e) => errors.push(String(e)));
const t0 = Date.now();
await page.goto(`http://127.0.0.1:3344/stats?range=${RANGE}`, { waitUntil: "domcontentloaded", timeout: 180000 });
await page.waitForFunction(() => /Topics/i.test(document.body.innerText), { timeout: 180000 });
await page
  .waitForFunction(() => !/Loading topics/.test(document.body.innerText), { timeout: 120000 })
  .catch(() => console.log("topics still loading after 120 s"));
console.log("topics ready after", ((Date.now() - t0) / 1000).toFixed(1), "s");
await new Promise((r) => setTimeout(r, 1500));
const section = await page.evaluate(() => {
  const h = [...document.querySelectorAll("h2,h3")].find((e) => /^topics$/i.test(e.textContent.trim()));
  const box = h?.closest("section") || h?.parentElement;
  box?.scrollIntoView();
  return box ? box.innerText : null;
});
console.log("---- Topics section text ----\n" + (section || "(not found)").slice(0, 1500));
const handle = await page.evaluateHandle(() => {
  const h = [...document.querySelectorAll("h2,h3")].find((e) => /^topics$/i.test(e.textContent.trim()));
  return h?.closest("section") || h?.parentElement;
});
if (handle.asElement()) await handle.asElement().screenshot({ path: `${OUT}/CHK001-stats-topics-${RANGE}.png` });
await page.screenshot({ path: `${OUT}/CHK001-stats-full-${RANGE}.png`, fullPage: true });
console.log("console errors:", errors.length, JSON.stringify(errors.slice(0, 3)));
await browser.close();
