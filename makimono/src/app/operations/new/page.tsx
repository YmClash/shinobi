import { redirect } from "next/navigation";

// Legacy Redirect — /operations/new → /system/default/operations/new
export default function LegacyNewOperationRedirect() {
  redirect("/system/default/operations/new");
}
