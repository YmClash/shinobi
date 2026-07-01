"use server";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono · Server Action: Forge Operation (URL-Driven)
// Encodes files to base64 and POSTs to Taijutsu backend
// Phase 5C: owner/repo extracted from FormData (URL params)
// ═══════════════════════════════════════════════════════════════

const TAIJUTSU_URL = process.env.TAIJUTSU_URL || "http://localhost:3000";

// Default author UUID — in production this would come from auth
const DEFAULT_AUTHOR_ID = "a1a2a3a4-b1b2-c1c2-d1d2-e1e2e3e4e5e6";

export interface ForgeResult {
  success: boolean;
  operationId?: string;
  error?: string;
}

export async function forgeOperation(formData: FormData): Promise<ForgeResult> {
  try {
    const description = formData.get("description") as string;
    const owner = formData.get("owner") as string;
    const repo = formData.get("repo") as string;

    if (!description?.trim()) {
      return { success: false, error: "La description est requise." };
    }

    if (!owner || !repo) {
      return { success: false, error: "Le contexte du dépôt est manquant." };
    }

    // Extract files from FormData
    const files = formData.getAll("files") as File[];
    if (files.length === 0) {
      return { success: false, error: "Au moins un fichier est requis." };
    }

    // Encode each file to base64
    const encodedFiles: { path: string; content_b64: string }[] = [];

    for (const file of files) {
      if (!(file instanceof File) || file.size === 0) continue;

      const buffer = await file.arrayBuffer();
      const base64 = Buffer.from(buffer).toString("base64");

      encodedFiles.push({
        path: file.name,
        content_b64: base64,
      });
    }

    if (encodedFiles.length === 0) {
      return { success: false, error: "Aucun fichier valide trouvé." };
    }

    // Call Taijutsu backend directly (server-side, no proxy needed)
    // Phase 5C: Route fédérée dynamique (owner/repo from URL params)
    const response = await fetch(
      `${TAIJUTSU_URL}/api/v1/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}/operations`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          author_id: DEFAULT_AUTHOR_ID,
          description: description.trim(),
          parent_ids: [],
          files: encodedFiles,
        }),
      },
    );

    if (!response.ok) {
      const text = await response.text().catch(() => "Erreur inconnue");
      return {
        success: false,
        error: `Taijutsu a répondu ${response.status}: ${text}`,
      };
    }

    const operation = await response.json();

    return {
      success: true,
      operationId: operation.id,
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : "Erreur inattendue";
    return { success: false, error: message };
  }
}
