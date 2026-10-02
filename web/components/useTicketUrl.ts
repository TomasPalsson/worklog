import { useEffect, useRef } from "react";

const PARAM = "ticket";

const keyInUrl = () => new URLSearchParams(window.location.search).get(PARAM);

/** Writes `?ticket=` straight to the History API (no Next navigation, so nothing refetches). */
function writeKey(key: string | null, push: boolean) {
  const url = new URL(window.location.href);
  if (key) url.searchParams.set(PARAM, key);
  else url.searchParams.delete(PARAM);
  const next = url.pathname + url.search + url.hash;
  try {
    if (push) window.history.pushState(null, "", next);
    else window.history.replaceState(null, "", next);
  } catch {
    /* a sandboxed frame may forbid it; the dialog still works, it just isn't linkable */
  }
}

/**
 * Keeps `?ticket=KEY` in step with the open ticket so it can be linked and Back closes it. Loading the page with
 * the param opens that ticket when it is on the board; Back and Forward follow the URL.
 * `onUrl` receives the ticket the URL now names (null for none); `known` says whether a key is on the board.
 */
export function useTicketUrl(known: (key: string) => boolean, onUrl: (key: string | null) => void) {
  const pushed = useRef(false);
  const latest = useRef({ known, onUrl });
  latest.current = { known, onUrl };
  useEffect(() => {
    const read = () => {
      const key = keyInUrl();
      return key && latest.current.known(key) ? key : null;
    };
    const initial = read();
    if (initial) latest.current.onUrl(initial);
    const onPop = () => {
      pushed.current = false;
      latest.current.onUrl(read());
    };
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, []);
  return {
    /** A card was opened; `switching` when another ticket was already open (one Back still closes it all). */
    opened: (key: string, switching: boolean) => {
      writeKey(key, !switching);
      if (!switching) pushed.current = true;
    },
    closed: () => {
      if (!pushed.current) return writeKey(null, false);
      pushed.current = false;
      window.history.back();
    },
  };
}
