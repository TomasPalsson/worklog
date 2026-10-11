export function sectionLoader(failed: string[]) {
  const declared: string[] = [];
  return <T,>(name: string, p: Promise<T>, fallback: T): Promise<T> => {
    declared.push(name);
    return p.catch((err) => {
      console.error(`Day page: ${name} failed to load`, err);
      failed.push(name);
      failed.sort((x, y) => declared.indexOf(x) - declared.indexOf(y));
      return fallback;
    });
  };
}

export function DayLoadNotice({ failed }: { failed: string[] }) {
  if (failed.length === 0) return null;
  return (
    <p role="alert" className="review-sec-error">
      Couldn&apos;t load: {failed.join(", ")}. The rest of the day is shown.
    </p>
  );
}
