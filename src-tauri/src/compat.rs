//! Compatibility database (P2).
//!
//! Two things live here that the manifest-only detectors in [`crate::conflicts`]
//! cannot know on their own:
//!
//! 1. **Known-incompatible mod PAIRS** that no `mod.info` declares. PZ has no
//!    central conflict registry, so "these two Workshop mods fight each other"
//!    is crowd-sourced knowledge.
//! 2. **Per-mod game-build ranges** and **required-library constraints**.
//!
//! # Loading is never fatal
//!
//! The primary source is the bundled Tauri resource
//! `resources/compatibility.json`. If it is missing, unreadable or fails to
//! parse, we fall back to a compile-time copy (`compatibility.embedded.json`,
//! pulled in with `include_str!`). If *that* is somehow unusable we degrade to
//! an **empty rule set with `status == Unavailable`** — we never silently
//! report "all clear", because an empty DB and a clean install look identical
//! downstream. `CompatStatus` is surfaced through [`crate::conflicts::analyze_with_compat`]
//! so the UI can render "compatibility data unavailable".

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::sync::OnceLock;

use crate::conflicts::{compare_versions, normalize_id, ConflictKind, ModDiagnostic, Severity};
use crate::load_order::mod_info::ModManifest;

/// The compile-time mirror of `resources/compatibility.json`. Used only when
/// the bundled resource file is missing or corrupt.
pub const EMBEDDED_JSON: &str = include_str!("compatibility.embedded.json");

/// The file name looked up inside the Tauri resource directory.
pub const RESOURCE_FILE_NAME: &str = "compatibility.json";

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompatSeverity {
    Error,
    Warning,
}

impl CompatSeverity {
    pub fn as_conflict_severity(self) -> Severity {
        match self {
            CompatSeverity::Error => Severity::Error,
            CompatSeverity::Warning => Severity::Warning,
        }
    }
}

/// `{"mods": ["a","b"], "reason": "...", "severity": "error"}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncompatiblePairRule {
    pub mods: Vec<String>,
    /// Human explanation. Optional on disk; a rule with no reason still fires
    /// with a generic message, because one entry with a typo'd prose field
    /// must not cost the user the whole catalog.
    #[serde(default)]
    pub reason: String,
    /// Defaults to `error` when the field is absent from the JSON.
    #[serde(default = "default_severity")]
    pub severity: CompatSeverity,
}

fn default_severity() -> CompatSeverity {
    CompatSeverity::Error
}

/// `{"mod": "...", "library": "...", "severity": "...", "reason": "..."}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryConstraint {
    /// Serialized as `"mod"`, matching the JSON schema. `mod` is a Rust
    /// keyword so the field is spelled `mod_id` here.
    #[serde(rename = "mod")]
    pub mod_id: String,
    pub library: String,
    #[serde(default = "default_severity")]
    pub severity: CompatSeverity,
    #[serde(default)]
    pub reason: String,
}

/// `{"mod": "...", "min_game_build": "42.0", "max_game_build": null, ...}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameVersionRange {
    /// Serialized as `"mod"`, matching the JSON schema. `mod` is a Rust
    /// keyword so the field is spelled `mod_id` here.
    #[serde(rename = "mod")]
    pub mod_id: String,
    #[serde(default)]
    pub min_game_build: Option<String>,
    #[serde(default)]
    pub max_game_build: Option<String>,
    #[serde(default = "default_severity")]
    pub severity: CompatSeverity,
    #[serde(default)]
    pub reason: String,
}

/// The whole catalog as parsed from disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompatDatabase {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub updated: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub incompatible_pairs: Vec<IncompatiblePairRule>,
    #[serde(default)]
    pub required_libraries: Vec<LibraryConstraint>,
    #[serde(default)]
    pub game_versions: Vec<GameVersionRange>,
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// Where the active rules came from. `Unavailable` is the important case: the
/// UI must say "compatibility data unavailable", NOT "no problems found".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum CompatStatus {
    /// Loaded from the bundled Tauri resource.
    LoadedResource { rule_count: usize },
    /// Loaded from the compile-time `include_str!` copy.
    LoadedEmbedded { rule_count: usize },
    /// No usable source. `rules` will be empty.
    Unavailable { reason: String },
}

