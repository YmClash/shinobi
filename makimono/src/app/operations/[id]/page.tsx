import { redirect } from "next/navigation";

// Legacy Redirect — /operations/[id] → /system/default/operations/[id]
export default async function LegacyOperationDetailRedirect({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  redirect(`/system/default/operations/${id}`);
}
