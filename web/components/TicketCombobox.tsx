"use client";

import { useEffect, useMemo, useRef, useState, useTransition } from "react";
import { AlertCircle, ChevronDown, Search, Ticket, X } from "lucide-react";
import type { JiraTicket } from "@/lib/types";
import { assignTicket } from "@/app/actions";
import { toast } from "@/lib/toast";

interface Props {
  blockId: number;
  current: string | null;
  tickets: JiraTicket[];
  day: string;
}

export function TicketCombobox({ blockId, current, tickets, day }: Props) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIdx, setActiveIdx] = useState(0);
  const [isPending, startTransition] = useTransition();
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const currentTicket = useMemo(
    () => tickets.find((t) => t.key === current),
    [tickets, current],
  );
  // "Assigned but not in cache" — the ticket key exists on the block but
  // we have no summary to show. Only flag it as *stale* when the cache
  // has entries but this specific key is missing; an empty cache means
  // the user hasn't run Refresh Jira yet, which is a different UX story
  // (handled by the empty-state in the dropdown itself).
  const currentIsStale = !!current && !currentTicket && tickets.length > 0;

  const matches = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return tickets.slice(0, 40);
    return tickets
      .filter(
        (t) =>
          t.key.toLowerCase().includes(q) ||
          (t.summary ?? "").toLowerCase().includes(q),
      )
      .slice(0, 60);
  }, [tickets, query]);

  // Reset active index when the filtered set changes.
  useEffect(() => {
    setActiveIdx(0);
  }, [query, open]);

  // Scroll the active option into view on keyboard navigation.
  useEffect(() => {
    if (!open) return;
    const el = listRef.current?.querySelector<HTMLElement>(
      `[data-idx="${activeIdx}"]`,
    );
    el?.scrollIntoView({ block: "nearest" });
  }, [activeIdx, open]);

  useEffect(() => {
    if (!open) return;
    inputRef.current?.focus();
    const onClick = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) {
        // Just close — do NOT steal focus back to the trigger. The user
        // clicked elsewhere, meaning they intend to focus that other
        // element; calling triggerRef.focus() here would race with the
        // browser's native focus handling on the clicked target and win
        // (mousedown fires before focus), undoing the user's intent.
        // Focus restoration belongs in the explicit-close paths
        // (Escape / Enter-to-select), which use closeAndRestoreFocus.
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onClick);
    return () => {
      document.removeEventListener("mousedown", onClick);
    };
  }, [open]);

  const closeAndRestoreFocus = () => {
    setOpen(false);
    setQuery("");
    // Defer so React processes the popover unmount first.
    queueMicrotask(() => triggerRef.current?.focus());
  };

  const pick = (key: string | null) => {
    closeAndRestoreFocus();
    startTransition(async () => {
      const res = await assignTicket(blockId, key, day);
      if (!res.ok) toast.error(`Assign ticket failed — ${res.error}`);
    });
  };

  const onInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIdx((i) => Math.min(i + 1, Math.max(matches.length - 1, 0)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Home") {
      e.preventDefault();
      setActiveIdx(0);
    } else if (e.key === "End") {
      e.preventDefault();
      setActiveIdx(Math.max(matches.length - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const t = matches[activeIdx];
      if (t) pick(t.key);
    } else if (e.key === "Escape") {
      e.preventDefault();
      closeAndRestoreFocus();
    }
  };

  const triggerLabel = current
    ? currentIsStale
      ? `Ticket ${current} — not in cache`
      : `Change ticket (currently ${current})`
    : "Pick a ticket";

  return (
    <div className="combobox" ref={rootRef}>
      <button
        ref={triggerRef}
        type="button"
        className={`ticket-chip ${current ? "assigned" : "unassigned"} ${currentIsStale ? "stale" : ""}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={triggerLabel}
        aria-busy={isPending || undefined}
        onClick={() => setOpen((v) => !v)}
        onKeyDown={(e) => {
          if ((e.key === "Enter" || e.key === " ") && !open) {
            e.preventDefault();
            setOpen(true);
          } else if (e.key === "ArrowDown" && !open) {
            e.preventDefault();
            setOpen(true);
          }
        }}
      >
        {current ? (
          <>
            <span className="key">{current}</span>
            {currentIsStale ? (
              <span
                className="summary stale-note"
                title="Ticket key isn't in the Jira cache — click 'Refresh Jira' to reload"
              >
                <AlertCircle width={12} height={12} /> not in cache
              </span>
            ) : (
              currentTicket?.summary && (
                <span className="summary">{currentTicket.summary}</span>
              )
            )}
          </>
        ) : (
          <>
            <Ticket width={13} height={13} />
            <span className="key">Pick a ticket</span>
          </>
        )}
        <ChevronDown className="chevron" />
      </button>

      {open && (
        <div
          className="combobox-popover"
          role="listbox"
          id={`combobox-list-${blockId}`}
          aria-label="Jira tickets"
        >
          <div className="combobox-search">
            <Search />
            <input
              ref={inputRef}
              placeholder="Search PROJ-123 or words…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={onInputKeyDown}
              aria-label="Search tickets"
              aria-controls={`combobox-list-${blockId}`}
              aria-activedescendant={
                matches[activeIdx]
                  ? `combobox-item-${blockId}-${matches[activeIdx].key}`
                  : undefined
              }
            />
            {query && (
              <button
                type="button"
                aria-label="clear search"
                onClick={() => setQuery("")}
                style={{ color: "var(--fg-subtle)" }}
              >
                <X width={14} height={14} />
              </button>
            )}
          </div>

          <div className="combobox-list" ref={listRef}>
            {matches.length === 0 ? (
              <div className="combobox-empty">
                {tickets.length === 0
                  ? "No tickets cached — click 'Refresh Jira'"
                  : `No match for "${query}"`}
              </div>
            ) : (
              matches.map((t, idx) => (
                <button
                  key={t.key}
                  id={`combobox-item-${blockId}-${t.key}`}
                  data-idx={idx}
                  type="button"
                  role="option"
                  className="combobox-item"
                  aria-selected={t.key === current}
                  data-active={idx === activeIdx ? "true" : undefined}
                  onMouseEnter={() => setActiveIdx(idx)}
                  onClick={() => pick(t.key)}
                >
                  <span className="key">{t.key}</span>
                  <span className="summary">{t.summary ?? "—"}</span>
                  {t.status && <span className="status">{t.status}</span>}
                </button>
              ))
            )}
          </div>

          {current && (
            <button
              type="button"
              className="combobox-clear"
              onClick={() => pick(null)}
            >
              Unassign ({current})
            </button>
          )}
        </div>
      )}
    </div>
  );
}
