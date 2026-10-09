use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::load_order::mod_info::ModManifest;

#[cfg(test)]
mod tests;

/// User-facing severity. Declaration order is the sort order: most severe first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConflictKind {
    MissingDependency,
    LoadOrderViolation,
    CircularDependency,
    IncompatiblePair,
    DuplicateMod,
    GameVersionMismatch,
    RequiredLibraryVersion,
    MalformedVersionDirective,
    DataKeyCollision,
    AssetCollision,
    FileCollision,
}

/// One reported problem, in plain language. Every detector returns these.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDiagnostic {
    pub kind: ConflictKind,
    pub severity: Severity,
    pub title: String,
    /// One plain-language sentence. No jargon, no code paths.
    pub cause: String,
    pub mod_ids: Vec<String>,
    pub related_mod_ids: Vec<String>,
    pub file_path: Option<String>,
    pub detail: Option<String>,
    pub suggestion: Option<String>,
}

/// A single top-level declaration found in a PZ declarative `.txt` script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataKey {
    pub keyword: String,
    pub key: String,
    /// 1-based line number.
    pub line: usize,
}

/// Normalize a mod id the same way `vfs::scan_conflicts` and the dependency
/// resolver do, so diagnostics agree with the existing load-order output.
pub fn normalize_id(raw: &str) -> String {
    raw.trim()
        .to_lowercase()
        .replace('-', "_")
        .replace(' ', "")
}

fn display_name(m: &ModManifest) -> String {
    let n = m.name.trim();
    if n.is_empty() {
        m.id.clone()
    } else {
        n.to_string()
    }
}

/// Exact normalized match first, then prefix match (mirrors `topological_sort`).
fn find_by_id<'a>(manifests: &'a [ModManifest], needle: &str) -> Option<&'a ModManifest> {
    let n = normalize_id(needle);
    if n.is_empty() {
        return None;
    }
    if let Some(m) = manifests.iter().find(|m| normalize_id(&m.id) == n) {
        return Some(m);
    }
    manifests
        .iter()
        .filter(|m| {
            let mid = normalize_id(&m.id);
            !mid.is_empty() && mid.starts_with(&n)
        })
        .min_by_key(|m| normalize_id(&m.id))
}

/// Split a version string into numeric segments. Junk counts as 0.
fn version_segments(v: &str) -> Vec<u64> {
    let s = v.trim();
    let s = s
        .strip_prefix('v')
        .or_else(|| s.strip_prefix('V'))
        .unwrap_or(s);
    if s.is_empty() {
        return Vec::new();
    }
    s.split('.')
        .map(|seg| {
            let digits: String = seg.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u64>().unwrap_or(0)
        })
        .collect()
}

/// Component-wise numeric comparison. `42` == `42.0` == `42.0.0`, and `42.10` > `42.9`.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    match (a.trim().is_empty(), b.trim().is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }
    let pa = version_segments(a);
    let pb = version_segments(b);
    for i in 0..pa.len().max(pb.len()) {
        match pa.get(i).copied().unwrap_or(0).cmp(&pb.get(i).copied().unwrap_or(0)) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    Ordering::Equal
}

pub fn satisfies_min_version(have: &str, required: &str) -> bool {
    compare_versions(have, required) != Ordering::Less
}

/// `versionMin=42` (bare integer) crashes PZ on start; it must be written `42.00`.
pub fn detect_malformed_version_directives(m: &ModManifest) -> Vec<ModDiagnostic> {
    let mut out = Vec::new();
    if let Some(pz) = &m.pzversion {
        let t = pz.trim();
        if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) {
            out.push(ModDiagnostic {
                kind: ConflictKind::MalformedVersionDirective,
                severity: Severity::Warning,
                title: "This mod's version line can crash Project Zomboid".to_string(),
                cause: format!(
                    "{} declares its minimum game version as the bare number {}, with no decimal point. \
                     Project Zomboid reads that as an invalid version and can fail to start with a \
                     NullPointerException. Write it as {}.0 instead.",
                    display_name(m),
                    t,
                    t
                ),
                mod_ids: vec![m.id.clone()],
                related_mod_ids: Vec::new(),
                file_path: None,
                detail: Some(format!("pzversion={}", t)),
                suggestion: Some(format!(
                    "Open {} mod.info and change versionMin={} to versionMin={}.0",
                    m.id, t, t
                )),
            });
        }
    }
    out
}

