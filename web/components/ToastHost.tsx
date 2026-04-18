"use client";
import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { subscribe, type ToastMsg } from "@/lib/toast";

/**
 * Mounted once at the page root. Subscribes to the in-memory toast
 * bus and renders messages. Uses two separate live regions so errors
 * (assertive) interrupt the screen reader while successes (polite) wait.
 */
export function ToastHost() {
  const [mounted, setMounted] = useState(false);
  const [msgs, setMsgs] = useState<ToastMsg[]>([]);
  useEffect(() => {
    setMounted(true);
    const unsub = subscribe(setMsgs);
    return () => unsub();
  }, []);
  if (!mounted) return null;

  const ok = msgs.filter((m) => m.tone === "ok");
  const err = msgs.filter((m) => m.tone === "error");

  return createPortal(
    <div className="toast-region">
      <div role="status" aria-live="polite" aria-atomic="false">
        {ok.map((m) => (
          <div key={m.id} className="toast ok">
            {m.text}
          </div>
        ))}
      </div>
      <div role="alert" aria-live="assertive" aria-atomic="false">
        {err.map((m) => (
          <div key={m.id} className="toast error">
            {m.text}
          </div>
        ))}
      </div>
    </div>,
    document.body,
  );
}
