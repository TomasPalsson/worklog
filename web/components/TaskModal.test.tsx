// Ticket dialog: what it shows while Jira loads and once it has, and how it closes.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { actions, calls, detail, dialog, open, quiet, row } from "./taskModalTestKit";

afterEach(cleanup);

describe("TaskModal reading", () => {
  it("is a modal dialog labelled by the title, shows the cached row at once, then the description", async () => {
    const a = actions();
    open(a);
    expect(dialog().getAttribute("aria-modal")).toBe("true");
    const title = screen.getByRole("heading", { name: "Fix login" });
    expect(title.tagName).toBe("H2");
    expect(dialog().getAttribute("aria-labelledby")).toBe(title.id);
    expect(document.querySelector(".task-skel")).toBeTruthy(); // description is loading
    expect(await screen.findByText(/Steps to reproduce/)).not.toBeNull();
    expect(calls(a.loadTicketDetail)[0]).toEqual(["ABC-1"]);
    expect(screen.getByRole("link", { name: /Open in Jira/ }).getAttribute("href")).toBe("https://jira.example/browse/ABC-1");
  });

  it("puts Description before Activity and loads the work log for the key on open", async () => {
    const a = actions();
    open(a);
    expect(await screen.findByText("No work logged on ABC-1 in the last 14 days.")).not.toBeNull();
    expect(calls(a.loadTicketBlocks)[0]).toEqual(["ABC-1"]);
    const labels = [...document.querySelectorAll(".task-modal-main .task-label")].map((n) => n.textContent);
    expect(labels).toEqual(["Description", "Activity"]);
  });

  it("shows the epic above the title when there is one", () => {
    open(actions(), row({ parent_summary: "Login rework" }));
    const parent = document.querySelector(".task-modal-main > .task-parent") as HTMLElement;
    expect(parent.textContent).toBe("Login rework");
    expect(parent.nextElementSibling?.tagName).toBe("H2");
  });

  it("shows empty-description copy", async () => {
    open(actions({ loadTicketDetail: mock(async () => ({ ok: true as const, data: detail({ description: "", comments: [] }) })) }));
    expect(await screen.findByText("No description in Jira.")).not.toBeNull();
  });

  it("shows a load error and Try again re-calls the loader", async () => {
    let n = 0;
    const loader = mock(async () =>
      ++n === 1 ? { ok: false as const, error: "jira down" } : { ok: true as const, data: detail() },
    );
    open(actions({ loadTicketDetail: loader }));
    expect((await screen.findByText("Couldn't load ABC-1 from Jira: jira down")).textContent).toBe("Couldn't load ABC-1 from Jira: jira down");
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText(/Steps to reproduce/)).not.toBeNull();
    expect(loader.mock.calls.length).toBe(2);
  });

  it("strips a trailing period from a load error reason", async () => {
    open(actions({ loadTicketDetail: mock(async () => ({ ok: false as const, error: "jira down." })) }));
    expect((await screen.findByText(/Couldn't load ABC-1/)).textContent).toBe("Couldn't load ABC-1 from Jira: jira down");
  });

  it("shows the live Jira status once detail has loaded", async () => {
    open(actions({ loadTicketDetail: mock(async () => ({ ok: true as const, data: detail({ status: "In Review", status_category: "indeterminate" }) })) }));
    expect(screen.getByTestId("status-ABC-1").textContent).toBe("To Do");
    await waitFor(() => expect(screen.getByTestId("status-ABC-1").textContent).toBe("In Review"));
    expect(screen.getByTestId("status-ABC-1").getAttribute("data-category")).toBe("indeterminate");
  });
});

describe("TaskModal description clamp", () => {
  const measure = (scroll: number, client: number) => {
    Object.defineProperty(HTMLElement.prototype, "scrollHeight", { configurable: true, get: () => scroll });
    Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => client });
  };
  afterEach(() => {
    delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollHeight;
    delete (HTMLElement.prototype as unknown as Record<string, unknown>).clientHeight;
  });

  it("fades a tall description under Show more and folds it back with Show less", async () => {
    measure(400, 270);
    open(actions());
    const more = await screen.findByRole("button", { name: "Show more" });
    const prose = document.querySelector(".task-prose") as HTMLElement;
    expect(prose.hasAttribute("data-clamped")).toBe(true);
    expect(more.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(more);
    expect(prose.hasAttribute("data-clamped")).toBe(false);
    expect(screen.getByRole("button", { name: "Show less" }).getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "Show less" }));
    expect(prose.hasAttribute("data-clamped")).toBe(true);
  });

  it("offers no toggle when the text fits", async () => {
    measure(100, 100);
    open(actions());
    await screen.findByText(/Steps to reproduce/);
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByRole("button", { name: /Show (more|less)/ })).toBeNull();
  });
});

