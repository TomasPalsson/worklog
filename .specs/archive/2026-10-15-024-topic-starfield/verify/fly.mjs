// CHK001 star field: open it, fly with a held W, hover a star, click a topic chip, light mode.
// Run: bun fly.mjs <out-dir> [dark|light]
import puppeteer from "puppeteer-core";

const OUT = process.argv[2];
const SCHEME = process.argv[3] || "dark";
const browser = await puppeteer.launch({
  executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  headless: true,
  userDataDir: "/Users/tomas/.claude/jobs/491be4f8/tmp/pt/profile-fly",
  defaultViewport: { width: 1440, height: 900, deviceScaleFactor: 2 },
});
const page = await browser.newPage();
await page.emulateMediaFeatures([{ name: "prefers-color-scheme", value: SCHEME }]);
const errors = [];
page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
page.on("pageerror", (e) => errors.push(String(e)));
await page.goto("http://127.0.0.1:3344/stats?range=30d", { waitUntil: "domcontentloaded", timeout: 180000 });
await page.evaluate(() => localStorage.setItem("worklog.galaxyView", "stars"));
await page.reload({ waitUntil: "domcontentloaded" });
await page.waitForSelector(".sx-star canvas", { timeout: 180000 });
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
await page.evaluate(() => document.querySelector(".sx-star").scrollIntoView({ block: "center" }));
await wait(2500);
const field = await page.$(".sx-star");
const shot = (name) => field.screenshot({ path: `${OUT}/star-${SCHEME}-${name}.png` });
const fps = await page.evaluate(
  () =>
    new Promise((res) => {
      let n = 0;
      const t0 = performance.now();
      const f = () => (performance.now() - t0 < 1000 ? (n++, requestAnimationFrame(f)) : res(n));
      requestAnimationFrame(f);
    }),
);
console.log("raf per second while orbiting:", fps);
await shot("1-open");

const box = await (await page.$(".sx-star canvas")).boundingBox();
await page.mouse.click(box.x + box.width * 0.9, box.y + 20);
await page.keyboard.down("w");
await wait(1200);
await page.keyboard.up("w");
await wait(300);
await shot("2-after-W");

let hovered = null;
for (let gy = 0.2; gy < 0.9 && !hovered; gy += 0.04) {
  for (let gx = 0.1; gx < 0.9 && !hovered; gx += 0.02) {
    await page.mouse.move(box.x + box.width * gx, box.y + box.height * gy);
    // page.$eval is puppeteer's querySelector + a fixed local callback, not JS eval of a string.
    hovered = await page.$eval(".sx-star", (el) => el.querySelector(".sx-star-card")?.textContent ?? null).catch(() => null);
  }
}
console.log("hover card:", hovered ? hovered.slice(0, 90) : "NONE");
await shot("3-hover");

const before = await page.evaluate(() => document.querySelector(".sx-star canvas").toDataURL().length);
const chips = await page.$$(".sx-star-chip");
await chips[1].click();
await wait(1400);
const after = await page.evaluate(() => document.querySelector(".sx-star canvas").toDataURL().length);
console.log("chip fly changed the frame:", before !== after);
await shot("4-flew-to-topic");

await page.keyboard.press("r");
await wait(1200);
await shot("5-reset");
const scrollY0 = await page.evaluate(() => scrollY);
await page.keyboard.press("Escape");
await page.keyboard.press("ArrowDown");
await wait(300);
console.log("arrow scrolls page after Esc:", (await page.evaluate(() => scrollY)) !== scrollY0);
console.log("console errors:", errors.length, JSON.stringify(errors.slice(0, 3)));
await browser.close();
