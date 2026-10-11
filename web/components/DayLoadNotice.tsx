export function sectionLoader(failed: string[]) {
  return <T,>(name: string, p: Promise<T>, fallback: T): Promise<T> =>
    p.catch(() => {
      failed.push(name);
      return fallback;
    });
}

export function DayLoadNotice({ failed }: { failed: string[] }) {
  if (failed.length === 0) return null;
  return (
    <p role="alert" className="review-sec-error">
      Couldn&apos;t load: {failed.join(", ")}. The rest of the day is shown.
    </p>
  );
}
