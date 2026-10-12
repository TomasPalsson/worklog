import Link from "next/link";
import { formatDuration } from "@/lib/format";
import type { LooseEnds } from "@/lib/time_leaks_contract";
import { type CycleDay, type DayWork, summarise, TEMPO_SEND_HOUR } from "@/lib/unbilled";
import { StepBilledIcon, StepLoggedIcon, StepTempoIcon } from "./unbilled-icons";
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

export function Unbilled({ data, today, hourNow }: { data: LooseEnds | null; today: string; hourNow: number }) {
  if (!data) return <DaemonDown />;
  const s = summarise(data, today, hourNow);
  const range = cycleLabel(data.cycle_start, data.cycle_end);
  return (
    <>
      <header className="reg-page-header ub-head">
        <div>
          <h1>Unbilled work</h1>
          <p className="reg-lede">
            A day is finished when its hours are in Tempo and it is marked billed. These days in this billing cycle are
            missing one or both.
          </p>
        </div>
        <p className="ub-cycle">
          <span>Billing cycle</span>
          {range}
        </p>
      </header>

      <Steps
        open={s.days.reduce((a, d) => a + d.seconds, 0)}
        tempo={s.tempo}
        bill={s.bill}
        billReady={s.days.filter((d) => d.tempo.state === "done").length}
      />

      <section className="ub-sec" aria-labelledby="ub-strip-h">
        <h2 id="ub-strip-h">This cycle, day by day</h2>
        <CycleStrip cycle={s.cycle} today={today} />
      </section>

      {s.days.length === 0 ? (
        <div className="ub-state">
          <AllSettledSpot />
          <h2>Nothing left to finish</h2>
          <p>Every day from {range} is in Tempo and marked billed.</p>
        </div>
      ) : (
        <DayLists days={s.days} today={today} />
      )}
    </>
  );
}

/** Two groups, one row per day: what is missing, the open hours, and the page that fixes it. */
function DayLists({ days, today }: { days: DayWork[]; today: string }) {
  const tempo = days.filter((d) => d.tempo.state !== "done");
  const billOnly = days.filter((d) => d.tempo.state === "done");
  return (
    <>
      {tempo.length > 0 && (
        <section className="ub-sec" aria-labelledby="ub-tempo-h">
          <h2 id="ub-tempo-h">
            Not in Tempo yet <span className="ub-count">{tempo.length}</span>
          </h2>
          <p className="ub-sec-lede">
            The {TEMPO_SEND_HOUR}:00 send leaves a ticket out when its Tempo line has no ticket, rounds to zero hours,
            or has no text. On the day page, fill in what is missing, then send the day to Tempo. Once a day is in Tempo
            it moves to the billing list below.
          </p>
          <ol className="ub-rows">
            {tempo.map((d) => (
              <DayRow key={d.day} d={d} today={today} />
            ))}
          </ol>
        </section>
      )}
      {billOnly.length > 0 && (
        <section className="ub-sec" aria-labelledby="ub-bill-h">
          <h2 id="ub-bill-h">
            In Tempo, not marked billed <span className="ub-count">{billOnly.length}</span>
          </h2>
          <p className="ub-sec-lede">
            On the Billing page, check the day’s invoice lines, then press <strong>Mark exported</strong> — that marks
            the day billed. <Link href={`/billing?from=${billOnly[0].day}`}>Start with the oldest</Link>.
          </p>
          <ol className="ub-rows">
            {billOnly.map((d) => (
              <DayRow key={d.day} d={d} today={today} />
            ))}
          </ol>
        </section>
      )}
    </>
  );
}

type StepTotal = { seconds: number; days: number };

function Steps({ open, tempo, bill, billReady }: { open: number; tempo: StepTotal; bill: StepTotal; billReady: number }) {
  const left = (x: StepTotal, verb: string) =>
    x.days === 0 ? `All ${verb}` : `${formatDuration(x.seconds)} not ${verb} · ${x.days} ${x.days === 1 ? "day" : "days"}`;
  return (
    <ol className="ub-steps" aria-label="How a day gets finished">
      <li>
        <StepLoggedIcon size={28} />
        <strong>Logged</strong>
        <span>{open > 0 ? `${formatDuration(open)} still open this cycle` : "Your work, with a ticket on it"}</span>
      </li>
      <li data-open={tempo.days > 0 || undefined}>
        <StepTempoIcon size={28} />
        <strong>Sent to Tempo</strong>
        <span>{left(tempo, "sent")}</span>
      </li>
      <li data-open={bill.days > 0 || undefined}>
        <StepBilledIcon size={28} />
        <strong>Marked billed</strong>
        <span>
          {left(bill, "marked")}
          {bill.days > billReady && ` (${billReady} already in Tempo)`}
        </span>
      </li>
    </ol>
  );
}

const CELL_WORDS: Record<CycleDay["state"], string> = {
  tempo: "not in Tempo yet",
  bill: "in Tempo, not marked billed",
  waiting: "sends to Tempo at 17:00",
  clear: "no open work",
  future: "not yet",
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
            const open = c.state === "tempo" || c.state === "bill" || c.state === "waiting";
            return (
              <li key={c.day} className="ub-cell" data-state={c.state} data-today={c.day === today || undefined}>
                {open ? (
                  <a href={`#day-${c.day}`} aria-label={label} title={label}>
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
        <li data-state="bill">In Tempo, not marked billed</li>
        <li data-state="waiting">Sends at {TEMPO_SEND_HOUR}:00</li>
        <li data-state="clear">No open work (nothing logged, or all done)</li>
      </ul>
    </>
  );
}

function DayRow({ d, today }: { d: DayWork; today: string }) {
  const tickets = d.tempo.tickets;
  const extra = tickets.length - TICKETS_SHOWN;
  const tempoSeconds = tickets.reduce((a, t) => a + t.seconds, 0);
  const state = d.tempo.state === "needs" ? "tempo" : d.tempo.state === "waiting" ? "waiting" : "bill";
  return (
    <li id={`day-${d.day}`} className="ub-row" data-state={state}>
      <span className="ub-swatch" aria-hidden="true" />
      <div className="ub-row-main">
        <h3>
          {shortDay(d.day)}
          {d.day === today && <span className="ub-today">today</span>}
        </h3>
        {d.tempo.state === "waiting" && <p>Sends to Tempo at {TEMPO_SEND_HOUR}:00 today</p>}
        {d.tempo.state === "needs" && (
          <ul className="ub-tickets" aria-label="Not in Tempo">
            {tickets.slice(0, TICKETS_SHOWN).map((t) => (
              <li key={t.key}>
                <span className="ub-key">{t.key}</span> {formatDuration(t.seconds)}
              </li>
            ))}
            {extra > 0 && <li>+{extra} more</li>}
          </ul>
        )}
      </div>
      <span className="ub-row-hours">{formatDuration(d.tempo.state === "done" ? d.bill.seconds : tempoSeconds)}</span>
      <div className="ub-row-actions">
        {d.tempo.state === "needs" && (
          <Link href={`/${d.day}`} className="ub-btn">
            Fix Tempo line
          </Link>
        )}
        {d.tempo.state === "done" && (
          <Link href={`/billing?from=${d.day}`} className="ub-btn">
            Open Billing to mark billed
          </Link>
        )}
      </div>
    </li>
  );
}
