//! Modèle de configuration `jutsu.yml` — Phase 40 (Jutsu Runner Natif) 🥷⚡
//!
//! Représente la structure d'un fichier `jutsu.yml` à la racine d'un dépôt.
//! Ce fichier définit le pipeline CI/CD natif de Shinobi.
//!
//! ## Exemple
//! ```yaml
//! name: "Genin Pipeline"
//! on:
//!   - push
//!   - mr_created
//!
//! stages:
//!   Suiton_Build:
//!     image: "rust:1.80-slim"
//!     jutsus:
//!       - "cargo build --release"
//!
//!   Katon_Test:
//!     image: "rust:1.80-slim"
//!     requires: [Suiton_Build]
//!     jutsus:
//!       - "cargo test --workspace"
//! ```
//!
//! ## Parsing
//! Utilise `serde_yaml` pour la désérialisation directe.
//! La validation métier (cycles, champs vides) est faite dans
//! `ParseJutsuConfigUseCase`, pas ici.

use std::collections::BTreeMap;

use serde::Deserialize;

/// Configuration complète d'un pipeline Jutsu.
///
/// Correspond au fichier `jutsu.yml` à la racine du dépôt.
/// `BTreeMap` pour `stages` garantit un ordre alphabétique stable
/// des clés (indépendant de l'ordre YAML).
#[derive(Debug, Clone, Deserialize)]
pub struct JutsuConfig {
    /// Nom du pipeline (ex: "Genin Pipeline").
    pub name: String,

    /// Événements déclencheurs (ex: ["push", "mr_created"]).
    pub on: Vec<String>,

    /// Map ordonnée des stages : nom → définition.
    /// L'ordre BTreeMap est alphabétique ; l'ordonnancement réel
    /// est déterminé par le champ `requires` de chaque stage
    /// (Dynamic Scheduling).
    pub stages: BTreeMap<String, JutsuStage>,
}

/// Définition d'un stage Jutsu.
///
/// Chaque stage s'exécute dans un container Docker isolé.
/// Les commandes (`jutsus`) sont exécutées séquentiellement
/// dans un seul `/bin/sh -c "cmd1 && cmd2 && ..."`.
#[derive(Debug, Clone, Deserialize)]
pub struct JutsuStage {
    /// Image Docker OCI (ex: "rust:1.80-slim", "node:20-alpine").
    pub image: String,

    /// Commandes bash à exécuter dans le container.
    /// Enchaînées avec `&&` pour fail-fast.
    pub jutsus: Vec<String>,

    /// Dépendances entre stages (optionnel).
    /// Le stage ne démarre que lorsque tous les stages listés
    /// ici sont terminés avec succès.
    #[serde(default)]
    pub requires: Vec<String>,

    /// Mode IA auto-healing (Phase 41, ignoré en V1).
    /// Quand activé, le runner analysera les logs d'erreur
    /// et tentera une correction automatique.
    #[serde(default)]
    pub kage_bunshin: bool,
}