/// Both directions of `incompatible=`, one diagnostic per unordered pair.
pub fn detect_incompatible_pairs(manifests: &[ModManifest]) -> Vec<ModDiagnostic> {
    let mut pairs: BTreeMap<(String, String), (String, String)> = BTreeMap::new();
    for m in manifests {
        let a = normalize_id(&m.id);
        if a.is_empty() {
            continue;
        }
        for other in &m.incompatible {
            let b = normalize_id(other);
            if b.is_empty() || b == a {
                continue;
            }
            if !manifests.iter().any(|x| normalize_id(&x.id) == b) {
                continue;
            }
            let key = if a <= b {
                (a.clone(), b.clone())
            } else {
                (b.clone(), a.clone())
            };
            let names = (
                m.name.trim().to_string(),
                manifests
                    .iter()
                    .find(|x| normalize_id(&x.id) == b)
                    .map(|x| x.name.trim().to_string())
                    .unwrap_or_default(),
            );
            pairs.entry(key).or_insert(names);
        }
    }

    pairs
        .into_iter()
        .map(|((a, b), (name_a, name_b))| {
            let label_a = if name_a.is_empty() { a.clone() } else { name_a };
            let label_b = if name_b.is_empty() { b.clone() } else { name_b };
            ModDiagnostic {
                kind: ConflictKind::IncompatiblePair,
                severity: Severity::Error,
                title: "Two incompatible mods are installed".to_string(),
                cause: format!(
                    "{} and {} declare each other as incompatible. Loading both together breaks \
                     the game, so only one of them can be active at a time.",
                    label_a, label_b
                ),
                mod_ids: vec![a.clone(), b.clone()],
                related_mod_ids: Vec::new(),
                file_path: None,
                detail: Some(format!("{} <-> {}", a, b)),
                suggestion: Some(format!(
                    "Disable {} in the mod list, then run the dependency sorter to reload the order.",
                    label_b
                )),
            }
        })
        .collect()
}

/// Takes a slice, not a map: the caller's scan currently collapses duplicates
/// into a `HashMap`, so this needs the un-collapsed list to see them at all.
pub fn detect_duplicate_mods(manifests: &[ModManifest]) -> Vec<ModDiagnostic> {
    let mut groups: BTreeMap<String, Vec<&ModManifest>> = BTreeMap::new();
    for m in manifests {
        groups.entry(normalize_id(&m.id)).or_default().push(m);
    }

    groups
        .into_iter()
        .filter(|(id, g)| !id.is_empty() && g.len() > 1)
        .map(|(_id, g)| {
            let versions: Vec<String> = g
                .iter()
                .map(|m| {
                    let v = m.version.as_deref().map(str::trim).unwrap_or("");
                    if v.is_empty() {
                        format!("{} (no version declared)", display_name(m))
                    } else {
                        format!("{} v{}", display_name(m), v)
                    }
                })
                .collect();
            let ids: Vec<String> = g.iter().map(|m| m.id.clone()).collect();
            ModDiagnostic {
                kind: ConflictKind::DuplicateMod,
                severity: Severity::Warning,
                title: "The same mod is installed more than once".to_string(),
                cause: format!(
                    "{} appears {} times in your mod folders. Only one copy is actually used, \
                     and which one wins is decided by load order rather than by you.",
                    display_name(g[0]),
                    g.len()
                ),
                mod_ids: ids,
                related_mod_ids: Vec::new(),
                file_path: None,
                detail: Some(versions.join(" | ")),
                suggestion: Some(format!(
                    "Keep one copy of {} and remove the other from your Workshop and local mods folders.",
                    display_name(g[0])
                )),
            }
        })
        .collect()
}

