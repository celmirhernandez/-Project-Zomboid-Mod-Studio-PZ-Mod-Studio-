//! Safe fix engine (P3).
//!
//! The engine has exactly one job beyond "write a file": **never write a file
//! whose content differs from what the user was shown.**
//!
//! # The safety property
//!
//! [`preview_mod_fixes`] is a **dry run**. It reads state, computes a concrete
//! list of operations (this path, these exact bytes), and hashes them into a
//! `plan_id`. It performs **no writes at all** outside of nothing — it does not
//! touch mod folders, does not touch the load-order files, and does not create
//! the backup directory. Nothing on disk changes during a preview.
//!
//! [`apply_mod_fix`] does not trust the client. It **recomputes the plan from
//! freshly read state** and compares hashes. If a single byte of any target
//! file, or any mod's enabled set, has drifted since the preview, the hash
//! differs and the apply is **refused** with [`FixErrorCode::PlanStale`]. The
//! bytes that get written always come from the freshly recomputed plan, which by
//! definition matches the `plan_id` the caller presented.
//!
//! # Backups
//!
//! Every mutation is backed up **before** it is written, into
//! `<user_zomboid_dir>/PZModStudio_Backups/<backup_id>/`. That directory is
//! outside every mod folder and outside the load-order files, so a restore can
//! never write a mod's own content into a mod's own directory. The write order
//! is: copy original bytes to the backup dir, then swap the new bytes in.
//!
//! # Deletion
//!
//! The engine does not delete anything. Removing a mod's files is not a fix, it
//! is data loss, so [`FixKind::DisableMod`] is generated instead — it takes a
//! mod out of the active load order, which is reversible by re-adding one id.
//! Every generated change rewrites a file that **already exists**, so every
//! generated change is reversible by construction.
//!
//! # Determinism
//!
//! Changes are sorted by target path, affected mod ids are sorted, and the hash
//! is computed over that sorted form. The same inputs always produce the same
//! `plan_id`, `change_id`s and `backup_id`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::conflicts::{normalize_id, ConflictKind, ModDiagnostic, Severity};
use crate::load_order::ini_parser::{
    parse_default_txt_text, parse_ini_text, parse_plain_list_text, render_default_txt, render_ini,
    render_plain_list, LoadOrderFormat,
};
use crate::load_order::mod_info::{get_all_user_zomboid_dirs, ModManifest};
use crate::load_order::topological_sort::sort_dependencies_topologically;
use walkdir::WalkDir;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixKind {
    ReorderLoadOrder,
    DisableMod,
    ReenableMod,
    ApplyMerge,
    PatchFile,
}

/// One previewed operation. This is what the UI renders as a checkbox row.
///
/// `backup_path` is populated at preview time with the path the backup **will**
/// take; the bytes are only written there when the change is applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedChange {
    pub change_id: String,
    pub kind: FixKind,
    pub target_path: String,
    pub description: String,
    pub affected_mod_ids: Vec<String>,
    pub reversible: bool,
    pub backup_path: Option<String>,
    pub diff_preview: Option<String>,
}

/// A dry-run plan. `plan_id` binds `changes` to the exact bytes they will write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixPlan {
    pub plan_id: String,
    pub summary: String,
    pub changes: Vec<PlannedChange>,
    pub requires_confirmation: bool,
    pub affected_files: usize,
}

/// One backed-up original file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEntry {
    pub backup_id: String,
    pub created_at_unix: u64,
    pub original_path: String,
    pub backup_path: String,
    pub size_bytes: u64,
    /// `"fix_apply"` for backups created by this engine.
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyResult {
    pub applied: Vec<String>,
    pub skipped: Vec<SkippedChange>,
    pub backups: Vec<BackupEntry>,
    /// Each entry begins with one of [`FixErrorCode`] so the UI can branch on
    /// it instead of string-matching prose.
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedChange {
    pub change_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    pub restored: Vec<String>,
    pub errors: Vec<String>,
}

/// Machine-readable refusal reasons. The apply command has a pinned return type
/// of [`ApplyResult`] (no `Result`), so a refusal is reported as a
/// [`FixErrorCode`] at the front of `ApplyResult.errors` and a
/// [`SkippedChange`] for every change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixErrorCode {
    /// State drifted since the preview. Re-preview.
    PlanStale,
    /// The presented `plan_id` has never existed / matches nothing on disk.
    UnknownPlan,
    /// `confirm_token` did not echo the `plan_id`.
    ConfirmationRequired,
    /// Nothing to do.
    NothingToDo,
    /// Filesystem refused. Detail is in the rest of the message.
    Io,
}

impl FixErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            FixErrorCode::PlanStale => "PLAN_STALE",
            FixErrorCode::UnknownPlan => "UNKNOWN_PLAN",
            FixErrorCode::ConfirmationRequired => "CONFIRMATION_REQUIRED",
            FixErrorCode::NothingToDo => "NOTHING_TO_DO",
            FixErrorCode::Io => "IO_ERROR",
        }
    }
}

fn fail(code: FixErrorCode, detail: &str) -> String {
    format!("{}: {}", code.as_str(), detail)
}

// ---------------------------------------------------------------------------
// Hashing
// ---------------------------------------------------------------------------

