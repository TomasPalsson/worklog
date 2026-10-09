// Three-stage routing pipeline: browser heartbeat -> Slack collect -> model
// helper. Ring colour = freshness of the stage's last timestamp; the
// connector after a stale/never stage is dashed and broken.

import type { ReactNode } from "react";
import { Globe } from "lucide-react";
import type { RoutingStatus } from "@/lib/types";
import { ClaudeMark, SlackMark } from "../SourceIcon";

export type Health = "ok" | "stale" | "bad" | "never";

const MIN = 60_000;
const HOUR = 60 * MIN;

/** Age in ms, clamped at 0 (clock skew); null for missing/unparseable. */
export function ageMs(iso: string | null, now: number): number | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  return Number.isNaN(t) ? null : Math.max(0, now - t);
}

export function freshness(age: number | null): Health {
  if (age === null) return "never";
  if (age < 2 * HOUR) return "ok"; // Slack collects about hourly, so an hour old is normal
  if (age < 24 * HOUR) return "stale";
  return "bad";
}

export function formatAge(age: number | null): string {
  if (age === null) return "never";
  if (age < HOUR) return `${Math.floor(age / MIN)} min`;
  if (age < 24 * HOUR) return `${Math.floor(age / HOUR)} h`;
  return `${Math.floor(age / (24 * HOUR))} d`;
}

const WORDS: Record<Health, string> = { ok: "fresh", stale: "stale", bad: "old", never: "never seen" };

interface Stage {
  name: string;
  health: Health;
  label: string;
  word: string;
  title: string;
  icon: ReactNode;
}

function stages(status: RoutingStatus | null, now: number): Stage[] {
  const timed = (name: string, iso: string | null, icon: ReactNode): Stage => {
    const age = ageMs(iso, now);
    return {
      name,
      health: freshness(age),
      label: formatAge(age),
      word: WORDS[freshness(age)],
      title: iso ? `${name}: ${new Date(iso).toLocaleString()}` : `${name}: never`,
      icon,
    };
  };
  const up = !!status?.classifier_reachable;
  return [
    timed("Browser", status?.last_heartbeat ?? null, <Globe width={16} height={16} strokeWidth={1.6} />),
    timed("Slack", status?.last_slack ?? null, <SlackMark size={16} />),
    {
      name: "Model",
      health: up ? "ok" : "bad",
      label: up ? "up" : "down",
      word: up ? "reachable" : "unreachable",
      title: `Model helper: ${up ? "reachable" : "unreachable"}`,
      icon: <ClaudeMark size={16} />,
    },
  ];
}

export function RoutingPipeline({ status, now = Date.now() }: { status: RoutingStatus | null; now?: number }) {
  const s = stages(status, now);
  const label = s.map((x) => `${x.name} ${x.label} (${x.word})`).join(", then ");
  const parts: ReactNode[] = [];
  s.forEach((x, i) => {
    parts.push(
      <div className="art-settings-node" data-health={x.health} key={x.name} title={x.title}>
        <span className="art-settings-ring">{x.icon}</span>
        <span className="art-label art-settings-age">{x.label}</span>
        <span className="art-settings-name">{x.name}</span>
      </div>,
    );
    if (i < s.length - 1) {
      const broken = x.health !== "ok";
      parts.push(
        <svg className="art-settings-conn" data-broken={broken || undefined} height="4" aria-hidden="true" key={`c${i}`}>
          {broken ? (
            <>
              <line x1="0%" x2="38%" y1="2" y2="2" />
              <line x1="62%" x2="100%" y1="2" y2="2" />
            </>
          ) : (
            <line className="art-settings-flow" x1="0%" x2="100%" y1="2" y2="2" />
          )}
        </svg>,
      );
    }
  });
  return (
    <div className="art-settings-pipe" role="img" aria-label={`Routing pipeline: ${label}`}>
      {parts}
    </div>
  );
}
