// Recognisable marks for the Details view: the real Claude / Slack / GitHub /
// git logos in their own colours, lucide for the rest. Inline SVG so they
// render crisp at 13–14px with no extra requests.

import {
  CalendarDays,
  Circle,
  FilePen,
  FilePlus2,
  FileText,
  FolderSearch,
  Globe,
  ListChecks,
  Search,
  SquareTerminal,
  Users,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import type { SourceKind } from "@/lib/detailRows";

type IconProps = { size?: number };

export function ClaudeMark({ size = 14 }: IconProps) {
  // The Claude starburst: twelve rounded rays, alternating long and short.
  const rays = Array.from({ length: 12 }, (_, i) => {
    const a = (i * Math.PI) / 6;
    const r = i % 2 === 0 ? 10 : 7.5;
    // Rounded so server and browser render byte-identical coordinates.
    return { x: (12 + Math.cos(a) * r).toFixed(2), y: (12 + Math.sin(a) * r).toFixed(2) };
  });
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
      {rays.map((p, i) => (
        <line key={i} x1="12" y1="12" x2={p.x} y2={p.y} stroke="#D97757" strokeWidth="2.6" strokeLinecap="round" />
      ))}
    </svg>
  );
}

export function SlackMark({ size = 14 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
      <rect x="1" y="7.5" width="10" height="3.5" rx="1.75" fill="#36C5F0" />
      <rect x="7.5" y="1" width="3.5" height="5" rx="1.75" fill="#36C5F0" />
      <rect x="13" y="1" width="3.5" height="10" rx="1.75" fill="#2EB67D" />
      <rect x="18" y="7.5" width="5" height="3.5" rx="1.75" fill="#2EB67D" />
      <rect x="13" y="13" width="10" height="3.5" rx="1.75" fill="#ECB22E" />
      <rect x="13" y="18" width="3.5" height="5" rx="1.75" fill="#ECB22E" />
      <rect x="7.5" y="13" width="3.5" height="10" rx="1.75" fill="#E01E5A" />
      <rect x="1" y="13" width="5" height="3.5" rx="1.75" fill="#E01E5A" />
    </svg>
  );
}

export function GitHubMark({ size = 14 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12" />
    </svg>
  );
}

export function GitMark({ size = 14 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="#F05033" aria-hidden="true">
      <path d="M23.546 10.93L13.067.452c-.604-.603-1.582-.603-2.188 0L8.708 2.627l2.76 2.76c.645-.215 1.379-.07 1.889.441.516.515.658 1.258.438 1.9l2.658 2.66c.645-.223 1.387-.078 1.9.435.721.72.721 1.884 0 2.604-.719.719-1.881.719-2.6 0-.539-.541-.674-1.337-.404-1.996L12.86 8.955v6.525c.176.086.342.203.488.348.713.721.713 1.883 0 2.6-.719.721-1.889.721-2.609 0-.719-.719-.719-1.879 0-2.598.182-.18.387-.316.605-.406V8.835c-.217-.091-.424-.222-.6-.401-.545-.545-.676-1.342-.396-2.009L7.636 3.7.45 10.881c-.6.605-.6 1.584 0 2.189l10.48 10.477c.604.604 1.582.604 2.186 0l10.43-10.43c.605-.603.605-1.582 0-2.187" />
    </svg>
  );
}