/// Non-cryptographic but wide change detector: four FNV-1a-64 passes with
/// different offset bases, concatenated. Not a security primitive — its job is
/// to make "any byte of any planned write changed" change the id, which it does
/// with overwhelming probability.
///
/// Every field is length-prefixed so `("ab","c")` cannot hash like `("a","bc")`.
fn hash_fields(fields: &[&[u8]]) -> String {
    const BASES: [u64; 4] = [
        0xcbf2_9ce4_8422_2325,
        0x9e37_79b9_7f4a_7c15,
        0x0000_0000_0000_0001,
        0xff51_afd7_ed55_8ccd,
    ];
    let mut out = String::with_capacity(32);
    for base in BASES {
        let mut h = base;
        for f in fields {
            h ^= f.len() as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
            for &b in *f {
                h ^= b as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        }
        out.push_str(&format!("{:016x}", h));
    }
    out
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// Backups never live inside a mod folder and never shadow a load-order file.
const BACKUP_DIR_NAME: &str = "PZModStudio_Backups";
const BACKUP_MANIFEST_NAME: &str = "manifest.json";
const BACKUP_SOURCE: &str = "fix_apply";

/// The one Zomboid folder this engine is allowed to write to.
///
/// Deliberately narrower than [`get_all_user_zomboid_dirs`], which always
/// appends `~/Zomboid` and `%USERPROFILE%/Zomboid`. A fix engine that silently
/// rewrote a second directory the user never pointed at would be a surprise;
/// an explicit `user_zomboid_dir` wins, and only when it is empty do we fall
/// back to the first auto-detected directory.
pub fn primary_zomboid_dir(user_zomboid_dir: &str) -> PathBuf {
    let clean = user_zomboid_dir.trim();
    if !clean.is_empty() {
        return PathBuf::from(clean);
    }
    get_all_user_zomboid_dirs("")
        .into_iter()
        .next()
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn backup_root(user_zomboid_dir: &str) -> PathBuf {
    primary_zomboid_dir(user_zomboid_dir).join(BACKUP_DIR_NAME)
}

/// The load-order files the engine is willing to rewrite, as
/// `(relative path, format)`. Only files that already exist are ever touched,
/// so every generated change is reversible.
fn load_order_candidates() -> Vec<(&'static str, LoadOrderFormat)> {
    vec![
        ("mods.txt", LoadOrderFormat::PlainList),
        ("mods/default.txt", LoadOrderFormat::DefaultTxt),
        ("Lua/ModListData.ini", LoadOrderFormat::Ini),
        ("Lua/loadorder.ini", LoadOrderFormat::Ini),
        ("Lua/modgroups.ini", LoadOrderFormat::Ini),
    ]
}

/// The ini section+key used to re-render each ini-format load-order file.
fn ini_shape(path: &Path) -> (&'static str, &'static str) {
    match path.file_name().and_then(|n| n.to_str()) {
        Some("loadorder.ini") => ("LoadOrder", "mods"),
        Some("modgroups.ini") => ("ModGroups", "active"),
        _ => ("ModList", "activeMods"),
    }
}

fn render_list(format: LoadOrderFormat, path: &Path, mods: &[String]) -> String {
    match format {
        LoadOrderFormat::PlainList => render_plain_list(mods),
        LoadOrderFormat::DefaultTxt => render_default_txt(mods),
        LoadOrderFormat::Ini => {
            let (section, key) = ini_shape(path);
            render_ini(section, key, mods)
        }
    }
}

fn parse_list(format: LoadOrderFormat, content: &str) -> Vec<String> {
    match format {
        LoadOrderFormat::PlainList => parse_plain_list_text(content),
        LoadOrderFormat::DefaultTxt => parse_default_txt_text(content),
        LoadOrderFormat::Ini => parse_ini_text(content),
    }
}

// ---------------------------------------------------------------------------
// Locating a mod's mod.info
// ---------------------------------------------------------------------------

/// Find the `mod.info` that declares `mod_id`, newest version folder first.
///
/// Returns `None` rather than guessing. A patch that rewrites the wrong
/// `mod.info` is worse than no patch, and "no patch" is a plan with one fewer
/// change in it.
fn locate_mod_info(z_dir: &Path, mod_id: &str) -> Option<PathBuf> {
    let wanted = normalize_id(mod_id);
    if wanted.is_empty() {
        return None;
    }
    let mods_root = z_dir.join("mods");
    if !mods_root.is_dir() {
        return None;
    }

    // Deterministic order: WalkDir with sort_by_file_name, root mod.info first.
    let mut candidates: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(&mods_root)
        .max_depth(8)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_name() == "mod.info" {
            candidates.push(entry.path().to_path_buf());
        }
    }

    // Root `mod.info` (lowest version priority) before version subfolders, then
    // descending file name, so the same tree always yields the same pick.
    let mut scored: Vec<(u32, PathBuf)> = candidates
        .into_iter()
        .map(|p| {
            let score = crate::load_order::mod_info::get_mod_info_version_score(&p);
            (score, p)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    for (_, p) in scored {
        if let Some(manifest) = crate::load_order::mod_info::parse_mod_info(&p) {
            if normalize_id(&manifest.id) == wanted {
                return Some(p);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Plan construction (pure over the inputs it is given)
// ---------------------------------------------------------------------------

/// A concrete operation. `bytes` is exactly what will be written — never
/// recomputed at apply time from a different code path.
#[derive(Debug, Clone)]
struct Operation {
    change_id: String,
    kind: FixKind,
    target_path: PathBuf,
    description: String,
    affected_mod_ids: Vec<String>,
    /// Exact bytes to write. `None` means "no change needed"; such operations
    /// are dropped before hashing.
    bytes: Option<Vec<u8>>,
    /// Bytes the file holds right now. Recorded so a preview can show a real
    /// diff and so the backup copy is known to be complete.
    original: Vec<u8>,
    diff_preview: Option<String>,
}

impl Operation {
    fn to_planned(&self, backup_id: &str) -> PlannedChange {
        PlannedChange {
            change_id: self.change_id.clone(),
            kind: self.kind,
            target_path: self.target_path.to_string_lossy().to_string(),
            description: self.description.clone(),
            affected_mod_ids: self.affected_mod_ids.clone(),
            // Always true: every generated operation rewrites a file that
            // already exists, and the original bytes are in the backup dir.
            reversible: true,
            backup_path: Some(
                PathBuf::from(backup_id)
                    .join(backup_file_name(&self.target_path))
                    .to_string_lossy()
                    .to_string(),
            ),
            diff_preview: self.diff_preview.clone(),
        }
    }
}

fn short_hash(bytes: &[u8]) -> String {
    hash_fields(&[b"short", bytes])[..16].to_string()
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A minimal unified-style preview of just the changed lines.
///
/// Equal lines are skipped, not treated as "nothing changed" — a one-line edit
/// in the middle of a ten-line file must still produce a preview.
fn text_diff(path: &Path, original: &str, updated: &str) -> Option<String> {
    use similar::TextDiff;
    let d = TextDiff::from_lines(original, updated);
    let mut out = String::new();
    let mut changed = 0usize;
    for entry in d.iter_all_changes() {
        let sign = match entry.tag() {
            similar::ChangeTag::Equal => continue,
            similar::ChangeTag::Delete => '-',
            similar::ChangeTag::Insert => '+',
        };
        changed += 1;
        if changed > 40 {
            out.push_str("... (preview truncated)\n");
            break;
        }
        out.push(sign);
        out.push_str(entry.value().trim_end_matches(['\r', '\n']));
        out.push('\n');
    }
    if changed == 0 {
        return None;
    }
    Some(format!("--- {}\n{}", path.to_string_lossy(), out))
}

/// Everything the planner needs, gathered once from disk.
pub struct PlanInputs {
    pub z_dir: PathBuf,
    pub manifests: Vec<ModManifest>,
    pub diagnostics: Vec<ModDiagnostic>,
}

fn build_operations(inputs: &PlanInputs) -> Vec<Operation> {
    let mut ops: Vec<Operation> = Vec::new();
    let z_dir = &inputs.z_dir;

    // --- Work out the desired active-mod list ---------------------------------
    let active_now = read_authoritative_active_list(z_dir);
    // An empty active list is treated as "unknown", not as "disable everything":
    // if no load-order file parsed, the engine has nothing to plan against and
    // must not invent a list and write it over the user's files.
    let load_order_is_known = !active_now.is_empty();

    // Mods to switch off: every error-severity incompatible pair.
    let mut to_disable: Vec<(String, String)> = Vec::new(); // (kept, disabled)
    for d in &inputs.diagnostics {
        if d.kind != ConflictKind::IncompatiblePair || d.severity != Severity::Error {
            continue;
        }
        if d.mod_ids.len() < 2 {
            continue;
        }
        if let Some((keep, drop_id)) = pick_survivor(&inputs.manifests, &d.mod_ids) {
            to_disable.push((keep, drop_id));
        }
    }

    // Mods to switch back on: a required library that is installed but off.
    let mut to_reenable: BTreeSet<String> = BTreeSet::new();
    for d in &inputs.diagnostics {
        if d.kind != ConflictKind::MissingDependency || d.severity != Severity::Warning {
            continue;
        }
        for rel in &d.related_mod_ids {
            let norm = normalize_id(rel);
            if norm.is_empty() {
                continue;
            }
            let is_installed = inputs
                .manifests
                .iter()
                .any(|m| normalize_id(&m.id) == norm);
            let is_active = active_now.iter().any(|a| normalize_id(a) == norm);
            if is_installed && !is_active {
                to_reenable.insert(matching_id(&inputs.manifests, rel));
            }
        }
    }

    let desired = reconcile_active_list(&active_now, &to_disable, &to_reenable, &inputs.manifests);

    // --- Load-order file rewrites --------------------------------------------
    if load_order_is_known && desired != active_now {
        let kind = if !to_disable.is_empty() {
            FixKind::DisableMod
        } else if !to_reenable.is_empty() {
            FixKind::ReenableMod
        } else {
            FixKind::ReorderLoadOrder
        };

        let mut affected: Vec<String> = to_disable.iter().map(|(_, d)| d.clone()).collect();
        affected.extend(to_reenable.iter().cloned());
        affected.sort();
        affected.dedup();

        for (rel, format) in load_order_candidates() {
            let path = z_dir.join(rel);
            // Only rewrite files that already exist: this is what makes every
            // change reversible and keeps deletion out of the engine.
            let original = match std::fs::read(&path) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let original_text = String::from_utf8_lossy(&original).to_string();
            // If this particular file parses to nothing while the authoritative
            // list does not, its syntax is not what we assume (or the user
            // emptied it on purpose). Overwriting it with content derived from
            // a *different* file would destroy information we do not have.
            if parse_list(format, &original_text).is_empty() {
                continue;
            }
            let updated_text = render_list(format, &path, &desired);
            let bytes = updated_text.clone().into_bytes();
            if bytes == original {
                continue;
            }
            ops.push(Operation {
                change_id: format!("chg_loadorder_{}", rel.replace(['/', '\\'], "_")),
                kind,
                target_path: path.clone(),
                description: describe_load_order_change(rel, &to_disable, &to_reenable),
                affected_mod_ids: affected.clone(),
                bytes: Some(bytes),
                original,
                diff_preview: text_diff(&path, &original_text, &updated_text),
            });
        }
    }

    // --- mod.info version-directive patches ----------------------------------
    for d in &inputs.diagnostics {
        if d.kind != ConflictKind::MalformedVersionDirective {
            continue;
        }
        for mod_id in &d.mod_ids {
            let Some(info) = locate_mod_info(z_dir, mod_id) else {
                continue;
            };
            let Ok(original) = std::fs::read(&info) else {
                continue;
            };
            let Ok(text) = String::from_utf8(original.clone()) else {
                continue;
            };
            let fixed = patch_bare_integer_version(&text);
            let Some(fixed) = fixed else { continue };
            if fixed == text {
                continue;
            }
            ops.push(Operation {
                change_id: format!("chg_modinfo_{}", short_hash(info.to_string_lossy().as_bytes())),
                kind: FixKind::PatchFile,
                target_path: info.clone(),
                description: format!(
                    "Write {} as a decimal version so Project Zomboid can read it.",
                    "versionMin"
                ),
                affected_mod_ids: vec![mod_id.clone()],
                bytes: Some(fixed.clone().into_bytes()),
                original,
                diff_preview: text_diff(&info, &text, &fixed),
            });
        }
    }

    // Deterministic order: by target path, then change id.
    ops.sort_by(|a, b| {
        a.target_path
            .cmp(&b.target_path)
            .then_with(|| a.change_id.cmp(&b.change_id))
    });
    ops
}

/// `versionMin=42` -> `versionMin=42.0`. Only the bare-integer form is touched;
/// anything already decimal or non-numeric is left exactly as it was.
fn patch_bare_integer_version(text: &str) -> Option<String> {
    const KEYS: [&str; 2] = ["versionMin=", "pzversion="];

    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    for line in text.split_inclusive('\n') {
        // Split the line into [leading indent][key=value][line ending] without
        // rebuilding it from slices, so no byte can be duplicated or dropped.
        let eol_len = if line.ends_with("\r\n") {
            2
        } else if line.ends_with('\n') {
            1
        } else {
            0
        };
        let body = &line[..line.len() - eol_len];
        let eol = &line[line.len() - eol_len..];

        let indent_len = body.len() - body.trim_start().len();
        let (indent, rest) = body.split_at(indent_len);
        let value_start = rest.find('=').map(|i| i + 1).unwrap_or(0);
        let (head, value) = rest.split_at(value_start);
        let value_trimmed = value.trim();

        let is_target_key = KEYS.iter().any(|k| head.trim() == *k);
        let is_bare_integer = !value_trimmed.is_empty()
            && value_trimmed.chars().all(|c| c.is_ascii_digit());

        if is_target_key && is_bare_integer {
            out.push_str(indent);
            out.push_str(head);
            out.push_str(value_trimmed);
            out.push_str(".0");
            out.push_str(eol);
            changed = true;
        } else {
            out.push_str(line);
        }
    }
    if changed {
        Some(out)
    } else {
        None
    }
}

fn describe_load_order_change(
    rel: &str,
    to_disable: &[(String, String)],
    to_reenable: &BTreeSet<String>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !to_disable.is_empty() {
        let names: Vec<&str> = to_disable.iter().map(|(_, d)| d.as_str()).collect();
        parts.push(format!("switch off {}", names.join(", ")));
    }
    if !to_reenable.is_empty() {
        let names: Vec<&str> = to_reenable.iter().map(|d| d.as_str()).collect();
        parts.push(format!("switch on {}", names.join(", ")));
    }
    let action = if parts.is_empty() {
        "reorder".to_string()
    } else {
        parts.join(" and ")
    };
    format!("Update {} so the game loads {} in the right order.", rel, action)
}

/// Read the active-mod list using the same newest-file-wins rule the app
/// already uses, so the engine plans against the list the game will read.
fn read_authoritative_active_list(z_dir: &Path) -> Vec<String> {
    let ini = z_dir.join("mods").join("ModListData.ini");
    crate::load_order::ini_parser::read_mod_list_ini(&ini.to_string_lossy())
        .map(|d| d.active_mods)
        .unwrap_or_default()
}

/// Of a conflicting pair, keep one and disable the other.
///
/// Deterministic rule, in order: prefer keeping a mod that something else
/// depends on; then prefer keeping the enabled one; then keep the
/// lexicographically smaller normalized id. If any of those fail to separate
/// them, fall back to id order — never a coin flip.
fn pick_survivor(manifests: &[ModManifest], pair: &[String]) -> Option<(String, String)> {
    if pair.len() < 2 {
        return None;
    }
    let a = pair[0].clone();
    let b = pair[1].clone();

    let a_needed = manifests
        .iter()
        .any(|m| normalize_id(&m.id) != normalize_id(&a) && needs(&m.require, &a));
    let b_needed = manifests
        .iter()
        .any(|m| normalize_id(&m.id) != normalize_id(&b) && needs(&m.require, &b));

    if a_needed != b_needed {
        let (keep, drop_id) = if a_needed { (&a, &b) } else { (&b, &a) };
        return Some((keep.clone(), drop_id.clone()));
    }

    let a_enabled = is_enabled(manifests, &a);
    let b_enabled = is_enabled(manifests, &b);
    if a_enabled != b_enabled {
        let (keep, drop_id) = if a_enabled { (&a, &b) } else { (&b, &a) };
        return Some((keep.clone(), drop_id.clone()));
    }

    if normalize_id(&a) <= normalize_id(&b) {
        Some((a, b))
    } else {
        Some((b, a))
    }
}

fn needs(list: &[String], id: &str) -> bool {
    let n = normalize_id(id);
    list.iter().any(|r| {
        let nr = normalize_id(r);
        nr == n || (nr.starts_with(&n) && !n.is_empty())
    })
}

fn is_enabled(manifests: &[ModManifest], id: &str) -> bool {
    let n = normalize_id(id);
    manifests
        .iter()
        .find(|m| normalize_id(&m.id) == n)
        .map(|m| m.enabled)
        .unwrap_or(false)
}

fn matching_id(manifests: &[ModManifest], wanted: &str) -> String {
    let n = normalize_id(wanted);
    manifests
        .iter()
        .find(|m| normalize_id(&m.id) == n)
        .map(|m| m.id.clone())
        .unwrap_or_else(|| wanted.trim().to_string())
}

/// Apply disables, re-enables and a topological reorder to produce the list the
/// game should load. Ids the engine knows nothing about are preserved in their
/// original relative order rather than dropped.
fn reconcile_active_list(
    current: &[String],
    to_disable: &[(String, String)],
    to_reenable: &BTreeSet<String>,
    manifests: &[ModManifest],
) -> Vec<String> {
    let disabled: BTreeSet<String> = to_disable
        .iter()
        .map(|(_, d)| normalize_id(d))
        .collect();
    let mut target: Vec<String> = current
        .iter()
        .filter(|id| {
            let n = normalize_id(id);
            !n.is_empty() && !disabled.contains(&n)
        })
        .cloned()
        .collect();

    // Explicitly re-enabled mods join the list even if they were off.
    for id in to_reenable.iter() {
        let n = normalize_id(id);
        if n.is_empty() || target.iter().any(|t| normalize_id(t) == n) {
            continue;
        }
        target.push(id.clone());
    }

    // Any other installed-but-disabled mod that is a hard requirement of
    // something active comes back too. This is the "one bad mod must not break
    // the rest" rule applied to the load order as well.
    let active_now: BTreeSet<String> = target.iter().map(|id| normalize_id(id)).collect();
    let wanted_libs: BTreeSet<String> = manifests
        .iter()
        .filter(|m| active_now.contains(&normalize_id(&m.id)))
        .flat_map(|m| m.require.iter())
        .map(|r| normalize_id(r))
        .collect();
    for m in manifests {
        let n = normalize_id(&m.id);
        if n.is_empty() || m.enabled || target.iter().any(|t| normalize_id(t) == n) {
            continue;
        }
        if wanted_libs.contains(&n) {
            target.push(m.id.clone());
        }
    }

    // Topological pass over exactly the ids we are keeping, so required libs
    // load before their dependents. Ids with no manifest stay put at the end.
    let known: Vec<ModManifest> = manifests
        .iter()
        .filter(|m| target.iter().any(|t| normalize_id(t) == normalize_id(&m.id)))
        .cloned()
        .collect();
    let sorted = sort_dependencies_topologically(&known);
    let mut out: Vec<String> = Vec::new();
    for id in &sorted.sorted_mod_ids {
        let n = normalize_id(id);
        if let Some(existing) = target.iter().find(|t| normalize_id(t) == n) {
            let c = existing.clone();
            if !out.iter().any(|o| normalize_id(o) == n) {
                out.push(c);
            }
        }
    }
    for id in &target {
        let n = normalize_id(id);
        if !out.iter().any(|o| normalize_id(o) == n) {
            out.push(id.clone());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Plan id
// ---------------------------------------------------------------------------

/// Hash over the concrete operations: target path, the exact bytes to write,
/// the bytes currently on disk, and the backup directory the bytes will land in.
fn compute_plan_id(backup_id_base: &Path, ops: &[Operation]) -> String {
    let mut fields: Vec<Vec<u8>> = Vec::new();
    fields.push(b"pzms-fix-plan-v1".to_vec());
    fields.push(backup_id_base.to_string_lossy().as_bytes().to_vec());
    fields.push(ops.len().to_string().into_bytes());
    for op in ops {
        fields.push(op.change_id.clone().into_bytes());
        fields.push(format!("{:?}", op.kind).into_bytes());
        fields.push(op.target_path.to_string_lossy().as_bytes().to_vec());
        fields.push(hash_fields(&[&op.original]).into_bytes());
        match &op.bytes {
            Some(b) => fields.push(hash_fields(&[b]).into_bytes()),
            // A no-op contributes a distinct marker so "write these bytes" and
            // "write nothing" can never collide.
            None => fields.push(b"<no-op>".to_vec()),
        }
    }
    let refs: Vec<&[u8]> = fields.iter().map(|v| v.as_slice()).collect();
    hash_fields(&refs)
}

/// The backup folder name for a plan. `plan_id` comes straight from the
/// frontend, so it must be treated as untrusted: short ids are hashed up to
/// length rather than sliced, which would panic.
fn backup_id_for(plan_id: &str) -> String {
    let trimmed = plan_id.trim();
    let head: String = trimmed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(16)
        .collect();
    if head.len() >= 8 {
        format!("backup_{}", head)
    } else {
        format!("backup_{}", short_hash(trimmed.as_bytes()))
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Gather everything the planner needs from disk. Never fails: a missing
/// directory yields an empty scan, not an error.
///
/// The scan is strictly **read-only** ([`ScanMode::ReadOnly`]). The repairing
/// scan auto-installs the Live Bridge mod and copies `mod.info` into build-42
/// sub-folders; doing that during a preview would make two previews of an
/// unchanged install disagree about what would change.
fn gather_inputs(user_zomboid_dir: &str) -> PlanInputs {
    let z_dir = primary_zomboid_dir(user_zomboid_dir);
    let paths = crate::vfs::StudioPaths {
        pz_install_dir: String::new(),
        workshop_dir: String::new(),
        user_zomboid_dir: z_dir.to_string_lossy().to_string(),
        mod_list_ini_path: z_dir
            .join("mods")
            .join("ModListData.ini")
            .to_string_lossy()
            .to_string(),
        carrier_workshop_id: None,
        is_valid: z_dir.is_dir(),
    };
    let report = crate::load_order::mod_info::scan_all_installed_mods_in_mode(
        &paths,
        crate::load_order::mod_info::ScanMode::ReadOnly,
    );
    let diagnostics =
        crate::conflicts::analyze_with_compat(&report.all_installs, crate::compat::global_rules(), None);
    PlanInputs {
        z_dir,
        manifests: report.all_installs,
        diagnostics,
    }
}

/// **Dry run.** Computes what would change and returns it. Performs **no writes**
/// — no backup directory is created, no mod folder is touched, no load-order
/// file is modified. Safe to call repeatedly and from any UI state.
///
/// Operations whose target file does not exist, or whose recomputed content is
/// byte-identical to what is already there, are omitted entirely rather than
/// reported as no-ops.
#[tauri::command]
pub fn preview_mod_fixes(user_zomboid_dir: String) -> FixPlan {
    build_plan(&user_zomboid_dir)
}

pub fn build_plan(user_zomboid_dir: &str) -> FixPlan {
    let inputs = gather_inputs(user_zomboid_dir);
    let ops = build_operations(&inputs);
    let backup_id_base = backup_root(user_zomboid_dir);
    let plan_id = compute_plan_id(&backup_id_base, &ops);
    let backup_id = backup_id_for(&plan_id);
    let changes: Vec<PlannedChange> = ops.iter().map(|o| o.to_planned(&backup_id)).collect();
    let affected_files = changes
        .iter()
        .map(|c| c.target_path.clone())
        .collect::<BTreeSet<String>>()
        .len();

    let summary = if changes.is_empty() {
        "Nothing to fix automatically. Every problem found either has no safe \
         automatic fix, or is already handled."
            .to_string()
    } else {
        format!(
            "{} change{} across {} file{}.",
            changes.len(),
            if changes.len() == 1 { "" } else { "s" },
            affected_files,
            if affected_files == 1 { "" } else { "s" }
        )
    };

    FixPlan {
        plan_id,
        summary,
        requires_confirmation: !changes.is_empty(),
        affected_files,
        changes,
    }
}

/// Apply a previously previewed plan.
///
/// `plan_id` is **re-validated**: the plan is recomputed from freshly read disk
/// state and the hashes must match. `confirm_token` must equal `plan_id`, so a
/// caller cannot apply a plan it never displayed.
///
/// Backups are written before the corresponding file is replaced. If a backup
/// fails, that change is skipped and the rest still proceed.
#[tauri::command]
pub fn apply_mod_fix(
    user_zomboid_dir: String,
    plan_id: String,
    confirm_token: String,
) -> ApplyResult {
    apply_plan(&user_zomboid_dir, &plan_id, &confirm_token)
}

pub fn apply_plan(
    user_zomboid_dir: &str,
    plan_id: &str,
    confirm_token: &str,
) -> ApplyResult {
    let mut result = ApplyResult {
        applied: Vec::new(),
        skipped: Vec::new(),
        backups: Vec::new(),
        errors: Vec::new(),
    };

    let presented = plan_id.trim();
    if presented.is_empty() {
        result.errors.push(fail(
            FixErrorCode::UnknownPlan,
            "no plan_id was supplied. Preview the fixes first.",
        ));
        return result;
    }

    // --- Idempotency, checked before anything else --------------------------
    // A plan whose backup manifest already records this exact plan_id has been
    // applied. Re-applying it would double-write; report and stop.
    if let Some(manifest) = read_backup_manifest(user_zomboid_dir, &backup_id_for(presented)) {
        if manifest.plan_id == presented {
            // Reported in BOTH lists: `skipped` explains what did not happen,
            // `errors` is what a UI checks to decide whether this was a refusal
            // or a no-op. Silence in `errors` would read as "unknown outcome".
            let reason = fail(
                FixErrorCode::NothingToDo,
                "this plan was already applied; nothing was written again",
            );
            result.skipped.push(SkippedChange {
                change_id: String::new(),
                reason: reason.clone(),
            });
            result.errors.push(reason);
            result.backups = manifest_to_entries(
                &manifest,
                &backup_root(user_zomboid_dir).join(&manifest.backup_id),
            );
            return result;
        }
    }

    if confirm_token.trim() != presented {
        result.errors.push(fail(
            FixErrorCode::ConfirmationRequired,
            "confirm_token must echo the plan_id that was previewed.",
        ));
        return result;
    }

    // --- Recompute and compare ----------------------------------------------
    let inputs = gather_inputs(user_zomboid_dir);
    let ops = build_operations(&inputs);
    let backup_id_base = backup_root(user_zomboid_dir);
    let fresh_plan_id = compute_plan_id(&backup_id_base, &ops);

    if fresh_plan_id != presented {
        result.errors.push(fail(
            FixErrorCode::PlanStale,
            "the mods or load order changed since this plan was previewed. \
             Preview again and confirm the new plan.",
        ));
        for op in &ops {
            result.skipped.push(SkippedChange {
                change_id: op.change_id.clone(),
                reason: "plan is stale; re-preview".to_string(),
            });
        }
        return result;
    }

    let writable: Vec<&Operation> = ops.iter().filter(|o| o.bytes.is_some()).collect();
    if writable.is_empty() {
        result.errors.push(fail(
            FixErrorCode::NothingToDo,
            "this plan has no changes left to apply.",
        ));
        return result;
    }

    // --- Backup, then write --------------------------------------------------
    let backup_dir = backup_id_base.join(backup_id_for(presented));
    if let Err(e) = std::fs::create_dir_all(&backup_dir) {
        result.errors.push(fail(
            FixErrorCode::Io,
            &format!(
                "could not create the backup folder {}: {}",
                backup_dir.to_string_lossy(),
                e
            ),
        ));
        return result;
    }

    let created_at_unix = now_unix();
    let mut manifest = BackupManifest {
        backup_id: backup_id_for(presented),
        plan_id: presented.to_string(),
        created_at_unix,
        source: BACKUP_SOURCE.to_string(),
        entries: Vec::new(),
    };

    // Back up **every** file first. If any backup fails, nothing is written at
    // all: a partial write with a partial backup set is the worst outcome.
    let mut backup_results: Vec<(PathBuf, PathBuf, u64)> = Vec::new();
    let mut backup_errors: Vec<String> = Vec::new();
    for op in &writable {
        let dest = backup_dir.join(backup_file_name(&op.target_path));
        if let Err(e) = std::fs::write(&dest, &op.original) {
            backup_errors.push(format!(
                "could not back up {} to {}: {}",
                op.target_path.to_string_lossy(),
                dest.to_string_lossy(),
                e
            ));
        } else {
            backup_results.push((op.target_path.clone(), dest, op.original.len() as u64));
        }
    }
    if !backup_errors.is_empty() {
        for op in &writable {
            result.skipped.push(SkippedChange {
                change_id: op.change_id.clone(),
                reason: "not applied because its file could not be backed up first".to_string(),
            });
        }
        result
            .errors
            .push(fail(FixErrorCode::Io, &backup_errors.join("; ")));
        return result;
    }

    for (i, (target, dest, size)) in backup_results.iter().enumerate() {
        let op = writable[i];
        manifest.entries.push(BackupManifestEntry {
            change_id: op.change_id.clone(),
            original_path: target.to_string_lossy().to_string(),
            backup_file: dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            size_bytes: *size,
        });
    }

    // Now — and only now — write.
    for op in &writable {
        let Some(bytes) = &op.bytes else { continue };
        match write_atomic(op.target_path.as_path(), bytes) {
            Ok(()) => result.applied.push(op.change_id.clone()),
            Err(e) => {
                result.skipped.push(SkippedChange {
                    change_id: op.change_id.clone(),
                    reason: format!("write failed: {}", e),
                });
                result.errors.push(fail(
                    FixErrorCode::Io,
                    &format!("could not write {}: {}", op.target_path.to_string_lossy(), e),
                ));
            }
        }
    }

    if let Err(e) = std::fs::write(
        backup_dir.join(BACKUP_MANIFEST_NAME),
        serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "{}".to_string()),
    ) {
        result.errors.push(fail(
            FixErrorCode::Io,
            &format!(
                "the fix was applied but its backup index could not be written ({}). \
                 The original bytes are still in {} — restore them by hand if needed.",
                e,
                backup_dir.to_string_lossy()
            ),
        ));
    }

    result.backups = manifest_to_entries(&manifest, &backup_dir);
    result.applied.sort();
    result.skipped.sort_by(|a, b| a.change_id.cmp(&b.change_id));
    result.backups.sort_by(|a, b| {
        a.backup_id
            .cmp(&b.backup_id)
            .then_with(|| a.original_path.cmp(&b.original_path))
    });
    result
}

/// Write `bytes` to `path` via a sibling temp file plus a rename, so a reader
/// never observes a half-written load-order file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let tmp = path.with_file_name(format!("{}.pzms_tmp", file_name));
    std::fs::write(&tmp, bytes).map_err(|e| format!("temp write failed: {}", e))?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(format!("could not replace the file: {}", e))
        }
    }
}

fn backup_file_name(target: &Path) -> String {
    let base = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let safe: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    format!("{}.original", safe)
}

// ---------------------------------------------------------------------------
// Backups
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub backup_id: String,
    pub plan_id: String,
    pub created_at_unix: u64,
    pub source: String,
    pub entries: Vec<BackupManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifestEntry {
    pub change_id: String,
    pub original_path: String,
    pub backup_file: String,
    pub size_bytes: u64,
}

/// Flatten a manifest into one [`BackupEntry`] per backed-up file.
///
/// `backup_dir` is needed because the manifest stores the backup *file name*
/// only; `backup_path` in the wire format must be a usable absolute path, not a
/// bare name a caller cannot open.
fn manifest_to_entries(m: &BackupManifest, backup_dir: &Path) -> Vec<BackupEntry> {
    m.entries
        .iter()
        .map(|e| BackupEntry {
            backup_id: m.backup_id.clone(),
            created_at_unix: m.created_at_unix,
            original_path: e.original_path.clone(),
            backup_path: backup_dir.join(&e.backup_file).to_string_lossy().to_string(),
            size_bytes: e.size_bytes,
            source: m.source.clone(),
        })
        .collect()
}

fn read_backup_manifest(user_zomboid_dir: &str, backup_id: &str) -> Option<BackupManifest> {
    let p = backup_root(user_zomboid_dir).join(backup_id).join(BACKUP_MANIFEST_NAME);
    let text = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&text).ok()
}

/// Every backup this engine has taken, sorted by `(backup_id, original_path)`
/// so the list is stable across calls.
#[tauri::command]
pub fn list_backups(user_zomboid_dir: String) -> Vec<BackupEntry> {
    list_backups_impl(&user_zomboid_dir)
}

pub fn list_backups_impl(user_zomboid_dir: &str) -> Vec<BackupEntry> {
    let root = backup_root(user_zomboid_dir);
    let mut out: Vec<BackupEntry> = Vec::new();
    let Ok(dir) = std::fs::read_dir(&root) else {
        return out;
    };
    for entry in dir.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        let manifest = read_backup_manifest(user_zomboid_dir, &id);
        match manifest {
            Some(m) => out.extend(manifest_to_entries(&m, &entry.path())),
            None => out.push(BackupEntry {
                // A backup dir with no readable index is still reported, so the
                // user can see that bytes exist and are not silently forgotten.
                backup_id: id,
                created_at_unix: 0,
                original_path: String::new(),
                backup_path: entry.path().to_string_lossy().to_string(),
                size_bytes: 0,
                source: "unknown".to_string(),
            }),
        }
    }
    out.sort_by(|a, b| {
        a.backup_id
            .cmp(&b.backup_id)
            .then_with(|| a.original_path.cmp(&b.original_path))
    });
    out
}

/// Restore every file in a backup set to the bytes it had before the fix.
///
/// Each file is restored to a temp sibling and renamed into place, so a failure
/// part-way through leaves the remaining files untouched rather than leaving
/// one truncated. Files are restored in sorted `original_path` order so the
/// sequence is deterministic.
#[tauri::command]
pub fn restore_backup(user_zomboid_dir: String, backup_id: String) -> RestoreResult {
    restore_backup_impl(&user_zomboid_dir, &backup_id)
}

pub fn restore_backup_impl(user_zomboid_dir: &str, backup_id: &str) -> RestoreResult {
    let mut result = RestoreResult {
        restored: Vec::new(),
        errors: Vec::new(),
    };
    let id = backup_id.trim();
    if id.is_empty() {
        result.errors.push("no backup_id was supplied.".to_string());
        return result;
    }
    // Reject traversal in the id: it must be a single directory name.
    if id.contains(['/', '\\', ':']) || id.contains("..") {
        result.errors.push(format!("{} is not a valid backup_id.", id));
        return result;
    }

    let dir = backup_root(user_zomboid_dir).join(id);
    let Some(manifest) = read_backup_manifest(user_zomboid_dir, id) else {
        result.errors.push(format!(
            "no backup index found for {}. Nothing was restored.",
            id
        ));
        return result;
    };

    // Deterministic order, and originals are restored before anything is
    // reported as done.
    let mut entries = manifest.entries.clone();
    entries.sort_by(|a, b| a.original_path.cmp(&b.original_path));

    // Read every backup file up front. A missing one means the whole restore is
    // refused rather than half-completed.
    let mut payloads: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for e in &entries {
        let src = dir.join(&e.backup_file);
        match std::fs::read(&src) {
            Ok(bytes) => payloads.push((PathBuf::from(&e.original_path), bytes)),
            Err(err) => result.errors.push(format!(
                "backup file {} is unreadable ({}); nothing was restored.",
                src.to_string_lossy(),
                err
            )),
        }
    }
    if !result.errors.is_empty() {
        return result;
    }

    for (target, bytes) in &payloads {
        match write_atomic(target, bytes) {
            Ok(()) => result.restored.push(target.to_string_lossy().to_string()),
            Err(e) => result.errors.push(format!(
                "could not restore {}: {}",
                target.to_string_lossy(),
                e
            )),
        }
    }
    result.restored.sort();
    result.errors.sort();
    result
}