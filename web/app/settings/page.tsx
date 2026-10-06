// Settings page. A page rather than a dialog: it holds six topics and a
// set of service keys, more than a modal can show without cramping.

import Link from "next/link";
import { ChevronLeft } from "lucide-react";

import { SettingsPanel } from "@/components/SettingsPanel";
import { formatDayHeading, todayISO } from "@/lib/format";
import "./settings.css";

export const metadata = {
  title: "Settings · worklog",
};

interface Props {
  // `?from=YYYY-MM-DD`: the day to go back to, and to refresh on save.
  searchParams: Promise<{ from?: string }>;
}

export default async function SettingsPage({ searchParams }: Props) {
  const { from } = await searchParams;
  const back = /^\d{4}-\d{2}-\d{2}$/.test(from ?? "") ? (from as string) : todayISO();

  return (
    <div className="set-page">
      <header className="set-page-header">
        <Link href={`/${back}`} className="reg-back">
          <ChevronLeft size={14} strokeWidth={1.75} />
          {formatDayHeading(back)}
        </Link>
        <h1>Settings</h1>
        <p className="reg-lede">
          How worklog tracks, sorts and cleans up your time. Edits are kept when you press{" "}
          <strong>Save changes</strong> at the bottom; turning Verdict on or off happens at once.
        </p>
      </header>
      <SettingsPanel day={back} />
    </div>
  );
}
