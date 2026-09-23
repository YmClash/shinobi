//! Parser et validateur `jutsu.yml` — Phase 40-E (ANBU) 🥷⚡
//!
//! Adapté de `taijutsu/crates/application/src/use_cases/parse_jutsu_config.rs`.
//!
//! ## Différences avec Taijutsu
//! - Utilise `anyhow::Error` au lieu de `DomainError`
//! - Pas de `tracing` (output direct vers stderr)
//! - Même logique de validation et détection de cycles
//!
//! ## Validations
//! 1. YAML syntaxiquement correct
//! 2. `name` non vide
//! 3. `on` ≥ 1 trigger, chaque trigger ∈ {"push", "mr_created", "tag"}
//! 4. `stages` ≥ 1 stage
//! 5. Chaque stage : `image` non vide, `jutsus` ≥ 1 commande
//! 6. Chaque `requires` référence un stage existant
//! 7. Pas de dépendance circulaire (cycle detection DFS/coloring)
//!
//! ## Source de vérité
//! `taijutsu/crates/application/src/use_cases/parse_jutsu_config.rs` (213 lignes)
//!
//! TODO(V2): Extraire en crate partagé `jutsu-core`.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};

use super::config::JutsuConfig;

/// Parse et valide un fichier `jutsu.yml`.
///
/// # Validations
/// 1. YAML syntaxiquement correct
/// 2. `name` non vide
/// 3. `on` ≥ 1 trigger, chaque trigger ∈ {"push", "mr_created", "tag"}
/// 4. `stages` ≥ 1 stage
/// 5. Chaque stage : `image` non vide, `jutsus` ≥ 1 commande
/// 6. Chaque `requires` référence un stage existant
/// 7. Pas de dépendance circulaire (cycle detection DFS)
pub fn parse_and_validate(yaml_content: &str) -> Result<JutsuConfig> {
    // ── 1. Désérialisation YAML ──────────────────────────────
    let config: JutsuConfig = serde_yaml::from_str(yaml_content)
        .map_err(|e| anyhow::anyhow!("jutsu.yml invalide — erreur YAML: {e}"))?;

    // ── 2. Valider le nom ───────────────────────────────────
    if config.name.trim().is_empty() {
        bail!("jutsu.yml: le champ 'name' est obligatoire et ne peut pas être vide");
    }

    // ── 3. Valider les triggers ─────────────────────────────
    if config.on.is_empty() {
        bail!("jutsu.yml: le champ 'on' doit contenir au moins un trigger");
    }

    let valid_triggers: HashSet<&str> =
        ["push", "mr_created", "tag"].iter().copied().collect();
    for trigger in &config.on {
        if !valid_triggers.contains(trigger.as_str()) {
            bail!(
                "jutsu.yml: trigger invalide '{}'. Valeurs autorisées: push, mr_created, tag",
                trigger
            );
        }
    }

    // ── 4. Valider les stages ───────────────────────────────
    if config.stages.is_empty() {
        bail!("jutsu.yml: au moins un stage est requis dans 'stages'");
    }

    let stage_names: HashSet<&str> = config.stages.keys().map(|k| k.as_str()).collect();

    for (name, stage) in &config.stages {
        // 5a. Image non vide
        if stage.image.trim().is_empty() {
            bail!("jutsu.yml: le stage '{}' doit avoir un champ 'image' non vide", name);
        }

        // 5b. Au moins une commande
        if stage.jutsus.is_empty() {
            bail!("jutsu.yml: le stage '{}' doit avoir au moins un jutsu (commande)", name);
        }

        // 6. Requires référence des stages existants
        for dep in &stage.requires {
            if !stage_names.contains(dep.as_str()) {
                bail!("jutsu.yml: le stage '{}' dépend de '{}' qui n'existe pas", name, dep);
            }
            // Auto-référence interdite
            if dep == name {
                bail!("jutsu.yml: le stage '{}' ne peut pas dépendre de lui-même", name);
            }
        }
    }

    // ── 7. Cycle detection (DFS/coloring) ───────────────────
    detect_cycles(&config)?;

    Ok(config)
}

