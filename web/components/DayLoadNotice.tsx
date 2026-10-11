export function DayLoadNotice({ failed }: { failed: string[] }) {
  if (failed.length === 0) return null;
  return (
    <p role="alert" className="day-load-notice">
      Couldn&apos;t load: {failed.join(", ")}. The rest of the day is shown.
    </p>
  );
}
