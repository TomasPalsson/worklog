"use client";

// Exceptions only: the one thing the owner might actually need to act on
// above an otherwise silent block list. Nothing renders when there's
// nothing to flag.

interface Props {
  /** Blocks without a jira_issue, excluding personal blocks. */
  count: number;
  /** Id of the first such block, to scroll to on "Show". Null when count is 0. */
  firstBlockId: number | null;
}

export function AttentionLine({ count, firstBlockId }: Props) {
  if (count === 0 || firstBlockId === null) return null;

  function show() {
    document
      .getElementById(`block-${firstBlockId}`)
      ?.scrollIntoView({ behavior: "smooth", block: "center" });
  }

  return (
    <p className="attention-line">
      {count} block{count === 1 ? "" : "s"} {count === 1 ? "has" : "have"} no ticket
      <button type="button" className="attention-show" onClick={show}>
        Show
      </button>
    </p>
  );
}