/// Détecte les dépendances circulaires dans le graphe de stages.
///
/// Utilise un DFS avec 3 couleurs :
/// - `White` : pas encore visité
/// - `Gray` : en cours de visite (dans la pile DFS)
/// - `Black` : visite terminée (pas de cycle via ce nœud)
fn detect_cycles(config: &JutsuConfig) -> Result<()> {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    let mut colors: HashMap<&str, Color> = config
        .stages
        .keys()
        .map(|k| (k.as_str(), Color::White))
        .collect();

    // Construire le graphe d'adjacence
    let graph: HashMap<&str, Vec<&str>> = config
        .stages
        .iter()
        .map(|(name, stage)| {
            let deps: Vec<&str> = stage.requires.iter().map(|s| s.as_str()).collect();
            (name.as_str(), deps)
        })
        .collect();

    fn dfs<'a>(
        node: &'a str,
        graph: &HashMap<&str, Vec<&'a str>>,
        colors: &mut HashMap<&'a str, Color>,
        path: &mut Vec<&'a str>,
    ) -> Result<()> {
        colors.insert(node, Color::Gray);
        path.push(node);

        if let Some(deps) = graph.get(node) {
            for dep in deps {
                match colors.get(dep) {
                    Some(Color::Gray) => {
                        path.push(dep);
                        let cycle_start = path.iter().position(|n| n == dep).unwrap();
                        let cycle: Vec<&str> = path[cycle_start..].to_vec();
                        bail!(
                            "jutsu.yml: Dépendance circulaire détectée: {}",
                            cycle.join(" → ")
                        );
                    }
                    Some(Color::White) | None => {
                        dfs(dep, graph, colors, path)?;
                    }
                    Some(Color::Black) => {
                        // Déjà traité, pas de cycle via ce nœud
                    }
                }
            }
        }

        path.pop();
        colors.insert(node, Color::Black);
        Ok(())
    }

    let mut path = Vec::new();
    for node in config.stages.keys() {
        if colors.get(node.as_str()) == Some(&Color::White) {
            dfs(node.as_str(), &graph, &mut colors, &mut path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_config() {
        let yaml = r#"
name: "Test Pipeline"
on:
  - push

stages:
  Build:
    image: "rust:1.80-slim"
    jutsus:
      - "cargo build --release"
  Test:
    image: "rust:1.80-slim"
    requires: [Build]
    jutsus:
      - "cargo test"
"#;
        let config = parse_and_validate(yaml).unwrap();
        assert_eq!(config.name, "Test Pipeline");
        assert_eq!(config.stages.len(), 2);
    }

    #[test]
    fn test_empty_name() {
        let yaml = r#"
name: ""
on: [push]
stages:
  Build:
    image: "alpine"
    jutsus: ["echo hi"]
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(err.to_string().contains("name"));
    }

    #[test]
    fn test_invalid_trigger() {
        let yaml = r#"
name: "Test"
on: [invalid_trigger]
stages:
  Build:
    image: "alpine"
    jutsus: ["echo hi"]
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(err.to_string().contains("invalide"));
    }

    #[test]
    fn test_cycle_detection() {
        let yaml = r#"
name: "Cycle Test"
on: [push]
stages:
  A:
    image: "alpine"
    jutsus: ["echo A"]
    requires: [C]
  B:
    image: "alpine"
    jutsus: ["echo B"]
    requires: [A]
  C:
    image: "alpine"
    jutsus: ["echo C"]
    requires: [B]
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(err.to_string().contains("circulaire"));
    }

    #[test]
    fn test_missing_dependency() {
        let yaml = r#"
name: "Test"
on: [push]
stages:
  Build:
    image: "alpine"
    jutsus: ["echo hi"]
    requires: [NonExistent]
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(err.to_string().contains("n'existe pas"));
    }

    #[test]
    fn test_self_reference() {
        let yaml = r#"
name: "Test"
on: [push]
stages:
  Build:
    image: "alpine"
    jutsus: ["echo hi"]
    requires: [Build]
"#;
        let err = parse_and_validate(yaml).unwrap_err();
        assert!(err.to_string().contains("lui-même"));
    }
}
