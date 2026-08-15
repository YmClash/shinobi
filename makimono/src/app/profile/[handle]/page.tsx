import { redirect } from "next/navigation";

// ═══════════════════════════════════════════════════════════════
// Legacy redirect — /profile/[handle] → /[handle]
//
// Phase 37A : Le profil public a été déplacé vers /[owner].
// Cette route redirige les anciens liens pour éviter les 404.
// ═══════════════════════════════════════════════════════════════

interface LegacyProfileProps {
  params: Promise<{ handle: string }>;
}

export default async function LegacyProfileRedirect({ params }: LegacyProfileProps) {
  const { handle } = await params;
  redirect(`/${handle}`);
}
