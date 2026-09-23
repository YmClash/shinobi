//! Configuration Jutsu — Structs serde pour `jutsu.yml` 🥷⚡
//!
//! Duplication ciblée des structs de `taijutsu/crates/domain/src/entities/jutsu_config.rs`.
//!
//! ## Pourquoi dupliquer ?
//! ANBU est un crate indépendant, hors du workspace Taijutsu.
//! Le crate `domain` utilise `{ workspace = true }` pour ses deps,
//! ce qui le rend incompatible comme dépendance path directe.
//!
//! ## Garantie de cohérence
//! Le format `jutsu.yml` est stable (défini en Phase 40).
//! Si le format évolue, le crate partagé `jutsu-core` sera créé (V2).
//!
//! ## Source de vérité
//! `taijutsu/crates/domain/src/entities/jutsu_config.rs` (81 lignes)
//!
//! TODO(V2): Extraire en crate partagé `jutsu-core` si drift constaté.

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
    /// est déterminé par le champ `requires` de chaque stage.
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
    #[serde(default)]
    #[allow(dead_code)]
    pub kage_bunshin: bool,
}
