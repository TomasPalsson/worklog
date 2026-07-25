"use client";

// A Spotlight-style picker: chip trigger → floating panel with a search field
// and a filtered list, driven by arrow keys / Enter / Escape.
//
// Built on the SAME markup and CSS as `TicketCombobox` (`.combobox`,
// `.combobox-popover`, `.combobox-search`, `.combobox-item`) rather than a
// second lookalike. Native <select> was the wrong control here: it can't
// filter, and on macOS it renders as a system menu that looks nothing like
// the rest of this app.
//
// `allowFree` turns it into a combobox proper — whatever you type becomes a
// selectable value. Verkefni needs that (the accounting keys live in the
// external system, so the first time you bill a project you paste it in);
// customers do not (an unregistered customer is a typo, not a new customer).

import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, Loader2, Search, X } from "lucide-react";

interface Props {
  /** Currently selected value, or null when unset. */
  value: string | null;
  options: string[];
  /** Placeholder on the trigger when nothing is selected. */
  placeholder: string;
  /** Search-field placeholder. */
  searchPlaceholder: string;
  /** Accessible name for the control. */
  label: string;
  /** Tooltip on the trigger — say what selecting actually does. */
  tip?: string;
  /** Accept a typed value that isn't in `options`. */
  allowFree?: boolean;
  busy?: boolean;
  onPick: (value: string) => void;
  /** Offered as "Clear" when a value is set. Omit to hide. */
  onClear?: () => void;
}

export function PalettePicker({
  value,
  options,
  placeholder,
  searchPlaceholder,
  label,
  tip,
  allowFree = false,
  busy = false,
  onPick,
  onClear,
}: Props) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIdx, setActiveIdx] = useState(0);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const matches = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return options;
    return options.filter((o) => o.toLowerCase().includes(q));
  }, [options, query]);

  // A typed value that matches nothing gets its own entry at the end, so
  // Enter always does something predictable.
  const freeValue =
    allowFree && query.trim() !== "" && !matches.includes(query.trim())
      ? query.trim()
      : null;
  const items = freeValue ? [...matches, freeValue] : matches;

  useEffect(() => setActiveIdx(0), [query, open]);

  useEffect(() => {
    if (!open) return;
    inputRef.current?.focus();
    const onDown = (e: MouseEvent) => {
      // Close without stealing focus — the user clicked something else and
      // means to focus it.
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    listRef.current
      ?.querySelector<HTMLElement>(`[data-idx="${activeIdx}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [activeIdx, open]);

  function close() {
    setOpen(false);
    setQuery("");
    queueMicrotask(() => triggerRef.current?.focus());
  }

  function pick(v: string | undefined) {
    if (!v) return;
    close();
    onPick(v);
  }

  return (
    <div className="combobox" ref={rootRef} onClick={(e) => e.stopPropagation()}>
      <button
        ref={triggerRef}
        type="button"
        className={`ticket-chip ${value ? "assigned" : "unassigned"}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={value ? `${label} — currently ${value}` : label}
        aria-busy={busy || undefined}
        data-tip={tip}
        onClick={() => setOpen((v) => !v)}
        onKeyDown={(e) => {
          if (!open && (e.key === "Enter" || e.key === " " || e.key === "ArrowDown")) {
            e.preventDefault();
            setOpen(true);
          }
        }}
      >
        <span className="key">{value ?? placeholder}</span>
        {busy ? <Loader2 className="spin" width={12} height={12} /> : <ChevronDown className="chevron" />}
      </button>

      {open && (
        <div className="combobox-popover" role="listbox" aria-label={label}>
          <div className="combobox-search">
            <Search />
            <input
              ref={inputRef}
              placeholder={searchPlaceholder}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              aria-label={searchPlaceholder}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") {
                  e.preventDefault();
                  setActiveIdx((i) => Math.min(i + 1, Math.max(items.length - 1, 0)));
                } else if (e.key === "ArrowUp") {
                  e.preventDefault();
                  setActiveIdx((i) => Math.max(i - 1, 0));
                } else if (e.key === "Enter") {
                  e.preventDefault();
                  pick(items[activeIdx]);
                } else if (e.key === "Escape") {
                  e.preventDefault();
                  close();
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

          <div className="combobox-list" ref={listRef}>
            {items.length === 0 ? (
              <div className="combobox-empty">
                {options.length === 0
                  ? "Nothing to pick yet"
                  : `No match for "${query}"`}
              </div>
            ) : (
              items.map((o, idx) => (
                <button
                  key={o}
                  data-idx={idx}
                  type="button"
                  role="option"
                  className="combobox-item"
                  aria-selected={o === value}
                  data-active={idx === activeIdx ? "true" : undefined}
                  onMouseEnter={() => setActiveIdx(idx)}
                  onClick={() => pick(o)}
                >
                  <span className="key">{o}</span>
                  {o === freeValue && <span className="summary">use as typed</span>}
                </button>
              ))
            )}
          </div>

          {value && onClear && (
            <button
              type="button"
              className="combobox-clear"
              onClick={() => {
                close();
                onClear();
              }}
            >
              Clear ({value})
            </button>
          )}
        </div>
      )}
    </div>
  );
}
