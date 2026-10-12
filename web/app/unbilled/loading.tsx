export default function Loading() {
  return (
    <div className="ub-loading" role="status" aria-label="Loading unbilled work">
      <div className="ub-skel ub-skel--title" />
      <div className="ub-skel ub-skel--steps" />
      <div className="ub-skel ub-skel--strip" />
      <div className="ub-skel ub-skel--card" />
      <div className="ub-skel ub-skel--card" />
    </div>
  );
}