describe("TaskModal closing", () => {
  it("moves focus to the dialog on open and Esc or the close button calls onClose", async () => {
    const { onClose } = open(actions());
    expect(document.activeElement).toBe(dialog());
    expect(dialog().getAttribute("tabindex")).toBe("-1");
    expect(screen.getByRole("heading", { name: "Fix login" }).hasAttribute("tabindex")).toBe(false);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose.mock.calls.length).toBe(1);
    fireEvent.click(screen.getByRole("button", { name: "Close ABC-1" }));
    expect(onClose.mock.calls.length).toBe(2);
    await screen.findByText(/Steps to reproduce/);
  });

  it("a click on the backdrop closes; a click inside the dialog does not", async () => {
    const { onClose } = open(actions());
    const backdrop = dialog().parentElement as HTMLElement;
    fireEvent.mouseDown(dialog());
    fireEvent.click(dialog());
    expect(onClose.mock.calls.length).toBe(0);
    fireEvent.mouseDown(backdrop);
    fireEvent.click(backdrop);
    expect(onClose.mock.calls.length).toBe(1);
    await screen.findByText(/Steps to reproduce/);
  });

  it("a drag that starts inside the dialog and ends on the backdrop does not close", async () => {
    const { onClose } = open(actions());
    fireEvent.mouseDown(screen.getByRole("heading", { name: "Fix login" }));
    fireEvent.click(dialog().parentElement as HTMLElement);
    expect(onClose.mock.calls.length).toBe(0);
    await screen.findByText(/Steps to reproduce/);
  });

  it("locks page scroll while open and restores it on close", async () => {
    document.documentElement.style.overflow = "scroll";
    open(actions());
    expect(document.documentElement.style.overflow).toBe("hidden");
    cleanup();
    expect(document.documentElement.style.overflow).toBe("scroll");
    document.documentElement.style.overflow = "";
  });

  it("Esc closes only when no inner layer handled it", async () => {
    const { onClose } = open(actions(), quiet());
    fireEvent.click(screen.getByTestId("status-ABC-1"));
    await screen.findByRole("menuitem", { name: "Start → In Progress" });
    fireEvent.keyDown(document, { key: "Escape" }); // the menu takes it
    expect(onClose.mock.calls.length).toBe(0);
    expect(screen.queryByRole("menu")).toBeNull();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose.mock.calls.length).toBe(1);
  });

  it("Esc inside the log form closes the form, not the dialog", async () => {
    const { onClose } = open(actions());
    await screen.findByText("No work logged on ABC-1 in the last 14 days.");
    fireEvent.click(screen.getAllByRole("button", { name: "Log time" })[0]);
    expect(fireEvent.keyDown(screen.getByLabelText("What you did"), { key: "Escape" })).toBe(false);
    expect(screen.queryByLabelText("What you did")).toBeNull();
    expect(onClose.mock.calls.length).toBe(0);
  });
});

describe("TaskModal header", () => {
  it("shows the type, project and key, and copies the key", async () => {
    const writeText = mock(async (_t: string) => {});
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    open(actions());
    const crumb = document.querySelector(".task-crumb") as HTMLElement;
    expect(crumb.querySelector(".task-crumb-project")?.textContent).toBe("ABC");
    expect(crumb.querySelector(".task-key")?.textContent).toBe("ABC-1");
    const copy = screen.getByRole("button", { name: "Copy ABC-1" });
    expect(copy.getAttribute("data-tip")).toBe("Copy key");
    fireEvent.click(copy);
    await waitFor(() => expect(writeText.mock.calls[0]).toEqual(["ABC-1"]));
    const region = await screen.findByText("Copied ABC-1.");
    expect(region.className).toBe("task-sr");
    expect(region.getAttribute("aria-live")).toBe("polite");
    expect(dialog().contains(region)).toBe(true);
    await screen.findByText(/Steps to reproduce/);
  });

  it("copes with no clipboard at all", async () => {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: undefined });
    open(actions());
    fireEvent.click(screen.getByRole("button", { name: "Copy ABC-1" }));
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.queryByText("Copied ABC-1.")).toBeNull();
    await screen.findByText(/Steps to reproduce/);
  });
});
