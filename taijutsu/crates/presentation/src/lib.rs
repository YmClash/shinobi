//! # SHINOBI — Presentation Layer
//!
//! Adaptateurs primaires de l'architecture hexagonale.
//! Points d'entrée du système pour les consommateurs externes.
//!
//! ## Modules
//! - `rest` — Serveur HTTP/REST via Axum
//! - `grpc` — Protocole **Ninpo** : serveur gRPC via Tonic

pub mod errors;
pub mod grpc;
pub mod rest;
pub mod state;