/// Only `require` is checked, matching the existing sorter's behavior.
pub fn detect_missing_dependencies(manifests: &[ModManifest]) -> Vec<ModDiagnostic> {
    let mut out: Vec<ModDiagnostic> = Vec::new();
    for m in manifests {
        for req in &m.require {
            let nr = normalize_id(req);
            if nr.is_empty() || nr == normalize_id(&m.id) {
                continue;
            }
            match find_by_id(manifests, req) {
                Some(target) if !target.enabled => out.push(ModDiagnostic {
                    kind: ConflictKind::MissingDependency,
                    severity: Severity::Warning,
                    title: "A required library is switched off".to_string(),
                    cause: format!(
                        "{} needs {} to run, but that library is installed and currently disabled. \
                         Anything using {} will break while it stays off.",
                        display_name(m),
                        display_name(target),
                        display_name(m)
                    ),
                    mod_ids: vec![m.id.clone()],
                    related_mod_ids: vec![target.id.clone()],
                    file_path: None,
                    detail: Some(format!("require={}", req.trim())),
                    suggestion: Some(format!("Switch on {} in the mod list.", target.id)),
                }),
                Some(_) => {}
                None => out.push(ModDiagnostic {
                    kind: ConflictKind::MissingDependency,
                    severity: Severity::Error,
                    title: "A required mod is missing".to_string(),
                    cause: format!(
                        "{} needs {}, but that mod is not installed anywhere on this computer. \
                         Without it the game can crash while loading.",
                        display_name(m),
                        req.trim()
                    ),
                    mod_ids: vec![m.id.clone()],
                    related_mod_ids: vec![req.trim().to_string()],
                    file_path: None,
                    detail: Some(format!("require={}", req.trim())),
                    suggestion: Some(format!(
                        "Install {} from the Workshop, then rescan your mods.",
                        req.trim()
                    )),
                }),
            }
        }
    }
    out
}

struct Tarjan {
    index: Vec<usize>,
    low: Vec<usize>,
    on_stack: Vec<bool>,
    stack: Vec<usize>,
    next_index: usize,
    result: Vec<Vec<usize>>,
}

impl Tarjan {
    fn visit(&mut self, v: usize, adj: &[Vec<usize>]) {
        self.index[v] = self.next_index;
        self.low[v] = self.next_index;
        self.next_index += 1;
        self.stack.push(v);
        self.on_stack[v] = true;

        for &w in &adj[v] {
            if self.index[w] == usize::MAX {
                self.visit(w, adj);
                self.low[v] = self.low[v].min(self.low[w]);
            } else if self.on_stack[w] {
                self.low[v] = self.low[v].min(self.index[w]);
            }
        }

        if self.low[v] == self.index[v] {
            let mut component = Vec::new();
            while let Some(w) = self.stack.pop() {
                self.on_stack[w] = false;
                component.push(w);
                if w == v {
                    break;
                }
            }
            let self_loop = component.len() == 1 && adj[component[0]].contains(&component[0]);
            if component.len() > 1 || self_loop {
                self.result.push(component);
            }
        }
    }
}

