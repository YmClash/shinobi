//! Use Case : Parsing et Validation du jutsu.yml — Phase 40 (Jutsu Runner) 🥷⚡
//!
//! Parse le contenu YAML d'un fichier `jutsu.yml` et valide :
//! - La structure (nom, triggers, stages)
//! - Les champs obligatoires (image, jutsus)
//! - Les dépendances entre stages (requires)
//! - **L'absence de cycles** dans le graphe de dépendances (DFS/coloring)
//!
//! ## Vegapunk Tweak #6 — Séparation Parsing/Exécution
//! La détection de cycles se fait ICI (au parsing), pas dans le RunPipelineUseCase.
//! Si le graphe est acyclique, l'exécuteur peut partir du principe que
//! le graphe est sain et faire un Dynamic Scheduling simple.

use std::collections::{HashMap, HashSet};

use tracing::info;

use domain::entities::jutsu_config::JutsuConfig;
use domain::errors::DomainError;

/// Use case de parsing et validation des fichiers `jutsu.yml`.
pub struct ParseJutsuConfigUseCase;

impl ParseJutsuConfigUseCase {
    pub fn new() -> Self {
        Self
    }

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
    ///
    /// # Errors
    /// `DomainError::BusinessRule` si la validation échoue.
    pub fn parse_and_validate(&self, yaml_content: &str) -> Result<JutsuConfig, DomainError> {
        // ── 1. Désérialisation YAML ──────────────────────────────
        let config: JutsuConfig = serde_yaml::from_str(yaml_content).map_err(|e| {
            DomainError::BusinessRule(format!(
                "jutsu.yml invalide — erreur YAML: {e}"
            ))
        })?;

        // ── 2. Valider le nom ───────────────────────────────────
        if config.name.trim().is_empty() {
            return Err(DomainError::BusinessRule(
                "jutsu.yml: le champ 'name' est obligatoire et ne peut pas être vide".to_string(),
            ));
        }

        // ── 3. Valider les triggers ─────────────────────────────
        if config.on.is_empty() {
            return Err(DomainError::BusinessRule(
                "jutsu.yml: le champ 'on' doit contenir au moins un trigger".to_string(),
            ));
        }

        let valid_triggers: HashSet<&str> =
            ["push", "mr_created", "tag"].iter().copied().collect();
        for trigger in &config.on {
            if !valid_triggers.contains(trigger.as_str()) {
                return Err(DomainError::BusinessRule(format!(
                    "jutsu.yml: trigger invalide '{}'. Valeurs autorisées: push, mr_created, tag",
                    trigger
                )));
            }
        }

        // ── 4. Valider les stages ───────────────────────────────
        if config.stages.is_empty() {
            return Err(DomainError::BusinessRule(
                "jutsu.yml: au moins un stage est requis dans 'stages'".to_string(),
            ));
        }

        let stage_names: HashSet<&str> = config.stages.keys().map(|k| k.as_str()).collect();

        for (name, stage) in &config.stages {
            // 5a. Image non vide
            if stage.image.trim().is_empty() {
                return Err(DomainError::BusinessRule(format!(
                    "jutsu.yml: le stage '{}' doit avoir un champ 'image' non vide",
                    name
                )));
            }

            // 5b. Au moins une commande
            if stage.jutsus.is_empty() {
                return Err(DomainError::BusinessRule(format!(
                    "jutsu.yml: le stage '{}' doit avoir au moins un jutsu (commande)",
                    name
                )));
            }

            // 6. Requires référence des stages existants
            for dep in &stage.requires {
                if !stage_names.contains(dep.as_str()) {
                    return Err(DomainError::BusinessRule(format!(
                        "jutsu.yml: le stage '{}' dépend de '{}' qui n'existe pas",
                        name, dep
                    )));
                }
                // Auto-référence interdite
                if dep == name {
                    return Err(DomainError::BusinessRule(format!(
                        "jutsu.yml: le stage '{}' ne peut pas dépendre de lui-même",
                        name
                    )));
                }
            }
        }

        // ── 7. Cycle detection (DFS/coloring) ───────────────────
        self.detect_cycles(&config)?;

        info!(
            name = %config.name,
            stages = config.stages.len(),
            triggers = ?config.on,
            "📜 jutsu.yml validé avec succès"
        );

        Ok(config)
    }

    /// Détecte les dépendances circulaires dans le graphe de stages.
    ///
    /// Utilise un DFS avec 3 couleurs :
    /// - `White` : pas encore visité
    /// - `Gray` : en cours de visite (dans la pile DFS)
    /// - `Black` : visite terminée (pas de cycle via ce nœud)
    ///
    /// Si on rencontre un nœud `Gray` pendant le DFS, c'est un cycle.
    fn detect_cycles(&self, config: &JutsuConfig) -> Result<(), DomainError> {
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

        // Construire le graphe d'adjacence (stage → ses dépendances)
        let graph: HashMap<&str, Vec<&str>> = config
            .stages
            .iter()
            .map(|(name, stage)| {
                let deps: Vec<&str> = stage.requires.iter().map(|s| s.as_str()).collect();
                (name.as_str(), deps)
            })
            .collect();

        // Pile pour tracer le chemin du cycle
        fn dfs<'a>(
            node: &'a str,
            graph: &HashMap<&str, Vec<&'a str>>,
            colors: &mut HashMap<&'a str, Color>,
            path: &mut Vec<&'a str>,
        ) -> Result<(), String> {
            colors.insert(node, Color::Gray);
            path.push(node);

            if let Some(deps) = graph.get(node) {
                for dep in deps {
                    match colors.get(dep) {
                        Some(Color::Gray) => {
                            // Cycle trouvé ! Construire le message
                            path.push(dep);
                            let cycle_start = path.iter().position(|n| n == dep).unwrap();
                            let cycle: Vec<&str> = path[cycle_start..].to_vec();
                            return Err(format!(
                                "Dépendance circulaire détectée: {}",
                                cycle.join(" → ")
                            ));
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
                dfs(node.as_str(), &graph, &mut colors, &mut path)
                    .map_err(|msg| DomainError::BusinessRule(format!("jutsu.yml: {msg}")))?;
            }
        }

        Ok(())
    }
}