impl CompatStatus {
    pub fn is_available(&self) -> bool {
        !matches!(self, CompatStatus::Unavailable { .. })
    }
}

impl fmt::Display for CompatStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompatStatus::LoadedResource { rule_count } => {
                write!(f, "compatibility data loaded from bundled resource ({} rules)", rule_count)
            }
            CompatStatus::LoadedEmbedded { rule_count } => write!(
                f,
                "compatibility data loaded from built-in fallback ({} rules)",
                rule_count
            ),
            CompatStatus::Unavailable { reason } => {
                write!(f, "compatibility data unavailable: {}", reason)
            }
        }
    }
}

/// Rules plus where they came from. This is what the conflict engine holds.
#[derive(Debug, Clone)]
pub struct CompatRules {
    pub db: CompatDatabase,
    pub status: CompatStatus,
}

impl CompatRules {
    pub fn empty(reason: impl Into<String>) -> Self {
        CompatRules {
            db: CompatDatabase::default(),
            status: CompatStatus::Unavailable {
                reason: reason.into(),
            },
        }
    }

    pub fn is_available(&self) -> bool {
        self.status.is_available()
    }

    pub fn rule_count(&self) -> usize {
        self.db.incompatible_pairs.len()
            + self.db.required_libraries.len()
            + self.db.game_versions.len()
    }
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Parse a catalog from a JSON string. Total: any input yields a value or an
/// error string; never panics, never partially succeeds.
pub fn parse_database(json: &str) -> Result<CompatDatabase, String> {
    let db: CompatDatabase =
        serde_json::from_str(json).map_err(|e| format!("compatibility.json is not valid: {}", e))?;
    if db.schema_version == 0 {
        return Err("compatibility.json has no schema_version".to_string());
    }
    if db.schema_version > SUPPORTED_SCHEMA_VERSION {
        return Err(format!(
            "compatibility.json schema_version {} is newer than this build supports ({})",
            db.schema_version, SUPPORTED_SCHEMA_VERSION
        ));
    }
    Ok(db)
}

/// Highest `schema_version` this build knows how to read.
pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// Load from an explicit path, falling back to the embedded copy when that path
/// is missing or corrupt. Never returns `Err`: the worst case is
/// `CompatStatus::Unavailable`.
pub fn load_from_path(path: &Path) -> CompatRules {
    match std::fs::read_to_string(path) {
        Ok(text) => match parse_database(&text) {
            Ok(db) => {
                let count = db.incompatible_pairs.len()
                    + db.required_libraries.len()
                    + db.game_versions.len();
                CompatRules {
                    db,
                    status: CompatStatus::LoadedResource { rule_count: count },
                }
            }
            Err(e) => load_embedded(&format!("{} ({})", path.display(), e)),
        },
        Err(e) => load_embedded(&format!("{} ({})", path.display(), e)),
    }
}

/// Always parse the compile-time copy. On failure, degrade to empty rules and
/// record why, so callers can surface it instead of reporting "all clear".
pub fn load_embedded(reason: &str) -> CompatRules {
    match parse_database(EMBEDDED_JSON) {
        Ok(db) => {
            let count = db.incompatible_pairs.len()
                + db.required_libraries.len()
                + db.game_versions.len();
            CompatRules {
                db,
                status: CompatStatus::LoadedEmbedded { rule_count: count },
            }
        }
        Err(e) => CompatRules::empty(format!(
            "resource failed ({}) and the built-in fallback failed too ({})",
            reason, e
        )),
    }
}

/// Candidate locations for the bundled resource, most specific first.
///
/// `resource_dir` is the value of `app.path().resource_dir()` when the caller
/// has a Tauri `AppHandle`; the remaining entries let the same code work from
/// `cargo test` and from a plain library consumer.
pub fn candidate_resource_paths(resource_dir: Option<&Path>) -> Vec<std::path::PathBuf> {
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    let mut push = |p: std::path::PathBuf| {
        if !out.contains(&p) {
            out.push(p);
        }
    };

    if let Some(dir) = resource_dir {
        push(dir.join(RESOURCE_FILE_NAME));
        push(dir.join("resources").join(RESOURCE_FILE_NAME));
    }

    // `exe/resources/compatibility.json` is the layout `tauri build` produces.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            push(dir.join(RESOURCE_FILE_NAME));
            push(dir.join("resources").join(RESOURCE_FILE_NAME));
            // `target/debug/../../resources` -> the crate's own resources dir.
            let mut up = dir.to_path_buf();
            for _ in 0..4 {
                match up.parent() {
                    Some(p) => {
                        up = p.to_path_buf();
                        push(up.join("resources").join(RESOURCE_FILE_NAME));
                        push(up.join(RESOURCE_FILE_NAME));
                    }
                    None => break,
                }
            }
        }
    }

    // Compile-time path to the source tree: always correct under `cargo test`.
    push(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(RESOURCE_FILE_NAME));

    out
}

