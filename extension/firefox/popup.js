import { viewState } from "./popup-state.js";

const STATUS_URL = "http://127.0.0.1:9323/browser/status";
const RECORDING_URL = "http://127.0.0.1:9323/browser/recording";
const REVIEW_URL = "http://127.0.0.1:3333";
const REQUEST_TIMEOUT_MILLISECONDS = 1000;

const sign = document.getElementById("sign");
const primary = document.getElementById("primary");
const secondary = document.getElementById("secondary");

let paused = false;
let lastHeartbeat = null;
let status = null;
let daemonDown = false;

async function request(url, options) {
  try {
    const response = await fetch(url, { ...options, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MILLISECONDS) });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    status = await response.json();
    daemonDown = false;
  } catch {
    status = null;
    daemonDown = true;
  }
}

function fetchStatus() {
  return request(STATUS_URL);
}

function setRecording(on) {
  return request(RECORDING_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ on }),
  });
}

function show(button, choice) {
  button.hidden = choice === null;
  if (choice === null) return;
  button.textContent = choice.label;
  button.dataset.action = choice.action;
  button.disabled = false;
  button.removeAttribute("aria-busy");
}

function render() {
  const view = viewState({ status, daemonDown, paused, lastHeartbeat, now: new Date() });
  sign.dataset.status = view.status;
  document.getElementById("pictogram").setAttribute("href", `#icon-${view.status}`);
  document.getElementById("headline").textContent = view.headline;
  document.getElementById("detail").textContent = view.detail;

  const tab = document.getElementById("tab");
  tab.textContent = view.tab ? view.tab.title || view.tab.url : "—";
  tab.title = view.tab ? `${view.tab.title}\n${view.tab.url}` : "";
  document.getElementById("today").textContent = view.minutesToday;
  document.getElementById("row-left").hidden = view.timeLeft === null;
  document.getElementById("left").textContent = view.timeLeft ?? "";

  show(primary, view.primary);
  show(secondary, view.secondary);
}

async function perform(button) {
  const { action } = button.dataset;
  for (const each of [primary, secondary]) each.disabled = true;
  button.setAttribute("aria-busy", "true");
  button.textContent = "Working…";
  try {
    if (action === "pause" || action === "resume") {
      paused = action === "pause";
      await browser.storage.local.set({ paused });
      await fetchStatus();
    } else {
      await setRecording(action === "start");
    }
  } finally {
    render();
    (primary.hidden ? document.getElementById("open") : primary).focus();
  }
}

for (const button of [primary, secondary]) button.addEventListener("click", () => perform(button));

document.getElementById("open").addEventListener("click", (event) => {
  event.preventDefault();
  browser.tabs.create({ url: REVIEW_URL });
  window.close();
});

const stored = await browser.storage.local.get(["paused", "lastHeartbeat"]);
paused = Boolean(stored.paused);
lastHeartbeat = stored.lastHeartbeat ?? null;
await fetchStatus();
render();
(primary.hidden ? document.getElementById("open") : primary).focus();
