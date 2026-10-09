import type { Tone } from "@/lib/progress";

const stroke = {
  fill: "none",
  stroke: "currentColor",
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;

export const TONE_WORDS: Record<Tone, string> = { ok: "On track", low: "Running low", over: "Over" };

export function ToneIcon({ tone }: { tone: Tone }) {
  return (
    <svg className="ep-st" viewBox="0 0 24 24" strokeWidth={2.25} {...stroke} role="img" aria-label={TONE_WORDS[tone]}>
      {tone === "ok" && (
        <>
          <circle cx="12" cy="12" r="9" />
          <path d="m8.25 12.25 2.75 2.75 4.75-5.25" />
        </>
      )}
      {tone === "low" && (
        <>
          <path d="M6 3h12M6 21h12" />
          <path d="M7.5 3c0 4.5 4.5 5.5 4.5 9s-4.5 4.5-4.5 9M16.5 3c0 4.5-4.5 5.5-4.5 9s4.5 4.5 4.5 9" />
          <path d="M9.5 18h5" />
        </>
      )}
      {tone === "over" && (
        <>
          <path d="M3 17h18" />
          <path d="M12 13V3.5" />
          <path d="m7.5 8 4.5-4.5L16.5 8" />
        </>
      )}
    </svg>
  );
}

export function EstimateFlagIcon() {
  return (
    <svg className="ep-fl" aria-hidden="true" viewBox="0 0 12 20" strokeWidth={1.5} {...stroke}>
      <path d="M2 1.5v17" />
      <path d="M2 2.5h7.25L7.5 5l1.75 2.5H2" />
    </svg>
  );
}

export function PendingIcon() {
  return (
    <svg className="ep-pd" aria-hidden="true" viewBox="0 0 16 16" strokeWidth={1.5} {...stroke}>
      <circle cx="8" cy="8" r="6.75" strokeDasharray="4.5 3" />
      <path d="M8 4.75v6.5M4.75 8h6.5" />
    </svg>
  );
}
