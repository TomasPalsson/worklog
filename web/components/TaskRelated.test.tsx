// Related issues and attachments section.

import { afterEach, describe, expect, it, mock } from "bun:test";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { Attachment, IssueRef } from "@/lib/types";
import { TaskRelated, fileSize } from "./TaskRelated";
import { detail } from "./taskModalTestKit";

afterEach(cleanup);

const ref = (key: string, over: Partial<IssueRef> = {}): IssueRef => ({
  key,
  summary: `Summary ${key}`,
  status: "To Do",
  status_category: "new",
  issue_type: "Task",
  ...over,
});
const file = (n: number): Attachment => ({ filename: `f${n}.png`, size_bytes: 1_258_291, url: `https://jira.example/c/${n}`, created: "2026-09-05T08:07:00", author: "Ada" });

describe("TaskRelated", () => {
  it("renders nothing while loading and when there is nothing related", () => {
    const a = render(<TaskRelated detail={null} />);
    expect(a.container.innerHTML).toBe("");
    const b = render(<TaskRelated detail={detail()} />);
    expect(b.container.innerHTML).toBe("");
  });

  it("shows the parent, a done count for subtasks, and links grouped by relation", () => {
    const d = detail({
      parent: ref("GENAI-1"),
      subtasks: [ref("ABC-2", { status: "Done", status_category: "done" }), ref("ABC-3")],
      links: [
        { relation: "blocks", issue: ref("ABC-4") },
        { relation: "is blocked by", issue: ref("ABC-5") },
        { relation: "blocks", issue: ref("ABC-6") },
      ],
    });
    render(<TaskRelated detail={d} />);
    expect(screen.getByRole("heading", { name: "Related" })).toBeTruthy();
    expect(screen.getAllByRole("heading", { level: 4 }).map((h) => h.textContent)).toEqual(["Parent", "Subtasks 1 of 2 done", "blocks", "is blocked by"]);
    expect(screen.getByText("Summary GENAI-1")).toBeTruthy();
    expect(screen.getByText("Done").getAttribute("data-category")).toBe("done");
  });

  it("opens off-board issues in Jira and board issues in the modal", () => {
    const onOpen = mock((_k: string) => {});
    const d = detail({ subtasks: [ref("ABC-2"), ref("ABC-3")] });
    render(<TaskRelated detail={d} knownKeys={new Set(["ABC-2"])} onOpen={onOpen} />);
    const jira = screen.getByRole("link", { name: /ABC-3/ });
    expect(jira.getAttribute("href")).toBe("https://jira.example/browse/ABC-3");
    expect(jira.getAttribute("target")).toBe("_blank");
    expect(jira.textContent).toContain("Opens in Jira");
    fireEvent.click(screen.getByRole("button", { name: /ABC-2/ }));
    expect(onOpen).toHaveBeenCalledWith("ABC-2");
  });

  it("caps long lists at five until Show all", () => {
    const d = detail({ attachments: [1, 2, 3, 4, 5, 6, 7, 8, 9].map(file) });
    render(<TaskRelated detail={d} />);
    expect(screen.getAllByRole("link")).toHaveLength(5);
    fireEvent.click(screen.getByRole("button", { name: "Show all 9 attachments" }));
    expect(screen.getAllByRole("link")).toHaveLength(9);
    expect(screen.queryByRole("button", { name: /Show all/ })).toBeNull();
  });

  it("lists attachments with size, author and a new-tab link", () => {
    render(<TaskRelated detail={detail({ attachments: [file(1)] })} />);
    const link = screen.getByRole("link", { name: /f1\.png/ });
    expect(link.getAttribute("href")).toBe("https://jira.example/c/1");
    expect(link.getAttribute("target")).toBe("_blank");
    expect(link.textContent).toContain("1.2 MB · Ada · 5 Sep, 08:07");
  });

  it("formats sizes", () => {
    expect([fileSize(500), fileSize(2048), fileSize(5 * 1024 * 1024)]).toEqual(["500 B", "2.0 KB", "5.0 MB"]);
  });
});
