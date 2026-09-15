//! MentionService — Phase 37C (Le Mégaphone) + Phase 37F (Mégaphone Interstellaire).
//!
//! Service réutilisable qui orchestre le traitement des @mentions :
//! 1. Extrait les mentions du texte Markdown (via `mention::extract_mentions`)
//! 2. Résout les handles locaux en acteurs (via `ActorRepository`)
//! 3. Crée des événements `Mentioned` dans la timeline (Issue ou MR)
//! 4. **Phase 37F** — Route les mentions distantes via ActivityPub :
//!    WebFinger → Actor fetch → Create { Note { tag: Mention } } → deliver_activity
//!
//! Ce service est injecté dans `CreateIssue`, `CommentIssue`, et `CreateMR`.

use std::sync::Arc;
use tracing::{debug, info, warn};
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

/// Mention distante — routée vers l'inbox AP via WebFinger (Phase 37F).
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
    /// Mentions distantes (routées via `deliver_remote_mentions` — Phase 37F).
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
        // Mentions fédérées → routées via deliver_remote_mentions (Phase 37F)
        if mention.is_remote() {
            info!(
                handle = %mention.raw,
                domain = ?mention.domain,
                "📡 Mention fédérée détectée (Phase 37F — Mégaphone Interstellaire)"
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

// ── Phase 37F — Le Mégaphone Interstellaire 📡 ───────────────────────

use domain::ports::federation_repository::FederationRepository;
use infrastructure::federation::activity_builder::{mention_note_activity, MentionTarget};
use infrastructure::federation::delivery;
use infrastructure::federation::remote_actor::RemoteActorFetcher;
use infrastructure::federation::webfinger_resolver;

/// Livre les mentions distantes via ActivityPub.
///
/// Appelé en fire-and-forget (`tokio::spawn`) après `process_mentions()`.
/// Pour chaque mention distante :
/// 1. WebFinger lookup → actor URI + inbox URL
/// 2. Construction d'une activité `Create { Note { tag: Mention } }`
/// 3. Livraison signée RSA-SHA256 vers l'inbox distant
///
/// ## Sécurité
/// - Utilise la **keypair de l'auteur** (pas le system actor)
/// - SSRF guard hérité du RemoteActorFetcher
/// - Fire-and-forget : ne bloque pas la réponse HTTP
///
/// ## Arguments
/// - `remote_mentions` : mentions distantes extraites par `process_mentions()`
/// - `author_id` : UUID de l'auteur du commentaire (pour récupérer sa keypair)
/// - `context_url` : URL **frontend** de la ressource (issue, MR, etc.)
/// - `context_text` : texte brut du commentaire
/// - `note_id_suffix` : suffixe unique pour le Note id (ex: UUID du commentaire)
/// - `federation_domain` : domaine de notre instance
/// - `actor_repo` : pour résoudre le handle de l'auteur
/// - `federation_repo` : pour récupérer la keypair de l'auteur
/// - `remote_fetcher` : pour fetch les profils AP distants
pub async fn deliver_remote_mentions(
    remote_mentions: Vec<RemoteMention>,
    author_id: &Uuid,
    context_url: &str,
    context_text: &str,
    note_id_suffix: &str,
    federation_domain: &str,
    actor_repo: &Arc<dyn ActorRepository>,
    federation_repo: &Arc<dyn FederationRepository>,
    remote_fetcher: &Arc<RemoteActorFetcher>,
) {
    if remote_mentions.is_empty() {
        return;
    }

    // ── 1. Résoudre le handle de l'auteur ──
    let author_handle = match actor_repo.find_by_id(author_id).await {
        Ok(Some(actor)) => actor.handle,
        _ => {
            warn!(author_id = %author_id, "📡 Cannot resolve author handle — skipping remote mentions");
            return;
        }
    };

    // ── 2. Récupérer la keypair de l'auteur (Vegapunk Tweak #4) ──
    let keypair = match federation_repo.get_keypair(author_id).await {
        Ok(Some(kp)) => kp,
        Ok(None) => {
            warn!(
                author_id = %author_id,
                author_handle = %author_handle,
                "📡 No keypair for author — cannot sign mention deliveries"
            );
            return;
        }
        Err(e) => {
            warn!(error = %e, "📡 Failed to get author keypair");
            return;
        }
    };

    // ── 3. Résoudre chaque mention via WebFinger ──
    let mut targets: Vec<MentionTarget> = Vec::new();
    let mut inboxes: Vec<String> = Vec::new();

    for rm in &remote_mentions {
        match webfinger_resolver::resolve_mention(
            &rm.local_handle,
            &rm.domain,
            remote_fetcher,
        ).await {
            Ok(resolved) => {
                info!(
                    handle = %resolved.full_handle,
                    inbox = %resolved.inbox_url,
                    "📡✅ Remote mention resolved via WebFinger"
                );
                inboxes.push(resolved.inbox_url);
                targets.push(MentionTarget {
                    actor_uri: resolved.actor_uri,
                    full_handle: resolved.full_handle,
                });
            }
            Err(e) => {
                warn!(
                    handle = %rm.local_handle,
                    domain = %rm.domain,
                    error = %e,
                    "📡❌ WebFinger resolution failed — skipping this mention"
                );
            }
        }
    }

    if targets.is_empty() {
        info!("📡 No remote mentions resolved — nothing to deliver");
        return;
    }

    // ── 4. Construire l'activité Create { Note } ──
    let activity = mention_note_activity(
        federation_domain,
        &author_handle,
        &targets,
        context_text,
        context_url,
        note_id_suffix,
    );

    debug!(
        activity_json = %serde_json::to_string_pretty(&activity).unwrap_or_default(),
        "📡 Activity payload (debug)"
    );
    info!(
        author = %author_handle,
        targets = targets.len(),
        "📡🚀 Delivering mention activity to {} inbox(es)",
        inboxes.len()
    );

    // ── 5. Livrer à chaque inbox (séquentiellement, peu de mentions en général) ──
    for inbox in &inboxes {
        match delivery::deliver_activity(
            activity.clone(),
            inbox,
            &keypair.private_key_pem,
            &keypair.key_id,
        ).await {
            Ok(()) => {
                info!(
                    inbox = %inbox,
                    "📡✅ Mention activity delivered successfully"
                );
            }
            Err(e) => {
                warn!(
                    inbox = %inbox,
                    error = %e,
                    "📡⚠️ Mention delivery failed (non-fatal)"
                );
            }
        }
    }
}
