import { redirect } from "next/navigation";
import { monthOf, todayISO } from "@/lib/format";

export const dynamic = "force-dynamic";

export default function LoggedPage() {
  redirect(`/logged/month/${monthOf(todayISO())}`);
}
