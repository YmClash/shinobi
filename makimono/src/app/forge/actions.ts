"use server";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono · Server Action: Forge Repository
// Creates a new repository via the Taijutsu backend
// ═══════════════════════════════════════════════════════════════

const TAIJUTSU_URL = process.env.TAIJUTSU_URL || "http://localhost:3000";

// Default owner UUID — SYSTEM_ACTOR from migration 006
const DEFAULT_OWNER_ID = "00000000-0000-0000-0000-000000000001";

export interface ForgeRepoResult {
  success: boolean;
  repoId?: string;
  error?: string;
}

export async function forgeRepository(formData: FormData): Promise<ForgeRepoResult> {
  try {
    const name = formData.get("name") as string;
    const displayName = formData.get("display_name") as string;
    const description = formData.get("description") as string;
    const visibility = formData.get("visibility") as string;

    if (!name?.trim() || !displayName?.trim()) {
      return { success: false, error: "Le nom et le nom d'affichage sont requis." };
    }

    const response = await fetch(`${TAIJUTSU_URL}/api/v1/repos`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        owner_id: DEFAULT_OWNER_ID,
        name: name.trim(),
        display_name: displayName.trim(),
        description: description?.trim() || null,
        visibility: visibility || "public",
      }),
    });

    if (!response.ok) {
      const text = await response.text().catch(() => "Erreur inconnue");

      if (response.status === 409) {
        return { success: false, error: `Le dépôt "${name}" existe déjà. Choisissez un autre nom.` };
      }

      return {
        success: false,
        error: `Taijutsu a répondu ${response.status}: ${text}`,
      };
    }

    const repo = await response.json();

    return {
      success: true,
      repoId: repo.id,
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : "Erreur inattendue";
    return { success: false, error: message };
  }
}
