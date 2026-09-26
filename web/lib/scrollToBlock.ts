/** Scroll a BlockCard into view and flash it for a moment so the owner can
 * find it in the list. */
export function scrollToBlock(blockId: number) {
  const el = document.getElementById(`block-${blockId}`);
  if (!el) return;
  el.scrollIntoView({ behavior: "smooth", block: "center" });
  el.classList.add("block-flash");
  window.setTimeout(() => el.classList.remove("block-flash"), 1200);
}
