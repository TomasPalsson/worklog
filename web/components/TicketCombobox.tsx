"use client";

import { useEffect, useMemo, useRef, useState, useTransition } from "react";
import { ChevronDown, Search, Ticket, X } from "lucide-react";
import type { JiraTicket } from "@/lib/types";
import { assignTicket } from "@/app/actions";

interface Props {
  blockId: number;
  current: string | null;
  tickets: JiraTicket[];
  day: string;
}

export function TicketCombobox({ blockId, current, tickets, day }: Props) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [, start] = useTransition();
  const ref = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const currentTicket = useMemo(
    () => tickets.find((t) => t.key === current),
    [tickets, current],
  );

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

  useEffect(() => {
    if (!open) return;
    inputRef.current?.focus();
    const onClick = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onClick);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const pick = (key: string | null) => {
    setOpen(false);
    setQuery("");
    start(() => assignTicket(blockId, key, day));
  };

  return (
    <div className="combobox" ref={ref}>
      <button
        type="button"
        className={`ticket-chip ${current ? "assigned" : "unassigned"}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        {current ? (
          <>
            <span className="key">{current}</span>
            {currentTicket?.summary && (
              <span className="summary">{currentTicket.summary}</span>
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
        <div className="combobox-popover" role="listbox">
          <div className="combobox-search">
            <Search />
            <input
              ref={inputRef}
              placeholder="Search PROJ-123 or words…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && matches[0]) {
                  pick(matches[0].key);
                }
              }}
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

          <div className="combobox-list">
            {matches.length === 0 ? (
              <div className="combobox-empty">
                {tickets.length === 0
                  ? "No tickets cached — click 'Refresh Jira'"
                  : `No match for "${query}"`}
              </div>
            ) : (
              matches.map((t) => (
                <button
                  key={t.key}
                  type="button"
                  role="option"
                  className="combobox-item"
                  aria-selected={t.key === current}
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
