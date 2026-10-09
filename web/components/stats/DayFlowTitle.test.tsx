import { afterEach, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { DayFlow, type FlowBlock } from "../art/DayFlow";

afterEach(cleanup);

const blocks: FlowBlock[] = [{ seconds: 3600, kind: "work", ticket: "A-1", sources: [{ source: "claude_turn", n: 1 }] }];

it("title replaces the summary text; the default is unchanged", () => {
  const a = render(<DayFlow blocks={blocks} billing={null} title={<>Where <b>42h</b> went</>} />);
  expect(a.container.querySelector("summary")?.textContent).toContain("Where 42h went");
  expect(a.container.querySelector("summary")?.textContent).not.toContain("day");
  a.unmount();
  const b = render(<DayFlow blocks={blocks} billing={null} />);
  expect(b.container.querySelector("summary")?.textContent).toContain("Where the day");
});
