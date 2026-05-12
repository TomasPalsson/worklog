import { redirect } from "next/navigation";
import { mondayOf, todayISO } from "@/lib/format";

export default function WeekIndex() {
  redirect(`/week/${mondayOf(todayISO())}`);
}
