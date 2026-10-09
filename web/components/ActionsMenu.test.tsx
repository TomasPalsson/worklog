// The day page's "More" menu: the occasional actions live behind one button so the bar stays one row.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ActionsMenu, type MenuAction } from "./ActionsMenu";

afterEach(cleanup);

function items(over: Partial<Record<string, Partial<MenuAction>>> = {}): MenuAction[] {
  return ["Rebuild blocks", "Estimate with Claude", "Refresh Jira"].map((label) => ({
    label,
    pendingLabel: `${label}…`,
    title: `about ${label}`,
    icon: null,
    pending: false,
    onClick: mock(() => {}),
    ...over[label],
  }));
}

const trigger = () => screen.getByRole("button", { name: "More actions" });

describe("ActionsMenu", () => {
  it("keeps the items hidden until More is clicked (wrong version: always rendered inline)", () => {
    render(<ActionsMenu items={items()} />);
    expect(trigger().getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.queryByRole("menuitem", { name: "Rebuild blocks" })).toBeNull();
  });

  it("opens a menu with one item per action, in order", () => {
    render(<ActionsMenu items={items()} />);
    fireEvent.click(trigger());
    expect(trigger().getAttribute("aria-expanded")).toBe("true");
    const names = screen.getAllByRole("menuitem").map((b) => b.textContent);
    expect(names).toEqual(["Rebuild blocks", "Estimate with Claude", "Refresh Jira"]);
    expect(screen.getByRole("menuitem", { name: "Refresh Jira" }).getAttribute("title")).toBe("about Refresh Jira");
  });

  it("runs the clicked item once and closes (wrong version: runs another item, or stays open)", () => {
    const list = items();
    render(<ActionsMenu items={list} />);
    fireEvent.click(trigger());
    fireEvent.click(screen.getByRole("menuitem", { name: "Estimate with Claude" }));
    expect(list[1].onClick).toHaveBeenCalledTimes(1);
    expect(list[0].onClick).not.toHaveBeenCalled();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("closes on Escape and on an outside click", () => {
    render(
      <div>
        <ActionsMenu items={items()} />
        <p>outside</p>
      </div>,
    );
    fireEvent.click(trigger());
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    fireEvent.click(trigger());
    fireEvent.mouseDown(screen.getByText("outside"));
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("shows a running item's pending label, disabled, and marks More busy", () => {
    render(<ActionsMenu items={items({ "Refresh Jira": { pending: true } })} />);
    expect(trigger().getAttribute("aria-busy")).toBe("true");
    fireEvent.click(trigger());
    const jira = screen.getByRole("menuitem", { name: "Refresh Jira…" }) as HTMLButtonElement;
    expect(jira.disabled).toBe(true);
    expect((screen.getByRole("menuitem", { name: "Rebuild blocks" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("keyboard skips a running item: focus starts on the first free one, arrows step over it", () => {
    render(<ActionsMenu items={items({ "Rebuild blocks": { pending: true } })} />);
    fireEvent.click(trigger());
    const menu = screen.getByRole("menu");
    expect(document.activeElement?.textContent).toBe("Estimate with Claude");
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement?.textContent).toBe("Refresh Jira");
    // wraps past the disabled first item (wrong version: focus() on a disabled button is a silent no-op and sticks)
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement?.textContent).toBe("Estimate with Claude");
  });

  it("is not busy when nothing runs (wrong version: aria-busy always set)", () => {
    render(<ActionsMenu items={items()} />);
    expect(trigger().getAttribute("aria-busy")).toBeNull();
  });
});
