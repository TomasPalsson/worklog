import "./logged.css";

export default function LoggedLayout({ children }: { children: React.ReactNode }) {
  return <div className="logged-page">{children}</div>;
}