/// Real cycle membership via Tarjan SCC, replacing the old length heuristic
/// that could tell you a cycle existed but not which mods were in it.
pub fn find_dependency_cycles(manifests: &[ModManifest]) -> Vec<Vec<String>> {
    let mut ids: Vec<String> = manifests.iter().map(|m| normalize_id(&m.id)).collect();
    ids.sort();
    ids.dedup();
    let index: HashMap<String, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, s)| (s.clone(), i))
        .collect();

    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); ids.len()];
    for m in manifests {
        let u = match index.get(&normalize_id(&m.id)) {
            Some(&u) => u,
            None => continue,
        };
        let mut deps: Vec<usize> = Vec::new();
        for dep in m.require.iter().chain(m.load_mod_after.iter()) {
            let nd = normalize_id(dep);
            // Self-edges are kept here on purpose: a mod requiring itself is a
            // real one-node cycle. (Missing-dependency skips them separately.)
            if nd.is_empty() {
                continue;
            }
            let resolved = index.get(&nd).copied().or_else(|| {
                ids.iter()
                    .filter(|id| id.starts_with(&nd))
                    .min()
                    .and_then(|id| index.get(id).copied())
            });
            if let Some(v) = resolved {
                if !deps.contains(&v) {
                    deps.push(v);
                }
            }
        }
        deps.sort_unstable();
        adj[u] = deps;
    }

    let mut t = Tarjan {
        index: vec![usize::MAX; ids.len()],
        low: vec![0; ids.len()],
        on_stack: vec![false; ids.len()],
        stack: Vec::new(),
        next_index: 0,
        result: Vec::new(),
    };
    for v in 0..ids.len() {
        if t.index[v] == usize::MAX {
            t.visit(v, &adj);
        }
    }

    let mut out: Vec<Vec<String>> = t
        .result
        .into_iter()
        .map(|comp| {
            let mut names: Vec<String> = comp.into_iter().map(|i| ids[i].clone()).collect();
            names.sort();
            names
        })
        .collect();
    out.sort();
    out
}

pub fn cycles_to_diagnostics(cycles: &[Vec<String>]) -> Vec<ModDiagnostic> {
    cycles
        .iter()
        .filter(|c| !c.is_empty())
        .map(|c| ModDiagnostic {
            kind: ConflictKind::CircularDependency,
            severity: Severity::Error,
            title: "A group of mods cannot be ordered".to_string(),
            cause: format!(
                "{} all need each other to load first, so no valid load order exists for them. \
                 The sorter cannot place any of them and skips the whole group.",
                c.join(", ")
            ),
            mod_ids: c.clone(),
            related_mod_ids: Vec::new(),
            file_path: None,
            detail: Some(format!("cycle: {}", c.join(" -> "))),
            suggestion: Some(
                "Remove one of the `require=` lines from the mod.info files to break the loop."
                    .to_string(),
            ),
        })
        .collect()
}

const DECLARATION_KEYWORDS: &[&str] = &[
    "craftRecipe",
    "spawnregion",
    "distribution",
    "profession",
    "world_item",
    "container",
    "magazine",
    "newspaper",
    "moveable",
    "particle",
    "recipe",
    "vehicle",
    "clothing",
    "tiledef",
    "object",
    "weapon",
    "trait",
    "sound",
    "light",
    "book",
    "note",
    "item",
    "food",
];

fn match_keyword(line: &str) -> Option<&'static str> {
    DECLARATION_KEYWORDS.iter().copied().find(|kw| {
        line.starts_with(kw)
            && line[kw.len()..]
                .chars()
                .next()
                .map(char::is_whitespace)
                .unwrap_or(false)
    })
}

/// First name-like token after the keyword, skipping `=`, `{` and quotes noise.
fn first_key_token(rest: &str) -> Option<String> {
    let trimmed = rest.trim_start();
    // A quoted name may contain spaces, so it must be read before splitting.
    if let Some(quote) = trimmed.chars().next().filter(|c| *c == '\'' || *c == '"') {
        let quoted: String = trimmed.chars().skip(1).take_while(|c| *c != quote).collect();
        if !quoted.is_empty() {
            return Some(quoted);
        }
    }
    for raw in trimmed.split_whitespace() {
        let mut t = raw;
        if let Some(eq) = t.find('=') {
            if eq > 0 {
                t = &t[..eq];
            } else {
                continue;
            }
        }
        t = t.trim_end_matches(|c| c == '{' || c == '}' || c == ',' || c == ';');
        let t = t.trim_matches(|c| c == '\'' || c == '"').trim();
        if t.is_empty() || t == "{" || t == "}" || t == "=" {
            continue;
        }
        return Some(t.to_string());
    }
    None
}

fn brace_delta(line: &str) -> i32 {
    let (mut delta, mut in_single, mut in_double) = (0i32, false, false);
    for c in line.chars() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '{' if !in_single && !in_double => delta += 1,
            '}' if !in_single && !in_double => delta -= 1,
            _ => {}
        }
    }
    delta
}