/// Load using every candidate location in order. Always succeeds.
pub fn load_default(resource_dir: Option<&Path>) -> CompatRules {
    let candidates = candidate_resource_paths(resource_dir);
    let mut failures: Vec<String> = Vec::new();
    for p in &candidates {
        if p.is_file() {
            match std::fs::read_to_string(p) {
                Ok(text) => match parse_database(&text) {
                    Ok(db) => {
                        let count = db.incompatible_pairs.len()
                            + db.required_libraries.len()
                            + db.game_versions.len();
                        return CompatRules {
                            db,
                            status: CompatStatus::LoadedResource { rule_count: count },
                        };
                    }
                    Err(e) => failures.push(format!("{}: {}", p.display(), e)),
                },
                Err(e) => failures.push(format!("{}: {}", p.display(), e)),
            }
        }
    }
    let reason = if failures.is_empty() {
        "no compatibility.json found in any known location".to_string()
    } else {
        format!("all compatibility.json copies unusable ({})", failures.join("; "))
    };
    load_embedded(&reason)
}

static GLOBAL_RULES: OnceLock<CompatRules> = OnceLock::new();

/// Process-wide rules, loaded once. In `cargo test` the candidate list resolves
/// to `src-tauri/resources/compatibility.json`.
pub fn global_rules() -> &'static CompatRules {
    GLOBAL_RULES.get_or_init(|| load_default(None))
}

// ---------------------------------------------------------------------------
// Indexing
// ---------------------------------------------------------------------------

/// A parsed catalog indexed by normalized mod id.
///
/// Pair symmetry is enforced at build time: `("a","b")` and `("b","a")` both
/// register the same unordered key, so a rule never depends on declaration
/// order, and a duplicate unordered pair collapses to one rule.
#[derive(Debug, Clone)]
pub struct CompatIndex {
    /// key = (lower_id_a, lower_id_b) with a <= b
    pairs: BTreeMap<(String, String), IncompatiblePairRule>,
    libraries: BTreeMap<String, Vec<LibraryConstraint>>,
    game_versions: BTreeMap<String, Vec<GameVersionRange>>,
}

fn pair_key(a: &str, b: &str) -> (String, String) {
    let na = normalize_id(a);
    let nb = normalize_id(b);
    if na <= nb {
        (na, nb)
    } else {
        (nb, na)
    }
}

