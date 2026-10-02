"use client";

import { Search, X } from "lucide-react";

export function TaskToolbar(p: {
  text: string;
  onlyWorked: boolean;
  setText: (v: string) => void;
  setOnlyWorked: (v: boolean) => void;
}) {
  return (
    <div className="task-toolbar">
      <div className="task-filter">
        <label htmlFor="task-filter-input" className="task-label">
          Filter
        </label>
        <span className="task-filter-box">
          <Search className="task-filter-icon" size={14} aria-hidden="true" />
          <input
            id="task-filter-input"
            type="search"
            placeholder="Key or title"
            value={p.text}
            onChange={(e) => p.setText(e.target.value)}
          />
          {p.text && (
            <button type="button" aria-label="Clear filter" onClick={() => p.setText("")}>
              <X size={14} aria-hidden="true" />
            </button>
          )}
        </span>
      </div>
      <label className="task-check">
        <input type="checkbox" checked={p.onlyWorked} onChange={(e) => p.setOnlyWorked(e.target.checked)} />
        Only tickets I worked this week
      </label>
    </div>
  );
}
