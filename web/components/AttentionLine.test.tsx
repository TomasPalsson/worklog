// Exceptions-only attention line above the block list: nothing when there's
// nothing to flag, one line + a Show link that scrolls to the first
// offending block when there is.

import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { AttentionLine } from "./AttentionLine";

afterEach(() => {
  cleanup();
});

describe("AttentionLine", () => {
  it("renders nothing when count is 0", () => {
    const { container } = render(<AttentionLine count={0} firstBlockId={null} />);
    expect(container.firstChild).toBeNull();
  });

  it("renders nothing when there's no first block id even if count is nonzero", () => {
    const { container } = render(<AttentionLine count={2} firstBlockId={null} />);
    expect(container.firstChild).toBeNull();
  });

  it("says one block has no ticket for a count of 1", () => {
    render(<AttentionLine count={1} firstBlockId={7} />);
    expect(screen.getByText(/1 block has no ticket/)).toBeTruthy();
  });

  it("says N blocks have no ticket for a count above 1", () => {
    render(<AttentionLine count={2} firstBlockId={7} />);
    expect(screen.getByText(/2 blocks have no ticket/)).toBeTruthy();
  });

  it('"Show" scrolls to the first offending block', () => {
    const target = document.createElement("article");
    target.id = "block-7";
    document.body.appendChild(target);
    let scrolled = false;
    target.scrollIntoView = () => {
      scrolled = true;
    };

    render(<AttentionLine count={1} firstBlockId={7} />);
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    expect(scrolled).toBe(true);

    document.body.removeChild(target);
  });
});
