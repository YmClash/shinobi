//! Entités Kage Bunshin — Phase 41 (Auto-Healing CI/CD) 🥷⚡
//!
//! Représente les tentatives d'auto-correction des stages CI/CD en échec.
//! Quand un stage a `kage_bunshin: true` dans `jutsu.yml` et échoue,
//! Sensei analyse les logs d'erreur, propose un patch en hunks
//! (search/replace), et le Kage Bunshin tente de le vérifier dans
//! un workspace shadow jj.
//!
//! ## Flow
//! ```text
//! Stage ❌ → Healing 🥷 → Sensei analyse → Patch → Shadow re-run
//!   → Si ✅ → Healed (MR auto)
//!   → Si ❌ → Failed (notification)
//! ```
//!
//! ## Vegapunk Tweaks intégrés
//! - Tweak #2 : Smart context — regex-parse stacktrace, pas tout le repo
//! - Tweak #3b : Hunks search/replace, pas de fichier complet
//! - Tweak #4 : Statut `Healing` visible dans l'UI

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── HealStatus ──────────────────────────────────────────────────────

/// Statut d'une tentative de guérison Kage Bunshin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealStatus {
    /// Sensei analyse en cours.
    Pending,
    /// Patch appliqué, shadow re-run en cours.
    Healing,
    /// Patch vérifié, MR créée ✅.
    Success,
    /// Patch invalide ou re-run échoué ❌.
    Failed,
}

impl HealStatus {
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Healing => "healing",
            Self::Success => "success",
            Self::Failed => "failed",
        }
    }

    pub fn from_sql_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "healing" => Some(Self::Healing),
            "success" => Some(Self::Success),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

impl std::fmt::Display for HealStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_sql_str())
    }
}

// ── PatchHunk ───────────────────────────────────────────────────────

/// Hunk de remplacement — search/replace ciblé dans un fichier source.
///
/// ## Vegapunk Tweak #3b
/// Plutôt que de demander au LLM de réécrire un fichier complet (800 lignes
/// de tokens, hallucinations garanties), on demande des substitutions
/// chirurgicales : "cherche ce texte exact, remplace par celui-ci".
///
/// ## Sécurité
/// Le `search` doit correspondre **exactement** au texte du fichier source.
/// Si le texte n'est pas trouvé, le hunk est ignoré (fail-safe).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchHunk {
    /// Chemin relatif du fichier à modifier (ex: "src/main.rs").
    pub path: String,
    /// Texte exact à chercher dans le fichier (copié mot pour mot).
    pub search: String,
    /// Texte de remplacement.
    pub replace: String,
}

// ── KageBunshinPatch ────────────────────────────────────────────────

/// Résultat structuré du diagnostic Sensei.
///
/// Retourné par l'appel LLM après analyse des logs d'erreur.
/// Le format JSON est imposé par le prompt système (+ `format: "json"` Ollama).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KageBunshinPatch {
    /// Description concise de l'erreur (ex: "error[E0432]: unresolved import").
    pub diagnosis: String,
    /// Description humaine du fix (ex: "Added missing `use uuid::Uuid;`").
    pub patch_summary: String,
    /// Hunks de remplacement à appliquer.
    pub hunks: Vec<PatchHunk>,
    /// Score de confiance du LLM (0.0–1.0).
    /// Seuil minimum : 0.5 pour appliquer le patch.
    pub confidence: f32,
}

// ── HealAttempt ─────────────────────────────────────────────────────

/// Trace BDD d'une tentative de guérison Kage Bunshin.
///
/// Chaque fois qu'un stage échoue avec `kage_bunshin: true`,
/// un `HealAttempt` est créé pour tracer le diagnostic, le patch,
/// et le résultat du shadow re-run.
#[derive(Debug, Clone)]
pub struct HealAttempt {
    /// Identifiant unique.
    pub id: Uuid,
    /// Pipeline parent.
    pub pipeline_id: Uuid,
    /// Nom du stage en échec (ex: "Build", "Test").
    pub stage_name: String,

    // ── Diagnostic Sensei ────────────────────────────────────────
    /// Analyse LLM de l'erreur.
    pub diagnosis: String,
    /// Description humaine du fix proposé.
    pub patch_summary: Option<String>,
    /// Hunks de remplacement (sérialisés en JSONB en BDD).
    pub hunks: Vec<PatchHunk>,

    // ── Résultat ─────────────────────────────────────────────────
    /// Statut de la tentative.
    pub status: HealStatus,
    /// Branche shadow créée (ex: "kage-bunshin/build/4d26b098").
    pub shadow_branch: Option<String>,
    /// MR auto-générée (si succès).
    pub mr_id: Option<Uuid>,
    /// Logs de la re-exécution dans le shadow workspace.
    pub retry_logs: Option<String>,
    /// Exit code du shadow re-run.
    pub retry_exit_code: Option<i16>,

