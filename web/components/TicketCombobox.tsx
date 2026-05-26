"use client";

import { useEffect, useMemo, useRef, useState, useTransition } from "react";
import { AlertCircle, ChevronDown, Loader2, Search, Ticket, X } from "lucide-react";
import type { JiraTicket } from "@/lib/types";
import {
  assignTicket,
  assignExternalTicket,
  searchJiraTickets,
} from "@/app/actions";
import { toast } from "@/lib/toast";

interface Props {
  blockId: number;
  current: string | null;
  tickets: JiraTicket[];
  day: string;
}

/** Debounce, in ms, between the last keystroke and the live Jira search.
 *  Short enough to feel responsive while typing; long enough to skip the
 *  per-keystroke fetch storm during fast typing. */
const SEARCH_DEBOUNCE_MS = 300;

/** Minimum query length before the live search fires. One- and two-letter
 *  queries explode the JQL match set without giving useful suggestions. */
const SEARCH_MIN_LEN = 2;

export function TicketCombobox({ blockId, current, tickets, day }: Props) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIdx, setActiveIdx] = useState(0);
  const [isPending, startTransition] = useTransition();
  // Results from the live `/tickets/search` endpoint. Kept separate from
  // the local `tickets` prop so the user sees the cached set immediately
  // and the external set fills in once the debounce fires.
  const [searchResults, setSearchResults] = useState<JiraTicket[]>([]);
  const [searchPending, setSearchPending] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const currentTicket = useMemo(
    () => tickets.find((t) => t.key === current),
    [tickets, current],
  );
  const currentIsStale = !!current && !currentTicket && tickets.length > 0;

  // Local cache hits — filtered client-side from the assigned-to-me set.
  const localMatches = useMemo(() => {
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

  // External hits — de-duped against the local set so we don't render the
  // same ticket twice when a Jira search happens to return your assigned
  // tickets as well.
  const externalMatches = useMemo(() => {
    if (searchResults.length === 0) return [];
    const localKeys = new Set(tickets.map((t) => t.key));
    return searchResults.filter((t) => !localKeys.has(t.key));
  }, [searchResults, tickets]);

  // Flat ordered list for keyboard navigation. Locals first, then
  // externals — arrow keys walk through both groups in render order.
  const flatItems = useMemo(
    () =>
      [
        ...localMatches.map((t) => ({ ticket: t, external: false })),
        ...externalMatches.map((t) => ({ ticket: t, external: true })),
      ] as Array<{ ticket: JiraTicket; external: boolean }>,
    [localMatches, externalMatches],
  );

  // Reset active index when the result set changes.
  useEffect(() => {
    setActiveIdx(0);
  }, [query, open, externalMatches]);

  // Reset external results + abort any in-flight search when the popover
  // closes. Stops a slow round-trip from arriving later and surprising
  // the user with results that no longer match the (cleared) input.
  useEffect(() => {
    if (!open) {
      setSearchResults([]);
      setSearchError(null);
      setSearchPending(false);
    }
  }, [open]);

  // Debounced live search. We don't AbortController-cancel the in-flight
  // request because server actions don't surface a signal — instead, each
  // fetch carries the query string it was launched with and is dropped
  // if it doesn't match the current input on arrival.
  useEffect(() => {
    if (!open) return;
    const q = query.trim();
    if (q.length < SEARCH_MIN_LEN) {
      setSearchResults([]);
      setSearchPending(false);
      setSearchError(null);
      return;
    }
    setSearchPending(true);
    const handle = setTimeout(async () => {
      const launched = q;
      const res = await searchJiraTickets(launched);
      // Bail if the user kept typing while we were waiting — applying
      // stale results would clobber whatever's relevant for the current
      // query.
      if (launched !== query.trim()) return;
      setSearchPending(false);
      if (res.ok) {
        setSearchResults(res.data);
        setSearchError(null);
      } else {
        setSearchResults([]);
        setSearchError(res.error);
      }
    }, SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(handle);
  }, [open, query]);

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

  const pickLocal = (key: string | null) => {
    closeAndRestoreFocus();
    startTransition(async () => {
      const res = await assignTicket(blockId, key, day);
      if (!res.ok) toast.error(`Assign ticket failed — ${res.error}`);
    });
  };

  const pickExternal = (ticket: JiraTicket) => {
    closeAndRestoreFocus();
    startTransition(async () => {
      const res = await assignExternalTicket(blockId, ticket, day);
      if (!res.ok) toast.error(`Assign ticket failed — ${res.error}`);
    });
  };

  const pickFlatIdx = (idx: number) => {
    const item = flatItems[idx];
    if (!item) return;
    if (item.external) pickExternal(item.ticket);
    else pickLocal(item.ticket.key);
  };

  const onInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIdx((i) => Math.min(i + 1, Math.max(flatItems.length - 1, 0)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Home") {
      e.preventDefault();
      setActiveIdx(0);
    } else if (e.key === "End") {
      e.preventDefault();
      setActiveIdx(Math.max(flatItems.length - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      pickFlatIdx(activeIdx);
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

  const activeDescendant = flatItems[activeIdx]
    ? `combobox-item-${blockId}-${flatItems[activeIdx].ticket.key}`
    : undefined;

  // External group starts at this flat index — used by the renderer to
  // keep `data-idx` continuous across the two groups so keyboard nav
  // and scroll-into-view stay in sync.
  const externalGroupStart = localMatches.length;

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
              placeholder="Search your tickets or any Jira ticket…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={onInputKeyDown}
              aria-label="Search tickets"
              aria-controls={`combobox-list-${blockId}`}
              aria-activedescendant={activeDescendant}
            />
            {searchPending && (
              <Loader2
                width={14}
                height={14}
                className="combobox-spinner"
                aria-label="Searching Jira"
              />
            )}
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
            {flatItems.length === 0 ? (
              <div className="combobox-empty">
                {tickets.length === 0
                  ? "No tickets cached — click 'Refresh Jira'"
                  : query.trim().length < SEARCH_MIN_LEN
                    ? `No match for "${query}"`
                    : searchPending
                      ? "Searching Jira…"
                      : `No match for "${query}" — try a different word`}
              </div>
            ) : (
              <>
                {localMatches.length > 0 && (
                  <div
                    className="combobox-group-label"
                    role="presentation"
                    aria-hidden="true"
                  >
                    Your tickets
                  </div>
                )}
                {localMatches.map((t, idx) => (
                  <button
                    key={`local-${t.key}`}
                    id={`combobox-item-${blockId}-${t.key}`}
                    data-idx={idx}
                    type="button"
                    role="option"
                    className="combobox-item"
                    aria-selected={t.key === current}
                    data-active={idx === activeIdx ? "true" : undefined}
                    onMouseEnter={() => setActiveIdx(idx)}
                    onClick={() => pickLocal(t.key)}
                  >
                    <span className="key">{t.key}</span>
                    <span className="summary">{t.summary ?? "—"}</span>
                    {t.status && <span className="status">{t.status}</span>}
                  </button>
                ))}
                {externalMatches.length > 0 && (
                  <div
                    className="combobox-group-label"
                    role="presentation"
                    aria-hidden="true"
                    title="Tickets not assigned to you — picked manually, never auto-assigned by the estimator"
                  >
                    Other Jira tickets
                  </div>
                )}
                {externalMatches.map((t, i) => {
                  const idx = externalGroupStart + i;
                  return (
                    <button
                      key={`ext-${t.key}`}
                      id={`combobox-item-${blockId}-${t.key}`}
                      data-idx={idx}
                      type="button"
                      role="option"
                      className="combobox-item external"
                      aria-selected={t.key === current}
                      data-active={idx === activeIdx ? "true" : undefined}
                      onMouseEnter={() => setActiveIdx(idx)}
                      onClick={() => pickExternal(t)}
                    >
                      <span className="key">{t.key}</span>
                      <span className="summary">{t.summary ?? "—"}</span>
                      {t.status && <span className="status">{t.status}</span>}
                    </button>
                  );
                })}
              </>
            )}
            {searchError && (
              <div className="combobox-empty" role="status">
                Jira search failed — {searchError}
              </div>
            )}
          </div>

          {current && (
            <button
              type="button"
              className="combobox-clear"
              onClick={() => pickLocal(null)}
            >
              Unassign ({current})
            </button>
          )}
        </div>
      )}
    </div>
  );
}
