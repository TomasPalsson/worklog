const checkbox = document.getElementById("paused");

browser.storage.local.get("paused").then(({ paused }) => {
  checkbox.checked = Boolean(paused);
});

checkbox.addEventListener("change", () => {
  browser.storage.local.set({ paused: checkbox.checked });
});
