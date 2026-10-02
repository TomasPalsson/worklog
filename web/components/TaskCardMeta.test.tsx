// Card detail pieces: type icon, priority glyph, due chip, labels, parent, week spark.

import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import type { TaskRow } from "@/lib/types";
import { TaskCard } from "./TaskCard";
import { DueChip, Labels, ParentRow, PriorityGlyph, TypeIcon, UpdatedAgo, WeekSpark } from "./TaskCardMeta";

afterEach(cleanup);

describe("TypeIcon", () => {
  it.each([
    ["Bug", "bug"],
    ["story", "story"],
    ["TASK", "task"],
    ["Epic", "epic"],
    ["Sub-task", "subtask"],
    ["Subtask", "subtask"],
    ["Spike", "other"],
  ])("%s -> %s", (name, kind) => {
    const { container } = render(<TypeIcon type={name} />);
    expect(container.querySelector(`[data-type="${kind}"]`)).not.toBeNull();
    expect(container.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
    expect(container.querySelector(".task-sr")?.textContent).toBe(name);
  });
});

describe("PriorityGlyph", () => {
  it("shows a glyph with sr text and title", () => {
    const { container } = render(<PriorityGlyph priority="Highest" />);
    expect(container.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
    expect(screen.getByText("Priority Highest").className).toBe("task-sr");
    expect(container.querySelector("[title='Priority Highest']")).not.toBeNull();
  });

  it("renders nothing for an unknown priority", () => {
    const { container } = render(<PriorityGlyph priority="Whenever" />);
    expect(container.innerHTML).toBe("");
  });
});

describe("DueChip", () => {
  const state = (c: HTMLElement) => c.querySelector(".task-due")?.getAttribute("data-state");
  it("overdue", () => {
    const { container } = render(<DueChip due="2026-10-01" today="2026-10-03" />);
    expect(screen.getByText("Overdue 2d")).toBeTruthy();
    expect(state(container)).toBe("overdue");
  });
  it("today", () => {
    const { container } = render(<DueChip due="2026-10-03" today="2026-10-03" />);
    expect(screen.getByText("Due today")).toBeTruthy();
    expect(state(container)).toBe("today");
  });
  it("soon", () => {
    const { container } = render(<DueChip due="2026-10-05" today="2026-10-03" />);
    expect(screen.getByText("Due 5 Oct")).toBeTruthy();
    expect(state(container)).toBe("soon");
  });
  it("later", () => {
    const { container } = render(<DueChip due="2026-10-20" today="2026-10-03" />);
    expect(screen.getByText("Due 20 Oct")).toBeTruthy();
    expect(state(container)).toBe("later");
  });
  it("is neutral in the Done column", () => {
    const { container } = render(<DueChip due="2026-10-01" today="2026-10-03" done />);
    expect(screen.getByText("Overdue 2d")).toBeTruthy();
    expect(state(container)).toBe("later");
  });
});

describe("Labels / ParentRow / UpdatedAgo", () => {
  it("caps labels at 3 with +n", () => {
    render(<Labels labels={["a", "b", "c", "d", "e"]} />);
    expect(screen.getByText("a")).toBeTruthy();
    expect(screen.getByText("c")).toBeTruthy();
    expect(screen.queryByText("d")).toBeNull();
    expect(screen.getByText("+2")).toBeTruthy();
  });
  it("no +n at exactly 3, nothing when empty", () => {
    const { container } = render(<Labels labels={["a", "b", "c"]} />);
    expect(screen.queryByText(/^\+/)).toBeNull();
    cleanup();
    expect(render(<Labels labels={[]} />).container.innerHTML).toBe("");
    expect(container).toBeTruthy();
  });
  it("parent row carries the full text as title", () => {
    render(<ParentRow text="Checkout revamp" />);
    expect(screen.getByText("Checkout revamp").closest("[title]")?.getAttribute("title")).toBe("Checkout revamp");
  });
  it("updated ago", () => {
    render(<UpdatedAgo iso="2026-09-30T12:00:00Z" now={new Date("2026-10-02T12:00:00Z")} />);
    expect(screen.getByText("Updated 2d ago")).toBeTruthy();
  });
});

describe("WeekSpark", () => {
  const days = [3600, 0, 0, 1800, 0, 0, 0];
  it("labels the week and marks today", () => {
    const { container } = render(<WeekSpark daySeconds={days} max={3600} column="new" today="2026-10-01" />);
    const img = screen.getByRole("img");
    expect(img.getAttribute("aria-label")).toBe("This week: Mon 1h, Tue 0m, Wed 0m, Thu 30m, Fri 0m, Sat 0m, Sun 0m");
    const bars = container.querySelectorAll("i");
    expect(bars.length).toBe(7);
    expect(bars[3].hasAttribute("data-today")).toBe(true); // 2026-10-01 is a Thursday
    expect(container.querySelectorAll("[data-today]").length).toBe(1);
    expect((bars[0] as HTMLElement).style.height).toBe("16px");
    expect((bars[3] as HTMLElement).style.height).toBe("8px");
    expect(bars[1].hasAttribute("data-zero")).toBe(true);
  });
});

const row = (over: Partial<TaskRow> = {}): TaskRow => ({
  key: "ABC-1",
  summary: "Fix login",
  status: "To Do",
  status_category: "new",
  url: null,
  assigned: true,
  week_seconds: 5400,
  today_seconds: 1800,
  last_worked_day: null,
  issue_type: null,
  priority: null,
  due_date: null,
  labels: [],
  parent_summary: null,
  updated: null,
  day_seconds: [0, 0, 0, 0, 0, 0, 0],
  ...over,
});

const card = (task: TaskRow) =>
  render(
    <ul>
      <TaskCard
        task={task}
        column="new"
        today="2026-10-03"
        selected={false}
        pending={false}
        dragging={false}
        landed={false}
        error={undefined}
        onRetry={() => {}}
        onOpen={() => {}}
        loadTransitions={async () => ({ ok: true, data: [] })}
        onMove={() => {}}
        onDismissError={() => {}}
        onDragStart={() => {}}
        onDragEnd={() => {}}
      />
    </ul>,
  );

describe("TaskCard with rich data", () => {
  it("omits every optional row when the data is missing but keeps key, summary, hours", () => {
    const { container } = card(row());
    expect(screen.getByText("ABC-1")).toBeTruthy();
    expect(screen.getByText("Fix login")).toBeTruthy();
    expect(screen.getByText("1h 30m this week · 30m today")).toBeTruthy();
    for (const sel of ["[data-type]", ".task-due", ".task-parent", ".task-labels", ".task-updated"]) {
      expect(container.querySelector(sel)).toBeNull();
    }
  });

  it("shows every row when the data is present", () => {
    const { container } = card(
      row({
        issue_type: "Bug",
        priority: "High",
        due_date: "2026-10-04",
        labels: ["web"],
        parent_summary: "Checkout revamp",
        updated: new Date(Date.now() - 3 * 3600_000).toISOString(),
        day_seconds: [0, 0, 0, 0, 3600, 0, 0],
      }),
    );
    for (const sel of ["[data-type='bug']", ".task-due", ".task-parent", ".task-labels", ".task-updated"]) {
      expect(container.querySelector(sel)).not.toBeNull();
    }
    expect(screen.getByText("Updated 3h ago")).toBeTruthy();
    expect(screen.getByRole("img").getAttribute("aria-label")).toContain("Fri 1h");
  });
});
