//! MentionService — Phase 37C (Le Mégaphone).
//!
//! Service réutilisable qui orchestre le traitement des @mentions :
//! 1. Extrait les mentions du texte Markdown (via `mention::extract_mentions`)
//! 2. Résout les handles locaux en acteurs (via `ActorRepository`)
//! 3. Crée des événements `Mentioned` dans la timeline (Issue ou MR)
//!
//! Ce service est injecté dans `CreateIssue`, `CommentIssue`, et `CreateMR`.

use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use domain::entities::mention::extract_mentions;
use domain::ports::actor_repository::ActorRepository;

/// Mention résolue — prête à être persistée comme événement.
#[derive(Debug)]
pub struct ResolvedMention {
    /// UUID de l'acteur mentionné (local).
    pub actor_id: Uuid,
    /// Handle brut de la mention.
    pub handle: String,
}

/// Mention distante — stockée pour le fanout fédéré futur (Phase 37D).
#[derive(Debug)]
pub struct RemoteMention {
    /// Handle local de l'acteur distant.
    pub local_handle: String,
    /// Domaine distant.
    pub domain: String,
}

/// Résultat du traitement des mentions.
#[derive(Debug)]
pub struct MentionResult {
    /// Mentions locales résolues avec succès.
    pub resolved: Vec<ResolvedMention>,
    /// Mentions distantes (stockées, pas encore routées).
    pub remote: Vec<RemoteMention>,
    /// Handles non trouvés (ignore silencieusement).
    pub unresolved_count: usize,
}

/// Extrait et résout les @mentions d'un texte Markdown.
///
/// ## Flow
/// 1. Parse le Markdown AST pour extraire les mentions
/// 2. Pour chaque mention locale → `actor_repo.find_by_handle()`
/// 3. Pour chaque mention distante → stockée pour Phase 37D
/// 4. Handles non trouvés → warning log, pas d'erreur
///
/// ## Arguments
/// - `text` : Texte Markdown contenant potentiellement des @mentions
/// - `author_id` : UUID de l'auteur (exclu des auto-mentions)
/// - `actor_repo` : Repository des acteurs pour la résolution des handles
pub async fn process_mentions(
    text: &str,
    author_id: &Uuid,
    actor_repo: &Arc<dyn ActorRepository>,
) -> MentionResult {
    let mentions = extract_mentions(text);

    if mentions.is_empty() {
        return MentionResult {
            resolved: Vec::new(),
            remote: Vec::new(),
            unresolved_count: 0,
        };
    }

    let mut resolved = Vec::new();
    let mut remote = Vec::new();
    let mut unresolved_count = 0;

    for mention in mentions {
        // Mentions fédérées → stockées pour Phase 37D
        if mention.is_remote() {
            info!(
                handle = %mention.raw,
                domain = ?mention.domain,
                "📡 Mention fédérée détectée (Phase 37D)"
            );
            remote.push(RemoteMention {
                local_handle: mention.local_handle,
                domain: mention.domain.unwrap_or_default(),
            });
            continue;
        }

        // Résolution locale
        match actor_repo.find_by_handle(&mention.local_handle).await {
            Ok(Some(actor)) => {
                // Ne pas notifier l'auteur s'il se mentionne lui-même
                if actor.id == *author_id {
                    continue;
                }

                info!(
                    mentioned_actor = %actor.id,
                    handle = %mention.local_handle,
                    "📣 @mention résolue"
                );
                resolved.push(ResolvedMention {
                    actor_id: actor.id,
                    handle: mention.local_handle,
                });
            }
            Ok(None) => {
                warn!(
                    handle = %mention.local_handle,
                    "⚠️ @mention non résolue — handle introuvable"
                );
                unresolved_count += 1;
            }
            Err(e) => {
                warn!(
                    handle = %mention.local_handle,
                    error = %e,
                    "⚠️ Erreur lors de la résolution de @mention"
                );
                unresolved_count += 1;
            }
        }
    }

    MentionResult {
        resolved,
        remote,
        unresolved_count,
    }
}
