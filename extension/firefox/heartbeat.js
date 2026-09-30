const PERSONAL_CONTAINER = "Personal";

export function skipReason({ paused, incognito, containerName, idleState, windowFocused }) {
  if (paused) return "paused";
  if (incognito) return "incognito";
  if (containerName === PERSONAL_CONTAINER) return "personal_container";
  if (idleState !== "active") return "idle";
  if (!windowFocused) return "unfocused";
  return null;
}

export function shouldSend(state) {
  return skipReason(state) === null;
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
