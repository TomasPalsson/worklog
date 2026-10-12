import { redirect } from "next/navigation";

// The page moved to /unbilled; old links and bookmarks still land there.
export default function LooseEndsRedirect() {
  redirect("/unbilled");
}
