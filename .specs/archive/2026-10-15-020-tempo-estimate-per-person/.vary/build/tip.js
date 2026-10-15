// Hover / arrow-key tooltip for each burn-up chart. The chart and table work without it.
const fmt = (m) => { const h = Math.floor(m / 60), r = m % 60; return h && r ? `${h}h ${r}m` : h ? `${h}h` : `${r}m`; };
document.querySelectorAll(".burn").forEach((fig) => {
  const d = JSON.parse(fig.dataset.burn), svg = fig.querySelector("svg"), tip = fig.querySelector(".tip"), cross = svg.querySelector(".cross");
  let idx = d.x.length - 1;
  const show = (i) => {
    idx = i;
    cross.setAttribute("x1", d.x[i]); cross.setAttribute("x2", d.x[i]);
    const last = i === d.x.length - 1;
    let tot = 0, rows = "";
    d.people.forEach(([name, , c]) => { tot += c[i]; rows += `<p><span>${name}</span><span>${fmt(c[i])}</span></p>`; });
    if (last && d.pending) rows += `<p><span>This block</span><span>+${fmt(d.pending)}</span></p>`;
    const all = tot + (last ? d.pending : 0);
    const state = all > d.est ? `${fmt(all - d.est)} over` : `${fmt(d.est - all)} left`;
    tip.innerHTML = `<b>Total by end of ${last ? "today" : d.days[i]}</b>${rows}<p class="tot"><span>${fmt(all)} of ${fmt(d.est)}</span><span>${state}</span></p>`;
    const box = svg.getBoundingClientRect(), px = (d.x[i] / 260) * box.width;
    tip.style.left = Math.min(Math.max(0, px - 90), Math.max(0, fig.clientWidth - 190)) + "px";
    tip.style.top = box.top - fig.getBoundingClientRect().top - tip.offsetHeight - 6 + "px"; // above the chart, never over "Show as table"
    fig.dataset.hover = "";
  };
  const hide = () => delete fig.dataset.hover;
  const at = (e) => {
    const box = svg.getBoundingClientRect(), x = ((e.clientX - box.left) / box.width) * 260;
    let best = 0; d.x.forEach((v, i) => { if (Math.abs(v - x) < Math.abs(d.x[best] - x)) best = i; });
    show(best);
  };
  svg.addEventListener("pointermove", at);
  svg.addEventListener("pointerdown", at); // a tap on a phone shows the day too
  svg.addEventListener("pointerleave", hide);
  svg.addEventListener("focus", () => show(idx));
  svg.addEventListener("blur", hide);
  svg.addEventListener("keydown", (e) => {
    if (e.key === "ArrowLeft") { e.preventDefault(); show(Math.max(0, idx - 1)); }
    if (e.key === "ArrowRight") { e.preventDefault(); show(Math.min(d.x.length - 1, idx + 1)); }
    if (e.key === "Escape") hide();
  });
});
