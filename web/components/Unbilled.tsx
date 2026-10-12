import Link from "next/link";
import type { ReactNode } from "react";
import { formatDuration } from "@/lib/format";
import { relativeWords } from "@/lib/taskBoard";
import type { UnbilledDays } from "@/lib/time_leaks_contract";
import { type CycleDay, type DayWork, summarise, TEMPO_SEND_HOUR, type TicketGap } from "@/lib/unbilled";
import { StepLoggedIcon, StepTempoIcon, UnbilledIcon } from "./unbilled-icons";
import { AllSettledSpot, DaemonDownSpot } from "./unbilled-spots";

const TICKETS_SHOWN = 4;

/** "Mon 5 Oct". */
export function shortDay(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  return new Intl.DateTimeFormat("en-GB", { weekday: "short", day: "numeric", month: "short", timeZone: "UTC" }).format(
    new Date(Date.UTC(y, m - 1, d)),
  );
}

/** "20 Sep – 19 Oct". */
function cycleLabel(start: string, end: string): string {
  const f = (day: string) => shortDay(day).split(" ").slice(1).join(" ");
  return `${f(start)} – ${f(end)}`;
}

function DaemonDown() {
  return (
    <div className="ub-state ub-state--error" role="alert">
      <DaemonDownSpot />
      <h2>Couldn’t reach the worklog daemon</h2>
      <p>
        This list comes from the daemon, and it didn’t answer. Start it with <code>worklog daemon install</code>, then
        reload.
      </p>
      <Link className="ub-btn" href="/unbilled" prefetch={false}>
        Reload
      </Link>
    </div>
  );
}

type Props = { data: UnbilledDays | null; today: string; hourNow: number; now?: Date };

export function Unbilled({ data, today, hourNow, now = new Date() }: Props) {
  if (!data) return <DaemonDown />;
  const s = summarise(data, today, hourNow);
  const range = cycleLabel(data.cycle_start, data.cycle_end);
  return (
    <>
      <header className="reg-page-header ub-head">
        <div>
          <h1>Unbilled work</h1>
          <p className="reg-lede">
            A day is finished when all its work has a ticket and is in Tempo. These days in this billing cycle are not
            there yet.
          </p>
        </div>
        <div className="ub-cycle">
          <p>
            <span>Billing cycle</span>
            {range}
          </p>
          <TempoChecked at={data.tempo_pulled_at} now={now} />
        </div>
      </header>

      <Steps open={s.tempo.seconds + s.ticket.seconds} tempo={s.tempo} ticket={s.ticket} />

      <section className="ub-sec" aria-labelledby="ub-strip-h">
        <h2 id="ub-strip-h">This cycle, day by day</h2>
        <CycleStrip cycle={s.cycle} today={today} />
      </section>

      {s.days.length === 0 ? (
        <div className="ub-state">
          <AllSettledSpot />
          <h2>Nothing left to finish</h2>
          <p>Every day from {range} has a ticket on its work, is in Tempo and is exported.</p>
        </div>
      ) : (
        <DayLists days={s.days} today={today} />
      )}
    </>
  );
}

const DAY_MS = 86_400_000;

/** When Tempo last answered; never, or over a day ago, says so and points at the pull. */
function TempoChecked({ at, now }: { at: string | null; now: Date }) {
  const stale = !at || now.getTime() - new Date(at).getTime() > DAY_MS;
  return (
    <p className="ub-pulled" data-stale={stale || undefined}>
      {at ? (
        <time dateTime={at} title={new Date(at).toLocaleString("en-GB")}>
          Tempo checked {relativeWords(at, now)}
        </time>
      ) : (
        "Tempo never checked"
      )}
      {stale && (
        <Link href="/logged" prefetch={false}>
          Pull Tempo on the Logged page
        </Link>
      )}
    </p>
  );
}

