import { redirect } from "next/navigation";
import { todayISO } from "@/lib/format";

export default function Home() {
  redirect(`/${todayISO()}`);
}
