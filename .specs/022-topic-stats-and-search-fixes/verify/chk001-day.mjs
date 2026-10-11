// CHK001 day view: noise tag + bulk dismiss, against a sandbox daemon on a DB copy.
// Run: bun chk001-day.mjs <out-dir> [day]
import puppeteer from "puppeteer-core";
const OUT = process.argv[2];
const DAY = process.argv[3] || "2026-10-08";
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
await page.goto(`http://127.0.0.1:3344/${DAY}`, { waitUntil: "domcontentloaded", timeout: 180000 });
await page.waitForFunction(() => /Unsorted|unsorted|noise/.test(document.body.innerText), { timeout: 180000 });
await new Promise((r) => setTimeout(r, 3000));
const text = () => page.evaluate(() => document.body.innerText);
let body = await text();
const tags = body.match(/dismissed \d+× before/g) || [];
const button = body.match(/Dismiss \d+ noise groups?/)?.[0];
console.log("tags:", tags.length, JSON.stringify(tags.slice(0, 6)));
console.log("button:", button);
await page.screenshot({ path: `${OUT}/CHK001-day-before.png`, fullPage: true });
if (button) {
  const [btn] = await page.$$(`xpath/.//button[contains(., "noise group")]`);
  await btn.click();
  await new Promise((r) => setTimeout(r, 500));
  await page.screenshot({ path: `${OUT}/CHK001-day-confirm.png`, fullPage: true });
  const [yes] = await page.$$(`xpath/.//button[normalize-space(.)="Yes"]`);
  if (yes) {
    const t0 = Date.now();
    await yes.click();
    await page
      .waitForFunction(() => !/Dismiss \d+ noise group/.test(document.body.innerText), { timeout: 60000 })
      .catch(() => console.log("button still there after 60 s"));
    console.log("dismiss took", ((Date.now() - t0) / 1000).toFixed(1), "s");
    await new Promise((r) => setTimeout(r, 1500));
  } else console.log("no Yes button found");
  body = await text();
  console.log("after: tags", (body.match(/dismissed \d+× before/g) || []).length, "button", body.match(/Dismiss \d+ noise groups?/)?.[0] || "gone");
  await page.screenshot({ path: `${OUT}/CHK001-day-after.png`, fullPage: true });
}
console.log("console errors:", errors.length, JSON.stringify(errors.slice(0, 3)));
await browser.close();
