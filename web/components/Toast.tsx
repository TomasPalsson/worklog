"use client";
import { createPortal } from "react-dom";
import { useEffect, useState } from "react";

export function Toast({
  children,
  tone,
}: {
  children: React.ReactNode;
  tone: "ok" | "error";
}) {
  const [mounted, setMounted] = useState(false);
  useEffect(() => {
    setMounted(true);
  }, []);
  if (!mounted) return null;
  return createPortal(
    <div className="toast-region" role="status" aria-live="polite">
      <div className={`toast ${tone}`}>{children}</div>
    </div>,
    document.body,
  );
}