impl CompatIndex {
    pub fn build(db: &CompatDatabase) -> CompatIndex {
        let mut pairs: BTreeMap<(String, String), IncompatiblePairRule> = BTreeMap::new();
        for rule in &db.incompatible_pairs {
            let ids: Vec<String> = rule
                .mods
                .iter()
                .map(|m| normalize_id(m))
                .filter(|m| !m.is_empty())
                .collect();
            if ids.len() < 2 {
                continue;
            }
            // An N-mod rule covers every unordered pair inside it.
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    let key = pair_key(&ids[i], &ids[j]);
                    pairs.entry(key).or_insert_with(|| rule.clone());
                }
            }
        }

        let mut libraries: BTreeMap<String, Vec<LibraryConstraint>> = BTreeMap::new();
        for c in &db.required_libraries {
            let k = normalize_id(&c.mod_id);
            if k.is_empty() {
                continue;
            }
            libraries.entry(k).or_default().push(c.clone());
        }

        let mut game_versions: BTreeMap<String, Vec<GameVersionRange>> = BTreeMap::new();
        for g in &db.game_versions {
            let k = normalize_id(&g.mod_id);
            if k.is_empty() {
                continue;
            }
            game_versions.entry(k).or_default().push(g.clone());
        }

        CompatIndex {
            pairs,
            libraries,
            game_versions,
        }
    }

    pub fn pair_count(&self) -> usize {
        self.pairs.len()
    }

    pub fn rule_for_pair(&self, a: &str, b: &str) -> Option<&IncompatiblePairRule> {
        self.pairs.get(&pair_key(a, b))
    }
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

fn display_name(m: &ModManifest) -> String {
    let n = m.name.trim();
    if n.is_empty() {
        m.id.clone()
    } else {
        n.to_string()
    }
}

/// One diagnostic per unordered pair of *installed* mods that the catalog says
/// must not be active together. Output is sorted by (id_a, id_b).
pub fn detect_catalog_pairs(index: &CompatIndex, manifests: &[ModManifest]) -> Vec<ModDiagnostic> {
    let installed: BTreeMap<String, &ModManifest> = manifests
        .iter()
        .filter_map(|m| {
            let k = normalize_id(&m.id);
            if k.is_empty() {
                None
            } else {
                Some((k, m))
            }
        })
        .collect();
    if installed.len() < 2 {
        return Vec::new();
    }

    let mut out: Vec<ModDiagnostic> = Vec::new();
    for ((a, b), rule) in &index.pairs {
        let (Some(ma), Some(mb)) = (installed.get(a), installed.get(b)) else {
            continue;
        };
        // Same mod installed twice is the duplicate detector's job, not ours.
        if normalize_id(&ma.id) == normalize_id(&mb.id) {
            continue;
        }
        let (la, lb) = if a <= b { (ma, mb) } else { (mb, ma) };
        let reason = if rule.reason.trim().is_empty() {
            "These two mods cannot be active at the same time.".to_string()
        } else {
            rule.reason.trim().to_string()
        };
        out.push(ModDiagnostic {
            kind: ConflictKind::IncompatiblePair,
            severity: rule.severity.as_conflict_severity(),
            title: "Two mods that are known to break each other are installed".to_string(),
            cause: format!(
                "{} and {} cannot run together. {}",
                display_name(la),
                display_name(lb),
                reason
            ),
            mod_ids: vec![la.id.clone(), lb.id.clone()],
            related_mod_ids: Vec::new(),
            file_path: None,
            detail: Some(format!("compatibility catalog: {} <-> {}", a, b)),
            suggestion: Some(format!(
                "Switch off {} in the mod list, then rescan to confirm the pair is gone.",
                lb.id
            )),
        });
    }
    out
}

/// Catalog required-library constraints, for mods that are actually installed.
/// When the library IS present but disabled, the manifest-level
/// `detect_missing_dependencies` already reports it, so we skip it here to
/// avoid a duplicate diagnostic.
pub fn detect_catalog_libraries(index: &CompatIndex, manifests: &[ModManifest]) -> Vec<ModDiagnostic> {
    let by_id: BTreeMap<String, &ModManifest> = manifests
        .iter()
        .filter_map(|m| {
            let k = normalize_id(&m.id);
            if k.is_empty() {
                None
            } else {
                Some((k, m))
            }
        })
        .collect();

    let mut out: Vec<ModDiagnostic> = Vec::new();
    for (mod_key, constraints) in &index.libraries {
        let Some(m) = by_id.get(mod_key) else { continue };
        for c in constraints {
            let lib_key = normalize_id(&c.library);
            if lib_key.is_empty() || lib_key == *mod_key {
                continue;
            }
            match by_id.get(&lib_key) {
                // Installed and enabled: nothing to report.
                Some(l) if l.enabled => continue,
                // Installed but disabled: the manifest detector owns this.
                Some(_) => continue,
                None => {}
            }
            let reason = if c.reason.trim().is_empty() {
                String::new()
            } else {
                format!(" {}", c.reason.trim())
            };
            out.push(ModDiagnostic {
                kind: ConflictKind::RequiredLibraryVersion,
                severity: c.severity.as_conflict_severity(),
                title: "A mod is missing a library it needs".to_string(),
                cause: format!(
                    "{} needs {} to work, and it is not installed on this computer.{}",
                    display_name(m),
                    c.library.trim(),
                    reason
                ),
                mod_ids: vec![m.id.clone()],
                related_mod_ids: vec![c.library.trim().to_string()],
                file_path: None,
                detail: Some(format!("compatibility catalog: requires {}", lib_key)),
                suggestion: Some(format!(
                    "Install {} from the Workshop before using {}.",
                    c.library.trim(),
                    m.id
                )),
            });
        }
    }
    out
}

