import "../logged/logged.css";
import "./week.css";

export default function WeekLayout({ children }: { children: React.ReactNode }) {
  return <div className="week-page">{children}</div>;
}
