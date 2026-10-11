"use client";

import { useRouter } from "next/navigation";
import { useEffect } from "react";

const DELAY_MS = 25_000;

export function RefreshSoon() {
  const router = useRouter();
  useEffect(() => {
    const id = setTimeout(() => router.refresh(), DELAY_MS);
    return () => clearTimeout(id);
  }, [router]);
  return <span data-refresh-soon hidden />;
}