export function GoogleCalendarMark({ size = 14 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 200 200" aria-hidden="true">
      <g transform="translate(3.75 3.75)">
        <path fill="#FFFFFF" d="M148.882,43.618l-47.368-5.263l-57.895,5.263L38.355,96.25l5.263,52.632l52.632,6.579l52.632-6.579l5.263-53.947L148.882,43.618z" />
        <path fill="#1A73E8" d="M65.211,125.276c-3.934-2.658-6.658-6.539-8.145-11.671l9.132-3.763c0.829,3.158,2.276,5.605,4.342,7.342c2.053,1.737,4.553,2.592,7.474,2.592c2.987,0,5.553-0.908,7.697-2.724s3.224-4.132,3.224-6.934c0-2.868-1.132-5.211-3.395-7.026s-5.105-2.724-8.5-2.724h-5.276v-9.039H76.5c2.921,0,5.382-0.789,7.382-2.368c2-1.579,3-3.737,3-6.487c0-2.447-0.895-4.395-2.684-5.855s-4.053-2.197-6.803-2.197c-2.684,0-4.816,0.711-6.395,2.145s-2.724,3.197-3.447,5.276l-9.039-3.763c1.197-3.395,3.395-6.395,6.618-8.987c3.224-2.592,7.342-3.895,12.342-3.895c3.697,0,7.026,0.711,9.974,2.145c2.947,1.434,5.263,3.421,6.934,5.947c1.671,2.539,2.5,5.382,2.5,8.539c0,3.224-0.776,5.947-2.329,8.184c-1.553,2.237-3.461,3.947-5.724,5.145v0.539c2.987,1.25,5.421,3.158,7.342,5.724c1.908,2.566,2.868,5.632,2.868,9.211s-0.908,6.776-2.724,9.579c-1.816,2.803-4.329,5.013-7.513,6.618c-3.197,1.605-6.789,2.421-10.776,2.421C73.408,129.263,69.145,127.934,65.211,125.276z" />
        <path fill="#1A73E8" d="M121.25,79.961l-9.974,7.25l-5.013-7.605l17.987-12.974h6.895v61.197h-9.895L121.25,79.961z" />
        <path fill="#EA4335" d="M148.882,196.25l47.368-47.368l-23.684-10.526l-23.684,10.526l-10.526,23.684L148.882,196.25z" />
        <path fill="#34A853" d="M33.092,172.566l10.526,23.684h105.263v-47.368H43.618L33.092,172.566z" />
        <path fill="#4285F4" d="M11.75-3.75C3.026-3.75-3.75,3.026-3.75,11.75v136.184l23.684,10.526l23.684-10.526V43.618h105.263l10.526-23.684L148.882-3.75H11.75z" />
        <path fill="#188038" d="M-3.75,148.882v31.579c0,8.724,7.066,15.789,15.789,15.789h31.579v-47.368H-3.75z" />
        <path fill="#FBBC04" d="M148.882,43.618v105.263h47.368V43.618l-23.684-10.526L148.882,43.618z" />
        <path fill="#1967D2" d="M196.25,43.618V11.75c0-8.724-7.066-15.789-15.789-15.789h-31.579v47.368H196.25z" />
      </g>
    </svg>
  );
}

function lucide(Icon: LucideIcon) {
  return function LucideMark({ size = 14 }: IconProps) {
    return <Icon width={size} height={size} strokeWidth={1.75} aria-hidden="true" />;
  };
}

export const SOURCE_ICON: Record<SourceKind, (p: IconProps) => React.JSX.Element> = {
  claude: ClaudeMark,
  shell: lucide(SquareTerminal),
  git: GitMark,
  github: GitHubMark,
  slack: SlackMark,
  web: lucide(Globe),
  meeting: lucide(CalendarDays),
  other: lucide(Circle),
};

const TOOL_ICON: Record<string, LucideIcon> = {
  Bash: SquareTerminal,
  Read: FileText,
  Edit: FilePen,
  MultiEdit: FilePen,
  NotebookEdit: FilePen,
  Write: FilePlus2,
  Grep: Search,
  Glob: FolderSearch,
  Agent: Users,
  Task: Users,
  WebFetch: Globe,
  WebSearch: Globe,
  TodoWrite: ListChecks,
};

export function ToolIcon({ tool, size = 12 }: { tool: string; size?: number }) {
  const Icon = TOOL_ICON[tool] ?? Wrench;
  return <Icon width={size} height={size} strokeWidth={1.9} aria-hidden="true" />;
}
