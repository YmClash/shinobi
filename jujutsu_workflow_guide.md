# 🥷 Guide Jujutsu × Shinobi — Bookmarks & Merge Requests

## Concepts clés

| Git | Jujutsu | Notes |
|-----|---------|-------|
| `branch` | `bookmark` | Un pointeur nommé vers un commit |
| `checkout` | `jj new` | Crée un **nouveau** commit enfant (pas de working copy sale) |
| `commit` | `jj describe` + `jj new` | En jj, chaque changement EST déjà un commit |
| `merge` | `jj merge` ou via Shinobi UI | Fast-forward ou squash |

> [!IMPORTANT]
> En Jujutsu, **il n'y a pas de `checkout`** au sens Git. Vous êtes toujours sur un commit
> de travail. `jj new @` crée un commit enfant vide où vous travaillez ensuite.

---

## Workflow : Feature Branch → Merge Request (sans conflit)

### Étape 1 — Cloner le dépôt

```bash
# Clone via Git protocol (Shinobi expose un endpoint Git HTTP)
jj git clone http://localhost:3000/ymclash/mon-repo.git mon-repo
cd mon-repo
```

### Étape 2 — Vérifier l'état actuel

```bash
# Voir les bookmarks existants
jj bookmark list

# Voir le log (graphe des commits)
jj log
```

Vous verrez quelque chose comme :
```
@  qpvuntsm ymclash 2026-08-13 (empty) (no description set)
│
◉  zzzzzzzz root() 00000000
```

### Étape 3 — Créer un bookmark feature depuis main

```bash
# 1. Se positionner sur main
jj new main

# 2. Créer le bookmark feature sur le commit courant
jj bookmark create feature-login

# Vérifier
jj bookmark list
# main: abc1234
# feature-login: abc1234  ← pointe au même endroit que main pour l'instant
```

### Étape 4 — Travailler sur la feature

```bash
# Éditez vos fichiers normalement (VS Code, vim, etc.)
echo "def login(): pass" > auth.py

# Jujutsu détecte automatiquement les changements !
# Décrivez le commit courant
jj describe -m "feat: add login function"

# Pour commencer un NOUVEAU commit (comme un 2ème commit de feature)
jj new
# Éditez d'autres fichiers...
echo "def logout(): pass" >> auth.py
jj describe -m "feat: add logout function"
```

> [!TIP]
> **Différence fondamentale avec Git** : En Jujutsu, chaque modification est
> automatiquement un commit. Pas besoin de `git add` ou `git commit`.
> `jj describe` ajoute juste le message.

### Étape 5 — Vérifier le graphe avant de push

```bash
jj log
```

Vous devriez voir :
```
@  rlvkpntz ymclash 2026-08-13 feature-login | feat: add logout function
│
◉  qpvuntsm ymclash 2026-08-13 feat: add login function
│
◉  xxxxxxxx ymclash 2026-08-12 main | initial commit
```

### Étape 6 — Mettre le bookmark sur le dernier commit

```bash
# Le bookmark doit pointer sur le DERNIER commit de votre feature
jj bookmark set feature-login
```

### Étape 7 — Push vers Shinobi

```bash
jj git push --bookmark feature-login
```

### Étape 8 — Créer la Merge Request sur Shinobi UI

1. Allez sur `http://localhost:3001/ymclash/mon-repo/mrs/new`
2. **Source** : `feature-login`
3. **Target** : `main`
4. Titre + Description
5. Cliquez **Create**

### Étape 9 — Merger via l'UI

1. Ouvrez la MR
2. (Optionnel) Cliquez **Review → Approve**
3. Cliquez **Merge → Fast-Forward** ou **Squash Merge**

---

## Éviter les conflits — Rebase avant MR

Si `main` a avancé pendant que vous travailliez sur votre feature :

```bash
# 1. Récupérer les dernières modifications
jj git fetch

# 2. Rebaser votre feature sur le dernier main
jj rebase -b feature-login -d main

# 3. Vérifier qu'il n'y a pas de conflits
jj log
jj status

# 4. Si conflits : les fichiers conflictuels sont marqués
#    Éditez-les, puis :
jj describe -m "resolve conflicts"

# 5. Re-push (force automatique en jj)
jj git push --bookmark feature-login
```

---

## Commandes essentielles — Aide-mémoire

| Action | Commande |
|--------|----------|
| **Voir les bookmarks** | `jj bookmark list` |
| **Créer un bookmark** | `jj bookmark create nom-feature` |
| **Se déplacer sur un bookmark** | `jj new main` ou `jj new feature-login` |
| **Décrire le commit courant** | `jj describe -m "message"` |
| **Nouveau commit (continuer à coder)** | `jj new` |
| **Pointer le bookmark sur @** | `jj bookmark set nom-feature` |
| **Voir le diff du commit courant** | `jj diff` |
| **Voir le graphe** | `jj log` |
| **Rebaser sur main** | `jj rebase -b feature -d main` |
| **Récupérer depuis le serveur** | `jj git fetch` |
| **Pousser un bookmark** | `jj git push --bookmark nom` |
| **Restaurer un fichier** | `jj restore --from main chemin/fichier` |
| **Annuler le dernier changement** | `jj undo` |

---

## Workflow visuel

```
main          feature-login
  │
  ◉ initial      
  │               
  │──────────◉ feat: add login      ← jj new main + jj bookmark create
  │          │
  │          ◉ feat: add logout     ← jj new + edit + jj describe
  │          │
  │          ◉ (bookmark set ici)   ← jj bookmark set feature-login
  │          │
  │    push ─┘──→ Shinobi           ← jj git push --bookmark feature-login
  │                  │
  │              MR créée            ← UI: /mrs/new
  │                  │
  ◉──────────────── merge            ← UI: Merge → Fast-Forward
  │
  ◉ main avance
```

---

## FAQ

### Q: Pourquoi mon clone est vide ?
**R** : Après `jj git clone`, le working copy pointe sur un commit vide par défaut.
Faites `jj new main` pour vous positionner sur la branche main.

### Q: Comment voir le contenu d'un bookmark ?
```bash
jj new feature-login   # se positionner dessus
jj diff --from root()  # voir tous les fichiers
```

### Q: Fast-Forward vs Squash ?
- **Fast-Forward** : conserve tous les commits individuels dans l'historique
- **Squash** : compresse tous les commits de la feature en un seul commit propre

### Q: Comment supprimer un bookmark après merge ?
```bash
jj bookmark delete feature-login
jj git push --bookmark feature-login  # propage la suppression au serveur
```