    // ── Métadonnées LLM ──────────────────────────────────────────
    /// Modèle Sensei utilisé (ex: "smollm2:1.7b").
    pub llm_model: Option<String>,
    /// Temps d'inférence en millisecondes.
    pub llm_duration_ms: Option<i32>,
    /// Score de confiance du diagnostic.
    pub confidence: Option<f32>,

    /// Date de création.
    pub created_at: DateTime<Utc>,
    /// Date de dernière mise à jour.
    pub updated_at: DateTime<Utc>,
}

impl HealAttempt {
    /// Construit un nouveau HealAttempt en status `pending`.
    pub fn new(
        pipeline_id: Uuid,
        stage_name: impl Into<String>,
        patch: &KageBunshinPatch,
        llm_model: Option<String>,
        llm_duration_ms: Option<i32>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            pipeline_id,
            stage_name: stage_name.into(),
            diagnosis: patch.diagnosis.clone(),
            patch_summary: Some(patch.patch_summary.clone()),
            hunks: patch.hunks.clone(),
            status: HealStatus::Pending,
            shadow_branch: None,
            mr_id: None,
            retry_logs: None,
            retry_exit_code: None,
            llm_model,
            llm_duration_ms,
            confidence: Some(patch.confidence),
            created_at: now,
            updated_at: now,
        }
    }
}

// ── HealResult ──────────────────────────────────────────────────────

/// Résultat d'une tentative Kage Bunshin.
///
/// Retourné par `KageBunshinUseCase::attempt_heal()` au `RunPipelineUseCase`.
#[derive(Debug)]
pub enum HealResult {
    /// Patch vérifié, MR créée ✅.
    Success {
        mr_id: Uuid,
        diagnosis: String,
        shadow_branch: String,
    },
    /// Patch échoué ou Sensei n'a pas pu diagnostiquer ❌.
    Failed {
        diagnosis: String,
        reason: String,
    },
    /// Kage Bunshin désactivé pour ce stage (kage_bunshin: false).
    Skipped,
}

// ── Tests unitaires ─────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heal_status_sql_roundtrip() {
        for status in [
            HealStatus::Pending,
            HealStatus::Healing,
            HealStatus::Success,
            HealStatus::Failed,
        ] {
            let sql = status.as_sql_str();
            let parsed = HealStatus::from_sql_str(sql).unwrap();
            assert_eq!(status, parsed);
        }
    }

    #[test]
    fn test_heal_attempt_new() {
        let patch = KageBunshinPatch {
            diagnosis: "error[E0432]: unresolved import".into(),
            patch_summary: "Added missing use statement".into(),
            hunks: vec![PatchHunk {
                path: "src/main.rs".into(),
                search: "fn main() {".into(),
                replace: "use uuid::Uuid;\nfn main() {".into(),
            }],
            confidence: 0.85,
        };

        let attempt = HealAttempt::new(
            Uuid::new_v4(),
            "Build",
            &patch,
            Some("smollm2:1.7b".into()),
            Some(1500),
        );

        assert_eq!(attempt.status, HealStatus::Pending);
        assert_eq!(attempt.stage_name, "Build");
        assert_eq!(attempt.hunks.len(), 1);
        assert_eq!(attempt.confidence, Some(0.85));
        assert!(attempt.mr_id.is_none());
        assert!(attempt.shadow_branch.is_none());
    }

    #[test]
    fn test_patch_hunk_serde_roundtrip() {
        let hunk = PatchHunk {
            path: "src/lib.rs".into(),
            search: "let x = 42;".into(),
            replace: "let x: i32 = 42;".into(),
        };

        let json = serde_json::to_string(&hunk).unwrap();
        let deserialized: PatchHunk = serde_json::from_str(&json).unwrap();
        assert_eq!(hunk.path, deserialized.path);
        assert_eq!(hunk.search, deserialized.search);
        assert_eq!(hunk.replace, deserialized.replace);
    }

    #[test]
    fn test_kage_bunshin_patch_serde() {
        let patch = KageBunshinPatch {
            diagnosis: "Missing import".into(),
            patch_summary: "Add use statement".into(),
            hunks: vec![],
            confidence: 0.2,
        };

        let json = serde_json::to_string(&patch).unwrap();
        let deserialized: KageBunshinPatch = serde_json::from_str(&json).unwrap();
        assert_eq!(patch.diagnosis, deserialized.diagnosis);
        assert!(deserialized.hunks.is_empty());
        assert!(deserialized.confidence < 0.5); // Below threshold → skip
    }
}
