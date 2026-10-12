"use client";

import { useRouter } from "next/navigation";
import { useEffect } from "react";

export function RefreshSoon({ intervalMs = 20_000, maxTries = 6 }: { intervalMs?: number; maxTries?: number }) {
  const router = useRouter();
  useEffect(() => {
    let tries = 0;
    const id = setInterval(() => {
      router.refresh();
      if (++tries >= maxTries) clearInterval(id);
    }, intervalMs);
    return () => clearInterval(id);
  }, [router, intervalMs, maxTries]);
  return <span data-refresh-soon hidden />;
}
