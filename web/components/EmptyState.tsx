import { Moon } from "lucide-react";

export function EmptyState({ day }: { day: string }) {
  return (
    <div className="empty-state">
      <Moon
        size={28}
        strokeWidth={1.5}
        style={{ color: "var(--fg-subtle)", marginBottom: 10 }}
      />
      <h2>Nothing logged for {day}.</h2>
      <p>
        Run{" "}
        <code style={{ fontFamily: "var(--font-mono)", fontSize: 13 }}>
          worklog collect
        </code>{" "}
        and then <em>Rebuild blocks</em> above.
      </p>
    </div>
  );
}
