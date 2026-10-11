const f = (v: string) => ({ fill: `var(--${v})` });

export function RhythmIcon({ kind }: { kind: "always" | "sprint" | "one_off" }) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
      {kind === "always" && (
        <>
          <path
            transform="rotate(-30 8 8)"
            fillRule="evenodd"
            style={f("fg-subtle")}
            d="M0.8 8a7.2 3 0 1 0 14.4 0a7.2 3 0 1 0 -14.4 0zM2.1 8a5.9 1.3 0 1 1 11.8 0a5.9 1.3 0 1 1 -11.8 0z"
          />
          <circle cx="8" cy="8" r="3.2" fill="currentColor" />
          <circle cx="2.4" cy="11.4" r="1.6" style={f("amber")} />
        </>
      )}
      {kind === "sprint" && (
        <>
          <polygon points="3.6,9.2 8.8,5.6 9.6,6.8" style={f("fg-muted")} opacity=".55" />
          <polygon points="0.8,15.2 8.6,4.9 11.6,8.3" style={f("fg-muted")} />
          <circle cx="11.8" cy="4.6" r="3.2" style={f("amber")} />
        </>
      )}
      {kind === "one_off" && (
        <>
          <rect x="1" y="7.3" width="14" height="1.4" rx=".7" style={f("fg-subtle")} />
          <circle cx="8" cy="8" r="2.8" fill="currentColor" />
        </>
      )}
    </svg>
  );
}

export function FirstTimeMark() {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 14 14" width="14" height="14" aria-hidden="true">
      <polygon points="5.5,3.5 6.4,7.6 10.5,8.5 6.4,9.4 5.5,13.5 4.6,9.4 0.5,8.5 4.6,7.6" style={f("amber")} />
      <polygon points="11,0.3 11.8,2.2 13.7,3 11.8,3.8 11,5.7 10.2,3.8 8.3,3 10.2,2.2" style={f("amber")} />
      <circle cx="12" cy="10.5" r=".9" style={f("amber")} />
    </svg>
  );
}

export function OddballMark() {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 14 14" width="14" height="14" aria-hidden="true">
      <circle
        cx="7"
        cy="7"
        r="5.6"
        fill="none"
        style={{ stroke: "var(--fg-muted)" }}
        strokeWidth="1.2"
        strokeDasharray="2.2 1.8"
      />
      <circle cx="7" cy="7" r="2.4" fill="currentColor" />
    </svg>
  );
}

const ALT = {
  helper_down: "A telescope parked pointing down with its lens cap on, facing away from a star",
  too_few: "A nearly empty star chart with only three faint stars",
  no_map: "A star chart with a few dots and a dotted outline where the map will appear",
} as const;

const RING = Array.from({ length: 22 }, (_, i) => {
  const a = (i / 22) * 2 * Math.PI;
  return [(60 + 34 * Math.cos(a)).toFixed(1), (46 + 22 * Math.sin(a)).toFixed(1)] as const;
});
const GRID = [24, 48, 72, 96].flatMap((x) => [22, 45, 68].map((y) => [x, y] as const));

function Body({ kind }: { kind: keyof typeof ALT }) {
  if (kind === "helper_down")
    return (
      <>
        <circle cx="30" cy="22" r="2.4" style={f("amber")} />
        <circle cx="98" cy="14" r="1.1" style={f("fg-subtle")} />
        <circle cx="16" cy="40" r="1" style={f("fg-subtle")} />
        <circle cx="102" cy="34" r="1.2" style={f("fg-subtle")} />
        <polygon points="58,54 46,80 50,80 60,58" style={f("fg-subtle")} />
        <polygon points="62,54 74,80 70,80 60,58" style={f("fg-subtle")} />
        <rect x="58.5" y="54" width="3" height="26" style={f("fg-subtle")} opacity=".55" />
        <g transform="rotate(22 60 50)">
          <rect x="36" y="44" width="46" height="12" rx="2" style={f("fg-muted")} />
          <rect x="30" y="46" width="8" height="8" rx="1" style={f("fg-muted")} opacity=".55" />
          <rect x="74" y="48" width="7" height="2.5" style={f("slate")} />
          <rect x="80" y="37" width="7" height="26" rx="2" style={f("slate")} />
          <rect x="86.5" y="47.5" width="3" height="5" style={f("slate")} />
        </g>
      </>
    );
  if (kind === "too_few")
    return (
      <>
        <g style={f("fg-subtle")} opacity=".55">
          {GRID.map(([x, y]) => (
            <circle key={`${x}-${y}`} cx={x} cy={y} r=".9" />
          ))}
        </g>
        <polygon points="40,31 42,36.5 47.5,38 42,39.5 40,45 38,39.5 32.5,38 38,36.5" style={f("fg-muted")} />
        <polygon
          points="80,52 81.4,55.6 85,57 81.4,58.4 80,62 78.6,58.4 75,57 78.6,55.6"
          style={f("fg-muted")}
          opacity=".55"
        />
        <polygon points="64,20 65.8,25 71,27 65.8,29 64,34 62.2,29 57,27 62.2,25" style={f("amber")} />
      </>
    );
  return (
    <>
      <g style={f("fg-subtle")} opacity=".3">
        {GRID.map(([x, y]) => (
          <circle key={`${x}-${y}`} cx={x} cy={y} r=".9" />
        ))}
      </g>
      <g style={f("fg-subtle")}>
        {RING.map(([x, y]) => (
          <circle key={`${x}-${y}`} cx={x} cy={y} r="1.4" />
        ))}
      </g>
      <circle cx="46" cy="40" r="2.6" style={f("fg-muted")} />
      <circle cx="55" cy="52" r="2" style={f("fg-muted")} opacity=".55" />
      <circle cx="70" cy="38" r="3" style={f("amber")} />
      <circle cx="78" cy="54" r="1.8" style={f("fg-muted")} opacity=".55" />
    </>
  );
}

export function AtlasEmpty({ kind }: { kind: keyof typeof ALT }) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 90" width="120" height="90" role="img" aria-label={ALT[kind]}>
      <rect x="4" y="4" width="112" height="82" rx="8" style={f("bg-sunk")} />
      <Body kind={kind} />
    </svg>
  );
}
