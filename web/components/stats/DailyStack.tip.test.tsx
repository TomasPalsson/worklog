import { afterEach, describe, expect, it } from "bun:test";
import { cleanup, render } from "@testing-library/react";
import { DailyStack, dayTip } from "./DailyStack";
import { day } from "./time-fixtures";

afterEach(cleanup);

describe("dayTip", () => {
  const all = [
    day({ day: "2026-10-05", work_seconds: 3600 }),
    day({ day: "2026-10-06", work_seconds: 31320, personal_seconds: 600, ignored_seconds: 60 }),
    day({ day: "2026-10-07" }),
  ];
  it("over target", () => {
    const t = dayTip(all[1], all);
    expect(t.title).toBe("Tue 6 Oct");
    expect(t.sub).toBe("1st most work of 3 days");
    expect(t.rows).toEqual([["Work", "8h 42m"], ["Personal", "10m"], ["Ignored", "1m"], ["Total", "8h 53m"], ["vs 7.5h target", "+1h 12m over"]]);
    expect(t.note).toBe("Cleared the 7.5h target by 1h 12m.");
    expect(t.bar!.label).toBe("116% of the 7.5h target");
  });
  it("under target uses the average worked day", () => {
    const t = dayTip(all[0], all);
    expect(t.rows![4]).toEqual(["vs 7.5h target", "6h 30m under"]);
    expect(t.note).toBe("0.2× your average worked day.");
  });
  it("empty day", () => {
    const t = dayTip(all[2], all);
    expect(t.sub).toBe("no work, 3 days shown");
    expect(t.note).toBe("Nothing tracked this day.");
  });
});

describe("DailyStack tips", () => {
  it("tips every day, drops native titles, focuses non-empty days only", () => {
    const { container } = render(
      <DailyStack daily={[day({ day: "2026-10-06", work_seconds: 7200 }), day({ day: "2026-10-07" })]} />,
    );
    expect(container.querySelectorAll("title").length).toBe(0);
    const tips = [...container.querySelectorAll("g[data-stip]")];
    expect(tips.length).toBe(2);
    expect(tips[0].getAttribute("aria-label")).toBe("Tue 6 Oct: 2h work");
    expect(tips[0].getAttribute("tabindex")).toBe("0");
    expect(tips[1].getAttribute("tabindex")).toBeNull();
    expect(tips[1].querySelector(".st-hit")).toBeTruthy();
  });
});
