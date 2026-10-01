// My Tasks: the Owner's cached Jira tickets with this-week and today hours.
// Reads the daemon's local cache only — no Jira call on load.

import Link from "next/link";
import { ChevronLeft } from "lucide-react";

import { TaskBoard } from "@/components/TaskBoard";
import { ThemeToggle } from "@/components/ThemeToggle";
import { tasks } from "@/lib/daemonHub";
import { formatDayHeading, todayISO } from "@/lib/format";

export const metadata = {
  title: "My Tasks · worklog",
};

export const dynamic = "force-dynamic";

export default async function TasksPage() {
  const { tasks: rows } = await tasks();
  const today = todayISO();

  return (
    <main className="reg-page">
      <header className="reg-page-header">
        <div>
          <Link href={`/${today}`} className="reg-back" data-tip="Back to the day view">
            <ChevronLeft size={14} strokeWidth={1.75} />
            {formatDayHeading(today)}
          </Link>
          <h1>My Tasks</h1>
        </div>
        <ThemeToggle />
      </header>

      <TaskBoard tasks={rows} />
    </main>
  );
}