/** Three groups, one row per day each: Tempo gaps, ticketless work, then (collapsed) export. */
function DayLists({ days, today }: { days: DayWork[]; today: string }) {
  const tempo = days.filter((d) => d.tempo.state !== "done");
  const ticket = days.filter((d) => d.ticketless > 0);
  const bill = days.filter((d) => d.bill > 0);
  return (
    <>
      {tempo.length > 0 && (
        <section className="ub-sec" aria-labelledby="ub-tempo-h">
          <h2 id="ub-tempo-h">
            Not in Tempo yet <span className="ub-count">{tempo.length}</span>
          </h2>
          <p className="ub-sec-lede">
            The {TEMPO_SEND_HOUR}:00 send leaves a ticket out when its Tempo line has no ticket, rounds to zero hours,
            or has no text; and a line can be short in Tempo, or deleted there. On the day page, fix the line, then send
            the day to Tempo.
          </p>
          <ol className="ub-rows">
            {tempo.map((d) => (
              <TempoRow key={d.day} d={d} today={today} />
            ))}
          </ol>
        </section>
      )}
      {ticket.length > 0 && (
        <section className="ub-sec" aria-labelledby="ub-ticket-h">
          <h2 id="ub-ticket-h">
            No ticket yet <span className="ub-count">{ticket.length}</span>
          </h2>
          <p className="ub-sec-lede">Work without a ticket never reaches Tempo. On the day page, put a ticket on it.</p>
          <ol className="ub-rows">
            {ticket.map((d) => (
              <PlainRow key={d.day} id={`ticket-${d.day}`} state="ticket" day={d.day} today={today} seconds={d.ticketless}>
                <Link href={`/${d.day}`} className="ub-btn">
                  Add ticket
                </Link>
              </PlainRow>
            ))}
          </ol>
        </section>
      )}
      {bill.length > 0 && <ExportList days={bill} today={today} />}
    </>
  );
}

function ExportList({ days, today }: { days: DayWork[]; today: string }) {
  const total = days.reduce((a, d) => a + d.bill, 0);
  return (
    <section className="ub-sec" aria-labelledby="ub-bill-h">
      <h2 id="ub-bill-h">
        Not exported to the invoice form <span className="ub-count">{days.length}</span>
      </h2>
      <details className="ub-details">
        <summary>
          {formatDuration(total)} over {days.length} {days.length === 1 ? "day" : "days"}
        </summary>
        <p className="ub-sec-lede">
          Only matters if you invoice through the Billing page export. Press <strong>Mark exported</strong> there after
          checking the lines.
        </p>
        <ol className="ub-rows">
          {days.map((d) => (
            <PlainRow key={d.day} id={`bill-${d.day}`} state="bill" day={d.day} today={today} seconds={d.bill}>
              <Link href={`/billing?from=${d.day}`} className="ub-btn">
                Open Billing
              </Link>
            </PlainRow>
          ))}
        </ol>
      </details>
    </section>
  );
}

type StepTotal = { seconds: number; days: number };

function Steps({ open, tempo, ticket }: { open: number; tempo: StepTotal; ticket: StepTotal }) {
  const left = (x: StepTotal, words: string, done: string) =>
    x.days === 0 ? done : `${formatDuration(x.seconds)} ${words} · ${x.days} ${x.days === 1 ? "day" : "days"}`;
  return (
    <ol className="ub-steps" aria-label="How a day gets finished">
      <li>
        <StepLoggedIcon size={28} />
        <strong>Logged</strong>
        <span>{open > 0 ? `${formatDuration(open)} still open this cycle` : "Nothing open this cycle"}</span>
      </li>
      <li data-open={tempo.days > 0 || undefined}>
        <StepTempoIcon size={28} />
        <strong>In Tempo</strong>
        <span>{left(tempo, "not in Tempo", "All in Tempo")}</span>
      </li>
      <li data-open={ticket.days > 0 || undefined}>
        <UnbilledIcon size={28} />
        <strong>Has a ticket</strong>
        <span>{left(ticket, "without a ticket", "All work has a ticket")}</span>
      </li>
    </ol>
  );
}

const CELL_WORDS: Record<CycleDay["state"], string> = {
  tempo: "not in Tempo yet",
  ticket: "no ticket yet",
  bill: "not exported to the invoice form",
  waiting: "sends to Tempo at 17:00",
  clear: "no open work",
  future: "not yet",
};

