import type { SVGProps } from "react";

/** A pencil writing a line, with Claude's spark — "rewrite this text from my instruction". Lucide-style line icon. */
export function RewriteSpark({ width = 16, height = 16, ...props }: SVGProps<SVGSVGElement>) {
  return (
    <svg
      viewBox="0 0 24 24"
      width={width}
      height={height}
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      <path d="M4.5 16.5 14 7l3 3-9.5 9.5-4 1Z" />
      <path d="M11.5 9.5l3 3" />
      <path d="M12 20h8" />
      <path d="M19 2.5c.3 1.6.8 2.2 2.5 2.5-1.7.3-2.2.9-2.5 2.5-.3-1.6-.8-2.2-2.5-2.5 1.7-.3 2.2-.9 2.5-2.5Z" />
    </svg>
  );
}