/// Per-mod known game-build ranges. `current_build` is the running game build
/// (`"41.15"`, `"42.0"`, ...). `None` means we do not know it, in which case no
/// version diagnostic is emitted — an unknown build is not a mismatch.
pub fn detect_catalog_game_versions(
    index: &CompatIndex,
    manifests: &[ModManifest],
    current_build: Option<&str>,
) -> Vec<ModDiagnostic> {
    let Some(build) = current_build.map(str::trim).filter(|b| !b.is_empty()) else {
        return Vec::new();
    };

    let mut out: Vec<ModDiagnostic> = Vec::new();
    for m in manifests {
        let key = normalize_id(&m.id);
        if key.is_empty() {
            continue;
        }
        let Some(ranges) = index.game_versions.get(&key) else {
            continue;
        };
        for r in ranges {
            let below_min = r
                .min_game_build
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| compare_versions(build, v) == Ordering::Less)
                .unwrap_or(false);
            let above_max = r
                .max_game_build
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| compare_versions(build, v) == Ordering::Greater)
                .unwrap_or(false);
            if !below_min && !above_max {
                continue;
            }
            let expected = match (
                r.min_game_build.as_deref().map(str::trim).filter(|v| !v.is_empty()),
                r.max_game_build.as_deref().map(str::trim).filter(|v| !v.is_empty()),
            ) {
                (Some(lo), Some(hi)) => format!("between {} and {}", lo, hi),
                (Some(lo), None) => format!("{} or newer", lo),
                (None, Some(hi)) => format!("{} or older", hi),
                (None, None) => "a specific build".to_string(),
            };
            let reason = if r.reason.trim().is_empty() {
                String::new()
            } else {
                format!(" {}", r.reason.trim())
            };
            out.push(ModDiagnostic {
                kind: ConflictKind::GameVersionMismatch,
                severity: r.severity.as_conflict_severity(),
                title: "This mod does not support your game build".to_string(),
                cause: format!(
                    "{} works on {} only, and you are running build {}.{}",
                    display_name(m),
                    expected,
                    build,
                    reason
                ),
                mod_ids: vec![m.id.clone()],
                related_mod_ids: Vec::new(),
                file_path: None,
                detail: Some(format!(
                    "compatibility catalog: supported {} (running {})",
                    expected, build
                )),
                suggestion: Some(format!(
                    "Update {} to a build {}, or switch it off while you stay on {}.",
                    m.id, build, build
                )),
            });
        }
    }
    out
}

/// Everything the catalog knows, given a set of manifests.
pub fn analyze_with_catalog(
    rules: &CompatRules,
    manifests: &[ModManifest],
    current_build: Option<&str>,
) -> Vec<ModDiagnostic> {
    if !rules.is_available() {
        return Vec::new();
    }
    let index = CompatIndex::build(&rules.db);
    let mut out: Vec<ModDiagnostic> = Vec::new();
    out.extend(detect_catalog_pairs(&index, manifests));
    out.extend(detect_catalog_libraries(&index, manifests));
    out.extend(detect_catalog_game_versions(&index, manifests, current_build));
    out
}

#[cfg(test)]
mod tests;