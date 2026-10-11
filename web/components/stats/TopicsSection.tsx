import "@/app/stats/charts-ranked.css";
import type { TopicsReport } from "@/lib/topics_contract";
import { AtlasEmpty } from "./AtlasArt";
import { DayCompass } from "./DayCompass";
import { DayMirror } from "./DayMirror";
import { RefreshSoon } from "./RefreshSoon";
import { TopicGalaxy } from "./TopicGalaxy";
import { TopicHeartbeats } from "./TopicHeartbeats";
import { TopicMoments } from "./TopicMoments";
import { TopicRiver } from "./TopicRiver";

export function TopicsSection({ report }: { report: TopicsReport | null }) {
  if (!report || report.status === "helper_down") {
    return (
      <>
        <AtlasEmpty kind="helper_down" />
        <p className="sr-empty">Topic stats need the local helper (`worklog verdict`).</p>
      </>
    );
  }
  if (report.status === "too_few_blocks") {
    return (
      <>
        <AtlasEmpty kind="too_few" />
        <p className="sr-empty">Not enough described work in this range yet.</p>
      </>
    );
  }
  return (
    <div className="stats-stack">
      {report.naming === "pending" && <RefreshSoon />}
      <TopicGalaxy report={report} />
      <TopicHeartbeats report={report} />
      <TopicRiver report={report} />
      <div className="stats-two">
        <DayMirror report={report} />
        <DayCompass report={report} />
      </div>
      <TopicMoments report={report} />
    </div>
  );
}
