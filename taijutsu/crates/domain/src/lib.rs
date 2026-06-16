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
pub use entities::content_id::ContentId;
pub use entities::operation::Operation;
pub use entities::user::User;
pub use errors::DomainError;
