const PERSONAL_CONTAINER = "Personal";

export function skipReason({ paused, incognito, containerName, idleState, windowFocused }) {
  if (incognito) return "incognito";
  if (containerName === PERSONAL_CONTAINER) return "personal_container";
  if (paused) return "paused";
  if (idleState !== "active") return "idle";
  if (!windowFocused) return "unfocused";
  return null;
}

export function storableTab(tab, containerName) {
  if (!tab || tab.incognito || containerName === PERSONAL_CONTAINER) return { title: null, url: null };
  return { title: tab.title, url: tab.url };
}

export function buildHeartbeat(tab, containerName, now) {
  return {
    ts: now.toISOString(),
    url: tab.url,
    title: tab.title,
    container: containerName,
    incognito: tab.incognito,
  };
}
