// My Tasks: the Owner's cached Jira tickets with this-week and today hours.
// Reads the daemon's local cache only — no Jira call on load.

import Link from "next/link";
import { ChevronLeft } from "lucide-react";

import { TaskBoard } from "@/components/TaskBoard";
import { ThemeToggle } from "@/components/ThemeToggle";
import { tasks } from "@/lib/daemonHub";
import { formatDayHeading, formatDuration, todayISO } from "@/lib/format";
import { ticketCount, weekTotal } from "@/lib/taskBoard";

export const metadata = {
  title: "My Tasks · worklog",
};

export const dynamic = "force-dynamic";

const fetchedAt = (iso: string) =>
  new Date(iso).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });

export default async function TasksPage() {
  const { tasks: rows, last_fetched } = await tasks();
  const today = todayISO();
  const parts = [ticketCount(rows.length), `${formatDuration(weekTotal(rows))} logged this week`];
  if (last_fetched) parts.push(`Jira cache from ${fetchedAt(last_fetched)}`);
  const lede = parts.join(" · ");

  return (
    <div className="reg-page tasks-page">
      <header className="reg-page-header">
        <div>
          <Link href={`/${today}`} className="reg-back" data-tip="Back to the day view">
            <ChevronLeft size={14} strokeWidth={1.75} />
            {formatDayHeading(today)}
          </Link>
          <h1>My Tasks</h1>
          <p className="tasks-lede">{lede}</p>
        </div>
        <ThemeToggle />
      </header>

      <TaskBoard tasks={rows} />
    </div>
  );
}