/// Top-level declarations in a PZ declarative `.txt`. Tolerant by design: this
/// runs on real-world broken mod files, so it never panics.
pub fn extract_declared_keys(content: &str) -> Vec<DataKey> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        // Comment lines are skipped entirely, including their braces: a commented
        // `item Foo {` must not push the next line into "nested property" depth.
        if line.starts_with('#') || line.starts_with("--") {
            continue;
        }
        if depth == 0 && !line.is_empty() {
            if let Some(kw) = match_keyword(line) {
                if let Some(key) = first_key_token(&line[kw.len()..]) {
                    out.push(DataKey {
                        keyword: kw.to_string(),
                        key,
                        line: idx + 1,
                    });
                }
            }
        }
        depth = (depth + brace_delta(raw_line)).max(0);
    }
    out
}

/// Same `(keyword, key)` declared by two different mods. A mod repeating its
/// own key, or two different keywords sharing a name, are both fine.
pub fn detect_data_key_collisions(groups: &[(String, Vec<DataKey>)]) -> Vec<ModDiagnostic> {
    let mut by_key: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for (mod_id, keys) in groups {
        let mid = mod_id.trim();
        if mid.is_empty() {
            continue;
        }
        for k in keys {
            if k.key.trim().is_empty() {
                continue;
            }
            by_key
                .entry((k.keyword.clone(), k.key.clone()))
                .or_default()
                .insert(mid.to_string());
        }
    }

    by_key
        .into_iter()
        .filter(|(_, mods)| mods.len() > 1)
        .map(|((keyword, key), mods)| {
            let ids: Vec<String> = mods.iter().cloned().collect();
            ModDiagnostic {
                kind: ConflictKind::DataKeyCollision,
                severity: Severity::Warning,
                title: "Two mods define something with the same name".to_string(),
                cause: format!(
                    "{} and {} both define a {} called {}. Only one definition survives loading, \
                     so one mod will silently lose it.",
                    ids[0], ids[1], keyword, key
                ),
                mod_ids: ids,
                related_mod_ids: Vec::new(),
                file_path: None,
                detail: Some(format!("{} {}", keyword, key)),
                suggestion: Some(format!(
                    "Rename the {} in one of the two mods, or merge them with the 3-way merger.",
                    key
                )),
            }
        })
        .collect()
}

/// Runs every manifest-level detector. Output ordering is left to the caller.
pub fn analyze(manifests: &[ModManifest]) -> Vec<ModDiagnostic> {
    let mut out: Vec<ModDiagnostic> = Vec::new();
    for m in manifests {
        out.extend(detect_malformed_version_directives(m));
    }
    out.extend(detect_missing_dependencies(manifests));
    out.extend(detect_incompatible_pairs(manifests));
    out.extend(detect_duplicate_mods(manifests));
    out.extend(cycles_to_diagnostics(&find_dependency_cycles(manifests)));
    out
}

/// The full report: every manifest-level detector from [`analyze`] plus the
/// crowd-sourced catalog from [`crate::compat`], and the catalog's load status
/// so the UI can say "compatibility data unavailable" instead of implying a
/// clean install.
///
/// `current_build` is the running game build if known; `None` simply skips the
/// version-range detector (an unknown build is not a mismatch).
pub fn analyze_with_compat(
    manifests: &[ModManifest],
    rules: &crate::compat::CompatRules,
    current_build: Option<&str>,
) -> Vec<ModDiagnostic> {
    let mut out = analyze(manifests);
    out.extend(crate::compat::analyze_with_catalog(rules, manifests, current_build));
    out
}

/// [`analyze_with_compat`] against the process-wide cached catalog.
pub fn analyze_with_global_compat(
    manifests: &[ModManifest],
    current_build: Option<&str>,
) -> (Vec<ModDiagnostic>, crate::compat::CompatStatus) {
    let rules = crate::compat::global_rules();
    (
        analyze_with_compat(manifests, rules, current_build),
        rules.status.clone(),
    )
}