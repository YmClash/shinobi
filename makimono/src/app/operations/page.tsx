import { redirect } from "next/navigation";

// ═══════════════════════════════════════════════════════════════
// Legacy Redirect — /operations → /system/default/operations
// Phase 5C: Rétro-compatibilité pour les bookmarks et liens existants
// ═══════════════════════════════════════════════════════════════

export default function LegacyOperationsRedirect() {
  redirect("/system/default/operations");
}
