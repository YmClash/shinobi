//! Module de fédération — Phase 27 + 27-bis + 27-ter ForgeFed.
//!
//! Contient l'infrastructure cryptographique et protocolaire
//! pour la fédération ActivityPub.

pub mod activity_builder;
pub mod crypto;
pub mod delivery;
pub mod fanout_service;
pub mod http_signature;
pub mod remote_actor;
