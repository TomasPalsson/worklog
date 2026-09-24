import { buildHeartbeat, shouldSend } from "./heartbeat.js";

const HEARTBEAT_URL = "http://127.0.0.1:9323/browser/heartbeat";
const ALARM_NAME = "heartbeat";
const IDLE_DETECTION_SECONDS = 120;

async function containerNameFor(tab) {
  if (!tab.cookieStoreId) return null;
  try {
    const identity = await browser.contextualIdentities.get(tab.cookieStoreId);
    return identity.name;
  } catch {
    // Default container has no contextualIdentity entry.
    return null;
  }
}

async function collectState() {
  const { paused } = await browser.storage.local.get("paused");
  const idleState = await browser.idle.queryState(IDLE_DETECTION_SECONDS);
  const win = await browser.windows.getLastFocused({ windowTypes: ["normal"] });
  const windowFocused = Boolean(win && win.focused);

  let tab = null;
  let containerName = null;
  if (windowFocused) {
    const [activeTab] = await browser.tabs.query({ active: true, windowId: win.id });
    tab = activeTab || null;
    if (tab) containerName = await containerNameFor(tab);
  }

  return { paused: Boolean(paused), idleState, windowFocused, tab, containerName };
}

async function tick() {
  const { paused, idleState, windowFocused, tab, containerName } = await collectState();
  const incognito = tab ? tab.incognito : false;

  if (!shouldSend({ paused, incognito, containerName, idleState, windowFocused })) return;

  const heartbeat = buildHeartbeat(tab, containerName, new Date());
  try {
    await fetch(HEARTBEAT_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(heartbeat),
    });
  } catch {
    // Daemon down: drop silently, never retry.
  }
}

browser.idle.setDetectionInterval(IDLE_DETECTION_SECONDS);
browser.alarms.create(ALARM_NAME, { periodInMinutes: 1 });
browser.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === ALARM_NAME) tick();
});