/** Which section's row a strip cell jumps to. */
const CELL_TARGET: Partial<Record<CycleDay["state"], string>> = {
  tempo: "day",
  waiting: "day",
  ticket: "ticket",
  bill: "bill",
};

function CycleStrip({ cycle, today }: { cycle: CycleDay[]; today: string }) {
  return (
    <>
      <div className="ub-strip-scroll">
        <ol className="ub-strip">
          {cycle.map((c) => {
            const label = `${shortDay(c.day)}: ${CELL_WORDS[c.state]}`;
            const body = (
              <>
                <span className="ub-cell-wd">{shortDay(c.day).slice(0, 2)}</span>
                <span className="ub-cell-n">{Number(c.day.slice(8))}</span>
              </>
            );
            const target = CELL_TARGET[c.state];
            return (
              <li key={c.day} className="ub-cell" data-state={c.state} data-today={c.day === today || undefined}>
                {target ? (
                  <a href={`#${target}-${c.day}`} aria-label={label} title={label}>
                    {body}
                  </a>
                ) : (
                  <span aria-label={label} title={label}>
                    {body}
                  </span>
                )}
              </li>
            );
          })}
        </ol>
      </div>
      <p className="ub-strip-cap">Click or tap a coloured day to jump to it below.</p>
      <ul className="ub-legend" aria-label="Legend">
        <li data-state="tempo">Not in Tempo yet</li>
        <li data-state="ticket">No ticket yet</li>
        <li data-state="bill">Not exported to the invoice form</li>
        <li data-state="waiting">Sends at {TEMPO_SEND_HOUR}:00</li>
        <li data-state="clear">No open work (nothing logged, or all done)</li>
      </ul>
    </>
  );
}

function DayHeading({ day, today }: { day: string; today: string }) {
  return (
    <h3>
      {shortDay(day)}
      {day === today && <span className="ub-today">today</span>}
    </h3>
  );
}

function reasonWords(t: TicketGap): string {
  if (t.reason === "short") return `only ${formatDuration(t.inTempo)} in Tempo`;
  return t.reason === "deleted" ? "deleted in Tempo" : "not sent";
}

function TempoRow({ d, today }: { d: DayWork; today: string }) {
  const tickets = d.tempo.tickets;
  const extra = tickets.length - TICKETS_SHOWN;
  const needs = d.tempo.state === "needs";
  return (
    <li id={`day-${d.day}`} className="ub-row" data-state={needs ? "tempo" : "waiting"}>
      <span className="ub-swatch" aria-hidden="true" />
      <div className="ub-row-main">
        <DayHeading day={d.day} today={today} />
        {!needs && <p>Sends to Tempo at {TEMPO_SEND_HOUR}:00 today</p>}
        {needs && (
          <ul className="ub-tickets" aria-label="Not in Tempo">
            {tickets.slice(0, TICKETS_SHOWN).map((t) => (
              <li key={t.key}>
                <span className="ub-key">{t.key}</span> {formatDuration(t.missing)} · {reasonWords(t)}
              </li>
            ))}
            {extra > 0 && <li>+{extra} more</li>}
          </ul>
        )}
      </div>
      <span className="ub-row-hours">{formatDuration(d.tempo.seconds)}</span>
      <div className="ub-row-actions">
        {needs && (
          <Link href={`/${d.day}`} className="ub-btn">
            Fix Tempo line
          </Link>
        )}
      </div>
    </li>
  );
}

type PlainRowProps = { id: string; state: string; day: string; today: string; seconds: number; children: ReactNode };

function PlainRow({ id, state, day, today, seconds, children }: PlainRowProps) {
  return (
    <li id={id} className="ub-row" data-state={state}>
      <span className="ub-swatch" aria-hidden="true" />
      <div className="ub-row-main">
        <DayHeading day={day} today={today} />
      </div>
      <span className="ub-row-hours">{formatDuration(seconds)}</span>
      <div className="ub-row-actions">{children}</div>
    </li>
  );
}
