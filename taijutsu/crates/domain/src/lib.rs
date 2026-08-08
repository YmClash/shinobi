//! # SHINOBI — Domain Layer
//!
//! Cœur pur de l'architecture hexagonale.
//! Ce crate ne contient **aucune dépendance technique** :
//! uniquement des entités métier et des Traits (Ports)
//! définissant les contrats que les adaptateurs doivent honorer.

pub mod entities;
pub mod errors;
pub mod ports;

// Ré-exports pour accès direct depuis les crates consommateurs.
pub use entities::actor::{Actor, ActorType, SYSTEM_ACTOR_ID, DEFAULT_REPO_ID};
pub use entities::content_id::ContentId;
pub use entities::merge_request::{MergeRequest, MrStatus, MrReview, MrVerdict, MrEvent, MrEventType, MergeStrategy};
pub use entities::issue::{Issue, IssueStatus, IssueComment, IssueEvent, IssueEventType, IssueLabel};
pub use entities::operation::Operation;
pub use entities::repository::{Repository, Visibility};
pub use errors::DomainError;
