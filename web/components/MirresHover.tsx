"use client";

import type { ReactNode, SyntheticEvent } from "react";

/** Dims the big bar's other customers while a ledger row is hovered or focused. */
export function MirresHover({ children }: { children: ReactNode }) {
  function mark(el: HTMLElement, name: string | null) {
    for (const seg of el.querySelectorAll<HTMLElement>(".mirres-bar .mirres-seg")) {
      if (name !== null && seg.dataset.customer !== name) seg.dataset.dim = "";
      else delete seg.dataset.dim;
    }
  }
  const on = (e: SyntheticEvent<HTMLElement>) => {
    const row = (e.target as HTMLElement).closest<HTMLElement>(".mirres-ledger-list > li[data-customer]");
    mark(e.currentTarget, row?.dataset.customer ?? null);
  };
  const off = (e: SyntheticEvent<HTMLElement>) => mark(e.currentTarget, null);
  return (
    <div className="mirres-hover" onMouseOver={on} onMouseLeave={off} onFocus={on} onBlur={off}>
      {children}
    </div>
  );
}
