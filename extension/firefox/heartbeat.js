const PERSONAL_CONTAINER = "Personal";

export function shouldSend({ paused, incognito, containerName, idleState, windowFocused }) {
  if (paused) return false;
  if (incognito) return false;
  if (containerName === PERSONAL_CONTAINER) return false;
  if (idleState !== "active") return false;
  if (!windowFocused) return false;
  return true;
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
