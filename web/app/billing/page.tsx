// Billing registry page.
//
// A page rather than a modal because it holds two editable tables plus the
// unmapped-folder discovery list — more than a dialog can show without
// cramping. It is intentionally NOT day-scoped: customers and folder
// mappings are global, so the export resolves the same way on every day.

import Link from "next/link";
import { ChevronLeft } from "lucide-react";

import { BillingRegistry } from "@/components/BillingRegistry";
import { ThemeToggle } from "@/components/ThemeToggle";
import { formatDayHeading, todayISO } from "@/lib/format";

export const metadata = {
  title: "Billing registry · worklog",
};

interface Props {
  // `?from=YYYY-MM-DD` lets the back link return to the day the user came
  // from instead of always dumping them on today.
  searchParams: Promise<{ from?: string }>;
}

export default async function BillingPage({ searchParams }: Props) {
  const { from } = await searchParams;
  const back = /^\d{4}-\d{2}-\d{2}$/.test(from ?? "") ? (from as string) : todayISO();

  return (
    <main className="reg-page">
      <header className="reg-page-header">
        <div>
          <Link href={`/${back}`} className="reg-back" data-tip="Back to the day view">
            <ChevronLeft size={14} strokeWidth={1.75} />
            {formatDayHeading(back)}
          </Link>
          <h1>Billing registry</h1>
          <p className="reg-lede">
            How the export works out <strong>Viðskiptamaður</strong> and{" "}
            <strong>Verkefni (deild)</strong> for each line.
          </p>
        </div>
        <ThemeToggle />
      </header>

      <BillingRegistry />
    </main>
  );
}
