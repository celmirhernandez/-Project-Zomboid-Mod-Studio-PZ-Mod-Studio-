//! Tests for the safe fix engine.
//!
//! These build a **real throwaway Zomboid tree on disk** (under the system temp
//! dir, in a uniquely named folder) rather than mocking the filesystem. The
//! safety property under test is about actual bytes on actual disk, so mocking
//! would test the mock.

use super::*;

fn manifest(id: &str, enabled: bool) -> ModManifest {
    ModManifest {
        id: id.to_string(),
        name: id.to_string(),
        description: None,
        workshop_id: None,
        author: None,
        version: None,
        pzversion: None,
        url: None,
        require: Vec::new(),
        load_mod_after: Vec::new(),
        incompatible: Vec::new(),
        icon_path: None,
        poster_url: None,
        is_library: false,
        is_map_mod: false,
        enabled,
        is_packaged: None,
    }
}

/// A scratch Zomboid folder. Deleted when the guard is dropped.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    /// Unique per test so parallel `cargo test` threads cannot collide.
    fn new(tag: &str) -> Sandbox {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("pzms_fixes_{}_{}", tag, nanos));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("mods")).expect("must create the sandbox root");
        Sandbox { root }
    }

    fn path(&self) -> String {
        self.root.to_string_lossy().to_string()
    }

    fn write(&self, rel: &str, contents: &str) -> PathBuf {
        let p = self.root.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("must create parent dir");
        }
        std::fs::write(&p, contents).expect("fixture write must succeed");
        p
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.root.join(rel)).unwrap_or_default()
    }

    fn exists(&self, rel: &str) -> bool {
        self.root.join(rel).exists()
    }

    /// Write a load-order set the app's own reader will accept.
    fn write_load_order(&self, active: &[&str]) {
        let ids: Vec<String> = active.iter().map(|s| s.to_string()).collect();
        self.write("mods.txt", &render_plain_list(&ids));
        self.write("mods/default.txt", &render_default_txt(&ids));
        self.write("Lua/ModListData.ini", &render_ini("ModList", "activeMods", &ids));
    }

    /// Install a mod folder with a `mod.info` declaring `id`.
    fn install_mod(&self, folder: &str, id: &str, extra: &str) {
        self.write(
            &format!("mods/{}/mod.info", folder),
            &format!("name={}\nid={}\n{}\n", id, id, extra),
        );
    }

    /// The standard fixture: a library that must load before an addon that
    /// requires it, plus one `mod.info` with a bare-integer `versionMin`.
    ///
    /// This produces **two kinds** of operation at once — a load-order reorder
    /// (three files) and a `mod.info` patch — so a single plan exercises both
    /// the backup path and the reversibility guarantee.
    fn standard(&self) {
        self.write_load_order(&["Alpha", "Beta"]);
        self.install_mod("Alpha", "Alpha", "require=Beta\n");
        self.install_mod("Beta", "Beta", "versionMin=42\n");
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn ids(list: &[String]) -> Vec<String> {
    list.iter().map(|s| s.to_lowercase()).collect()
}

// ---------------------------------------------------------------------------
// plan-id determinism
// ---------------------------------------------------------------------------

#[test]
fn hash_is_deterministic_and_field_boundaries_matter() {
    let a = hash_fields(&[b"ab", b"c"]);
    let b = hash_fields(&[b"ab", b"c"]);
    assert_eq!(a, b, "same inputs must hash the same");

    let split = hash_fields(&[b"a", b"bc"]);
    assert_ne!(a, split, "fields are length-prefixed, so ab|c != a|bc");
}

#[test]
fn hash_changes_when_any_single_byte_changes() {
    let base = hash_fields(&[b"target", b"original", b"new"]);
    let mut mutated = b"new".to_vec();
    mutated[0] ^= 0x01;
    assert_ne!(base, hash_fields(&[b"target", b"original", &mutated]));
    assert_ne!(base, hash_fields(&[b"targeu", b"original", b"new"]));
    assert_ne!(base, hash_fields(&[b"target", b"originaL", b"new"]));
}

#[test]
fn plan_id_is_stable_for_identical_inputs() {
    let sb = Sandbox::new("planid_stable");
    sb.standard();

    let a = build_plan(&sb.path());
    let b = build_plan(&sb.path());
    assert_eq!(
        a.plan_id, b.plan_id,
        "two previews of unchanged state must produce the same plan_id"
    );
    assert_eq!(
        ids(&a.changes.iter().flat_map(|c| c.affected_mod_ids.clone()).collect::<Vec<_>>()),
        ids(&b.changes.iter().flat_map(|c| c.affected_mod_ids.clone()).collect::<Vec<_>>()),
    );
}

#[test]
fn plan_id_changes_when_any_planned_byte_changes() {
    let sb = Sandbox::new("planid_drift");
    sb.standard();

    let before = build_plan(&sb.path());
    // A one-character change to a file the plan is about to rewrite.
    sb.write("mods.txt", "Alpha\nBeta\nBeta2\n");

    let after = build_plan(&sb.path());
    assert_ne!(
        before.plan_id, after.plan_id,
        "a changed target file must change the plan_id"
    );
}

#[test]
fn plan_id_differs_when_backup_root_differs() {
    let ops = vec![Operation {
        change_id: "c".to_string(),
        kind: FixKind::PatchFile,
        target_path: PathBuf::from("t"),
        description: String::new(),
        affected_mod_ids: Vec::new(),
        bytes: Some(b"x".to_vec()),
        original: b"y".to_vec(),
        diff_preview: None,
    }];
    let a = compute_plan_id(Path::new("/user/a/PZModStudio_Backups"), &ops);
    let b = compute_plan_id(Path::new("/user/b/PZModStudio_Backups"), &ops);
    assert_ne!(a, b, "backups in a different install must not share a plan id");
}

#[test]
fn a_no_op_operation_cannot_collide_with_a_write() {
    let mk = |bytes: Option<Vec<u8>>| Operation {
        change_id: "c".to_string(),
        kind: FixKind::PatchFile,
        target_path: PathBuf::from("t"),
        description: String::new(),
        affected_mod_ids: Vec::new(),
        bytes,
        original: b"y".to_vec(),
        diff_preview: None,
    };
    let noop = compute_plan_id(Path::new("b"), &[mk(None)]);
    let write = compute_plan_id(Path::new("b"), &[mk(Some(b"y".to_vec()))]);
    assert_ne!(noop, write);
}

#[test]
fn backup_id_is_derived_from_the_plan_id() {
    let plan_id = "0123456789abcdef0123456789abcdef";
    assert_eq!(backup_id_for(plan_id), "backup_0123456789abcdef");
    assert_eq!(backup_id_for(plan_id), backup_id_for(plan_id));
}

// ---------------------------------------------------------------------------
// determinism of output ordering
// ---------------------------------------------------------------------------

#[test]
fn changes_are_ordered_by_target_path() {
    let ops = vec![
        Operation {
            change_id: "z".to_string(),
            kind: FixKind::PatchFile,
            target_path: PathBuf::from("/z.txt"),
            description: String::new(),
            affected_mod_ids: vec!["B".to_string(), "A".to_string()],
            bytes: Some(b"1".to_vec()),
            original: b"0".to_vec(),
            diff_preview: None,
        },
        Operation {
            change_id: "a".to_string(),
            kind: FixKind::PatchFile,
            target_path: PathBuf::from("/a.txt"),
            description: String::new(),
            affected_mod_ids: Vec::new(),
            bytes: Some(b"1".to_vec()),
            original: b"0".to_vec(),
            diff_preview: None,
        },
    ];
    let mut sorted = ops.clone();
    sorted.sort_by(|x, y| {
        x.target_path
            .cmp(&y.target_path)
            .then_with(|| x.change_id.cmp(&y.change_id))
    });
    assert_eq!(sorted[0].target_path, PathBuf::from("/a.txt"));

    // Sorting is what makes the hash order-independent, and the planner always
    // sorts before hashing: two spellings of the same order must agree.
    let mut shuffled = ops.clone();
    shuffled.reverse();
    shuffled.sort_by(|x, y| {
        x.target_path
            .cmp(&y.target_path)
            .then_with(|| x.change_id.cmp(&y.change_id))
    });
    assert_eq!(
        compute_plan_id(Path::new("b"), &sorted),
        compute_plan_id(Path::new("b"), &shuffled),
        "hash must not depend on input order once the list is sorted"
    );

    // And an actually-unsorted input is a different plan, which is why the
    // planner must never skip the sort.
    assert_ne!(
        compute_plan_id(Path::new("b"), &ops),
        compute_plan_id(Path::new("b"), &sorted),
        "an unsorted operation list is a different plan"
    );
}

#[test]
fn affected_mod_ids_are_sorted_and_deduplicated() {
    let sb = Sandbox::new("affected_sorted");
    sb.write_load_order(&["Zeta", "Alpha", "Mid"]);
    // Alpha and Zeta are mutually incompatible; Beta is a library Alpha needs.
    sb.install_mod("Alpha", "Alpha", "require=Beta\n");
    sb.install_mod("Zeta", "Zeta", "");
    sb.install_mod("Beta", "Beta", "");
    sb.write_load_order(&["Zeta", "Alpha", "Mid"]);

    let plan = build_plan(&sb.path());
    for change in &plan.changes {
        let mut sorted = change.affected_mod_ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            change.affected_mod_ids, sorted,
            "affected_mod_ids must be sorted and deduplicated"
        );
    }
}

#[test]
fn listing_backups_twice_gives_the_same_order() {
    let sb = Sandbox::new("backup_order");
    sb.standard();

    let plan = build_plan(&sb.path());
    let result = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    assert!(!result.backups.is_empty(), "the fixture must produce backups");

    let a = list_backups_impl(&sb.path());
    let b = list_backups_impl(&sb.path());
    assert_eq!(
        a.iter().map(|e| (e.backup_id.clone(), e.original_path.clone())).collect::<Vec<_>>(),
        b.iter().map(|e| (e.backup_id.clone(), e.original_path.clone())).collect::<Vec<_>>(),
    );
    let mut sorted = a.clone();
    sorted.sort_by(|x, y| {
        x.backup_id
            .cmp(&y.backup_id)
            .then_with(|| x.original_path.cmp(&y.original_path))
    });
    assert_eq!(
        a.iter().map(|e| e.backup_id.clone()).collect::<Vec<_>>(),
        sorted.iter().map(|e| e.backup_id.clone()).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// dry run: preview writes nothing
// ---------------------------------------------------------------------------

#[test]
fn preview_writes_nothing_to_disk() {
    let sb = Sandbox::new("dry_run");
    sb.standard();

    let before: Vec<(String, Vec<u8>)> = snapshot_tree(&sb.root);

    let plan = build_plan(&sb.path());
    assert!(!plan.changes.is_empty(), "the fixture must plan at least one fix");
    assert!(plan.requires_confirmation);
    assert!(plan.affected_files >= 1);

    let after = snapshot_tree(&sb.root);
    assert_eq!(
        before, after,
        "preview_mod_fixes must be a pure dry run: not one byte may change"
    );
    assert!(
        !sb.exists("PZModStudio_Backups"),
        "preview must not even create the backup directory"
    );
}

#[test]
fn preview_twice_produces_identical_plans() {
    let sb = Sandbox::new("dry_run_twice");
    sb.standard();

    let first = build_plan(&sb.path());
    let second = build_plan(&sb.path());
    assert_eq!(first.plan_id, second.plan_id);
    assert_eq!(first.summary, second.summary);
    assert_eq!(first.changes.len(), second.changes.len());
    assert_eq!(first.affected_files, second.affected_files);
}

/// Every file under `root`, as `(relative path, bytes)`, sorted. Used to prove a
/// dry run touched nothing.
fn snapshot_tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    for entry in WalkDir::new(root).sort_by_file_name().into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| entry.path().to_string_lossy().to_string());
        let bytes = std::fs::read(entry.path()).unwrap_or_default();
        out.push((rel, bytes));
    }
    out.sort();
    out
}

/// Same as [`snapshot_tree`] but ignoring `PZModStudio_Backups`.
///
/// Used when asserting a restore: the backup directory legitimately still
/// exists afterwards (deleting it would throw away the undo), so including it
/// would make a perfect restore look like a failure.
fn snapshot_user_tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = snapshot_tree(root)
        .into_iter()
        .filter(|(p, _)| !p.starts_with("PZModStudio_Backups"))
        .collect();
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// staleness
// ---------------------------------------------------------------------------

#[test]
fn apply_refuses_a_plan_whose_target_changed_after_preview() {
    let sb = Sandbox::new("stale");
    sb.standard();

    let plan = build_plan(&sb.path());
    assert!(!plan.changes.is_empty(), "the fixture must plan a change");

    // Someone edits the game while the dialog is open.
    sb.write("mods.txt", "Someone\nElse\n");

    let result = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    assert!(
        result.errors.iter().any(|e| e.starts_with(FixErrorCode::PlanStale.as_str())),
        "a drifted plan must be refused, got: {:?}",
        result.errors
    );
    assert!(result.applied.is_empty(), "nothing may be applied when stale");
    assert!(
        !result.skipped.is_empty(),
        "every change must be reported as skipped"
    );
    assert_eq!(
        sb.read("mods.txt"),
        "Someone\nElse\n",
        "the refused apply must not have written anything"
    );
    assert!(
        !sb.exists("PZModStudio_Backups"),
        "a refused apply must not leave a backup directory behind"
    );
}

#[test]
fn apply_refuses_a_single_byte_drift() {
    let sb = Sandbox::new("stale_one_byte");
    sb.standard();

    let plan = build_plan(&sb.path());
    // One byte, in a file the plan does not even rewrite: the mod.info patch.
    let info = sb.root.join("mods/Beta/mod.info");
    let mut bytes = std::fs::read(&info).expect("fixture exists");
    bytes.push(b'\n');
    std::fs::write(&info, bytes).expect("fixture write");

    let result = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    assert!(
        result.errors.iter().any(|e| e.starts_with(FixErrorCode::PlanStale.as_str())),
        "one extra byte must invalidate the plan, got: {:?}",
        result.errors
    );
    assert!(result.applied.is_empty());
}

#[test]
fn apply_refuses_an_unknown_plan_id() {
    let sb = Sandbox::new("unknown_plan");
    sb.write_load_order(&["Alpha", "Beta"]);
    sb.install_mod("Alpha", "Alpha", "");
    let before = snapshot_tree(&sb.root);

    // An id that was never previewed can only mismatch the recomputed plan, so it
    // is reported as stale. Either code is a refusal; what matters is that
    // nothing is written.
    let result = apply_plan(
        &sb.path(),
        "ffffffffffffffffffffffffffffffff",
        "ffffffffffffffffffffffffffffffff",
    );
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.starts_with(FixErrorCode::PlanStale.as_str())
                || e.starts_with(FixErrorCode::UnknownPlan.as_str())),
        "got: {:?}",
        result.errors
    );
    assert!(result.applied.is_empty(), "nothing may be applied");
    assert_eq!(before, snapshot_tree(&sb.root), "nothing may be written");
}

#[test]
fn apply_refuses_an_empty_plan_id() {
    let sb = Sandbox::new("empty_plan_id");
    let result = apply_plan(&sb.path(), "   ", "   ");
    assert!(result
        .errors
        .iter()
        .any(|e| e.starts_with(FixErrorCode::UnknownPlan.as_str())));
}

#[test]
fn apply_requires_the_confirmation_token_to_echo_the_plan_id() {
    let sb = Sandbox::new("confirm");
    sb.standard();
    let before = snapshot_tree(&sb.root);

    let plan = build_plan(&sb.path());
    for bad in ["", "   ", "yes", "some-other-token"] {
        let result = apply_plan(&sb.path(), &plan.plan_id, bad);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.starts_with(FixErrorCode::ConfirmationRequired.as_str())),
            "token {:?} must be rejected",
            bad
        );
        assert!(result.applied.is_empty());
    }
    assert_eq!(before, snapshot_tree(&sb.root), "no write may happen");
}

// ---------------------------------------------------------------------------
// backup then write, and restore round-trip
// ---------------------------------------------------------------------------

#[test]
fn apply_backs_up_before_writing_and_restore_round_trips() {
    let sb = Sandbox::new("round_trip");
    sb.standard();

    let original = snapshot_user_tree(&sb.root);
    let plan = build_plan(&sb.path());
    assert!(!plan.changes.is_empty());

    let result = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    assert!(result.errors.is_empty(), "apply must not error: {:?}", result.errors);
    assert!(!result.applied.is_empty(), "at least one change must apply");
    assert!(
        !result.backups.is_empty(),
        "every mutation must leave a backup entry"
    );

    // Backups live outside every mod folder and outside the load-order files.
    let backup_dir = sb.root.join("PZModStudio_Backups");
    assert!(backup_dir.is_dir(), "the backup root must exist");
    for entry in &result.backups {
        assert!(
            !entry.original_path.contains("PZModStudio_Backups"),
            "a backup must not point inside the backup tree: {}",
            entry.original_path
        );
        let size = std::fs::metadata(&entry.backup_path)
            .map(|m| m.len())
            .unwrap_or(0);
        assert!(size > 0, "backup {} must hold real bytes", entry.backup_path);
        assert_eq!(entry.source, "fix_apply");
        assert!(entry.created_at_unix > 0);
    }

    // The tree actually changed.
    assert_ne!(original, snapshot_user_tree(&sb.root));

    // And restores byte-for-byte.
    let restore = restore_backup_impl(&sb.path(), &result.backups[0].backup_id);
    assert!(restore.errors.is_empty(), "restore must not error: {:?}", restore.errors);
    assert!(!restore.restored.is_empty());
    assert_eq!(
        original,
        snapshot_user_tree(&sb.root),
        "restore must reproduce the original tree byte for byte"
    );
}

#[test]
fn backup_contents_match_the_bytes_they_replaced() {
    let sb = Sandbox::new("backup_bytes");
    sb.standard();

    let before: std::collections::HashMap<String, Vec<u8>> = snapshot_tree(&sb.root)
        .into_iter()
        // Key by absolute path with separators normalised: that is what
        // `BackupEntry.original_path` carries, and Windows happily mixes `/` and
        // `\` in the same string.
        .map(|(rel, bytes)| {
            let abs = sb.root.join(&rel).to_string_lossy().replace('/', "\\");
            (abs, bytes)
        })
        .collect();
    let plan = build_plan(&sb.path());
    let result = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);

    for entry in &result.backups {
        let key = entry.original_path.replace('/', "\\");
        let original_bytes = before
            .get(&key)
            .unwrap_or_else(|| panic!("entry {} not in {:?}", key, before.keys()));
        let backed_up = std::fs::read(&entry.backup_path).expect("backup file must exist");
        assert_eq!(
            &backed_up, original_bytes,
            "backup of {} must hold the exact bytes it replaced",
            entry.original_path
        );
        assert_eq!(entry.size_bytes, original_bytes.len() as u64);
    }
}

#[test]
fn a_failed_backup_prevents_every_write() {
    // A file the plan wants to rewrite, made unwritable by making its path a
    // directory. The backup of that path must fail, and then nothing is written.
    let sb = Sandbox::new("backup_failure");
    sb.standard();

    let plan = build_plan(&sb.path());
    assert!(!plan.changes.is_empty());

    // Occupy the backup file name for the first change so writing it fails.
    let first_target = PathBuf::from(&plan.changes[0].target_path);
    let backup_name = backup_file_name(&first_target);
    let backup_dir = backup_root(&sb.path()).join(backup_id_for(&plan.plan_id));
    std::fs::create_dir_all(&backup_dir).expect("must create the blocking dir");
    // Make the destination a directory: `fs::write` to it will fail.
    std::fs::create_dir_all(backup_dir.join(&backup_name)).expect("must create the blocker");

    let before = snapshot_user_tree(&sb.root);
    let result = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);

    assert!(
        result.errors.iter().any(|e| e.starts_with(FixErrorCode::Io.as_str())),
        "a failed backup is an IO error, got: {:?}",
        result.errors
    );
    assert!(
        result.applied.is_empty(),
        "not one file may be written when any backup fails"
    );
    assert_eq!(
        before,
        snapshot_user_tree(&sb.root),
        "a failed backup run must leave every user file untouched"
    );
    assert!(
        result.backups.is_empty(),
        "a refused run must not report backups it cannot complete"
    );
}

#[test]
fn restore_rejects_a_traversing_backup_id() {
    let sb = Sandbox::new("restore_traversal");
    for bad in ["../../etc", "a/b", "a\\b", "C:x", ".."] {
        let r = restore_backup_impl(&sb.path(), bad);
        assert!(!r.errors.is_empty(), "{:?} must be rejected", bad);
        assert!(r.restored.is_empty(), "{:?} must restore nothing", bad);
    }
}

#[test]
fn restore_of_an_unknown_backup_reports_and_changes_nothing() {
    let sb = Sandbox::new("restore_unknown");
    sb.write_load_order(&["Alpha"]);
    let before = snapshot_tree(&sb.root);
    let r = restore_backup_impl(&sb.path(), "backup_does_not_exist");
    assert!(!r.errors.is_empty());
    assert!(r.restored.is_empty());
    assert_eq!(before, snapshot_tree(&sb.root));
}

#[test]
fn restore_refuses_wholesale_when_one_backup_file_is_missing() {
    let sb = Sandbox::new("restore_partial");
    sb.standard();
    let original = snapshot_tree(&sb.root);

    let plan = build_plan(&sb.path());
    let result = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    assert!(result.backups.len() > 1, "fixture must back up several files");

    // Delete one backup file: the restore must refuse entirely rather than
    // half-completing and leaving a mixed state.
    let victim = sb.root.join("PZModStudio_Backups").join(&result.backups[0].backup_id);
    std::fs::remove_file(victim.join(backup_file_name(Path::new(&result.backups[0].original_path))))
        .expect("must remove the backup file");

    let after_apply = snapshot_tree(&sb.root);
    let r = restore_backup_impl(&sb.path(), &result.backups[0].backup_id);
    assert!(!r.errors.is_empty(), "a missing backup file must be reported");
    assert!(r.restored.is_empty(), "nothing may be restored");
    assert_eq!(
        after_apply,
        snapshot_tree(&sb.root),
        "a refused restore must leave the tree exactly as it was"
    );
    assert_ne!(original, after_apply);
}

#[test]
fn restore_is_safe_to_repeat() {
    let sb = Sandbox::new("restore_repeat");
    sb.standard();
    let original = snapshot_user_tree(&sb.root);

    let plan = build_plan(&sb.path());
    let applied = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    let backup_id = applied.backups[0].backup_id.clone();

    assert!(restore_backup_impl(&sb.path(), &backup_id).errors.is_empty());
    let after_first = snapshot_user_tree(&sb.root);
    assert_eq!(after_first, original);

    // Running it again must be a no-op, not a second write of something else.
    let second = restore_backup_impl(&sb.path(), &backup_id);
    assert!(second.errors.is_empty());
    assert_eq!(snapshot_user_tree(&sb.root), after_first);
    assert_eq!(after_first, original);
}

#[test]
fn backup_root_is_outside_the_mods_folder() {
    let sb = Sandbox::new("backup_location");
    let root = backup_root(&sb.path());
    assert!(root.starts_with(&sb.root));
    assert!(root.ends_with("PZModStudio_Backups"));
    assert!(
        !root.starts_with(sb.root.join("mods")),
        "backups must never live inside a mod folder"
    );
    assert_ne!(
        root.to_string_lossy().to_string(),
        sb.root.join("mods").join("ModListData.ini").to_string_lossy().to_string()
    );
}

// ---------------------------------------------------------------------------
// idempotency
// ---------------------------------------------------------------------------

#[test]
fn applying_the_same_plan_twice_does_not_double_apply() {
    let sb = Sandbox::new("idempotent");
    sb.standard();

    let plan = build_plan(&sb.path());
    assert!(!plan.changes.is_empty());

    let first = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    assert!(!first.applied.is_empty(), "first apply must do work");
    assert!(first.errors.is_empty(), "first apply must not error");
    let after_first = snapshot_tree(&sb.root);

    let second = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    assert!(
        second.applied.is_empty(),
        "the second apply must not write anything again"
    );
    assert!(
        second
            .errors
            .iter()
            .any(|e| e.starts_with(FixErrorCode::NothingToDo.as_str())),
        "the second apply must report already-applied, got: {:?}",
        second.errors
    );
    assert_eq!(
        after_first,
        snapshot_tree(&sb.root),
        "a repeated apply must leave the tree byte-identical"
    );
}

#[test]
fn a_third_apply_is_also_a_no_op() {
    let sb = Sandbox::new("idempotent_thrice");
    sb.standard();

    let plan = build_plan(&sb.path());
    apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    let after = snapshot_tree(&sb.root);
    for _ in 0..2 {
        let again = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
        assert!(again.applied.is_empty());
        assert_eq!(snapshot_tree(&sb.root), after);
    }
}

// ---------------------------------------------------------------------------
// no-delete guarantee
// ---------------------------------------------------------------------------

#[test]
fn no_planned_change_ever_deletes() {
    let sb = Sandbox::new("no_delete");
    sb.standard();

    let plan = build_plan(&sb.path());
    for change in &plan.changes {
        assert!(
            change.reversible,
            "{} is not reversible; the engine must only rewrite files",
            change.change_id
        );
        assert!(
            change.backup_path.is_some(),
            "{} has no backup target",
            change.change_id
        );
    }
    // Apply, then confirm no mod file and no load-order file vanished.
    let apply = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    assert!(apply.errors.is_empty());
    assert!(sb.exists("mods/Alpha/mod.info"), "mod files must survive");
    assert!(sb.exists("mods/Beta/mod.info"), "mod files must survive");
    assert!(sb.exists("mods.txt"), "load-order files must survive");
    assert!(sb.exists("mods/default.txt"));
}

#[test]
fn apply_never_removes_a_file_from_disk() {
    let sb = Sandbox::new("no_delete_apply");
    sb.standard();

    let before: BTreeSet<String> = snapshot_tree(&sb.root).into_iter().map(|(p, _)| p).collect();
    let plan = build_plan(&sb.path());
    apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    let after: BTreeSet<String> = snapshot_tree(&sb.root).into_iter().map(|(p, _)| p).collect();

    for path in &before {
        if path.starts_with("PZModStudio_Backups") {
            continue;
        }
        assert!(after.contains(path), "{} must still exist after apply", path);
    }
}

#[test]
fn an_existing_mod_folder_is_never_emptied() {
    let sb = Sandbox::new("no_delete_folder");
    sb.write_load_order(&["Alpha"]);
    sb.install_mod("Alpha", "Alpha", "require=Beta\n");
    sb.write("mods/Alpha/media/thing.lua", "return 1\n");
    sb.write("mods/Alpha/42/chart.lua", "return 2\n");

    let plan = build_plan(&sb.path());
    apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);

    assert!(sb.exists("mods/Alpha/media/thing.lua"), "mod assets survive");
    assert!(sb.exists("mods/Alpha/42/chart.lua"), "mod assets survive");
}

#[test]
fn disable_is_used_instead_of_removal() {
    // Two mutually incompatible mods: the plan must reorder/disable the load
    // order, never propose removing files.
    let sb = Sandbox::new("disable_not_delete");
    sb.write_load_order(&["Alpha", "Beta"]);
    sb.install_mod("Alpha", "Alpha", "incompatible=Beta\n");
    sb.install_mod("Beta", "Beta", "");

    let plan = build_plan(&sb.path());
    assert!(
        plan.changes
            .iter()
            .any(|c| matches!(c.kind, FixKind::DisableMod)),
        "an incompatible pair must produce a DisableMod change"
    );
    let apply = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    assert!(apply.errors.is_empty());
    assert!(sb.exists("mods/Alpha/mod.info"), "Alpha must still be on disk");
    assert!(sb.exists("mods/Beta/mod.info"), "Beta must still be on disk");
}

// ---------------------------------------------------------------------------
// totality
// ---------------------------------------------------------------------------

#[test]
fn an_empty_directory_produces_an_empty_plan_not_a_crash() {
    let sb = Sandbox::new("totality_empty");
    let plan = build_plan(&sb.path());
    assert!(plan.changes.is_empty());
    assert_eq!(plan.affected_files, 0);
    assert!(!plan.requires_confirmation);
    assert!(!plan.plan_id.is_empty(), "a plan always has an id");
    assert!(!plan.summary.is_empty());
}

#[test]
fn a_missing_directory_does_not_crash_the_engine() {
    let missing = std::env::temp_dir().join("pzms_fixes_definitely_absent_zzz");
    let _ = std::fs::remove_dir_all(&missing);
    let plan = build_plan(&missing.to_string_lossy());
    assert!(plan.changes.is_empty(), "a missing folder plans nothing");

    let result = apply_plan(&missing.to_string_lossy(), &plan.plan_id, &plan.plan_id);
    assert!(result.applied.is_empty());
    assert!(!missing.exists(), "the engine must not create the user's folder");
}

#[test]
fn empty_user_zomboid_dir_falls_back_without_panicking() {
    let plan = build_plan("");
    assert!(!plan.plan_id.is_empty());
    assert_eq!(plan.affected_files, 0);
    let _ = list_backups_impl("");
    let r = restore_backup_impl("", "backup_nothing");
    assert!(!r.errors.is_empty());
}

#[test]
fn empty_manifests_and_diagnostics_produce_no_operations() {
    let inputs = PlanInputs {
        z_dir: std::env::temp_dir().join("pzms_fixes_no_inputs"),
        manifests: Vec::new(),
        diagnostics: Vec::new(),
    };
    let ops = build_operations(&inputs);
    assert!(ops.is_empty(), "no input can never mean a change");
}

#[test]
fn corrupt_load_order_files_are_left_alone() {
    // Binary junk where a load-order list should be: the engine must skip the
    // file rather than overwrite it with content derived from a different file.
    let sb = Sandbox::new("corrupt_load_order");
    sb.install_mod("Alpha", "Alpha", "incompatible=Beta\n");
    sb.install_mod("Beta", "Beta", "");
    sb.write("Lua/ModListData.ini", &render_ini("ModList", "activeMods", &["Alpha".to_string(), "Beta".to_string()]));
    sb.write("mods.txt", "\u{0}\u{1}\u{2}\u{3} not a list at all");
    let junk = sb.read("mods.txt");

    let plan = build_plan(&sb.path());
    apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    assert_eq!(sb.read("mods.txt"), junk, "an unparsable file must not be rewritten");
}

#[test]
fn a_version_directive_that_is_already_decimal_is_not_touched() {
    let sb = Sandbox::new("already_decimal");
    sb.write_load_order(&["Alpha"]);
    sb.install_mod("Alpha", "Alpha", "versionMin=42.0\n");
    let before = snapshot_tree(&sb.root);
    let plan = build_plan(&sb.path());
    assert!(
        !plan.changes
            .iter()
            .any(|c| c.target_path.ends_with("mod.info")),
        "a correct versionMin must not be planned for a rewrite"
    );
    assert_eq!(before, snapshot_tree(&sb.root));
}

// ---------------------------------------------------------------------------
// version-directive patch
// ---------------------------------------------------------------------------

#[test]
fn patch_bare_integer_version_only_touches_bare_integers() {
    assert_eq!(
        patch_bare_integer_version("versionMin=42\n").as_deref(),
        Some("versionMin=42.0\n")
    );
    assert_eq!(patch_bare_integer_version("versionMin=42.0\n"), None);
    assert_eq!(patch_bare_integer_version("versionMin=41.15\n"), None);
    assert_eq!(patch_bare_integer_version("versionMin=\n"), None);
    assert_eq!(patch_bare_integer_version("name=42\n"), None);
    assert_eq!(patch_bare_integer_version(""), None);
}

#[test]
fn patch_bare_integer_version_preserves_every_other_line() {
    let text = "name=Thing\nid=Thing\nversionMin=42\nrequire=Other\n";
    let fixed = patch_bare_integer_version(text).expect("bare integer must be patched");
    assert_eq!(fixed, "name=Thing\nid=Thing\nversionMin=42.0\nrequire=Other\n");
}

#[test]
fn patch_bare_integer_version_preserves_crlf_and_indentation() {
    let text = "  versionMin=42\r\nname=Thing\r\n";
    let fixed = patch_bare_integer_version(text).expect("bare integer must be patched");
    assert_eq!(fixed, "  versionMin=42.0\r\nname=Thing\r\n");
}

#[test]
fn patch_bare_integer_version_handles_a_file_without_a_trailing_newline() {
    assert_eq!(
        patch_bare_integer_version("versionMin=41").as_deref(),
        Some("versionMin=41.0")
    );
}

#[test]
fn a_bare_integer_version_in_a_real_mod_info_is_fixed_and_reversible() {
    let sb = Sandbox::new("patch_version");
    sb.write_load_order(&["Thing"]);
    sb.write(
        "mods/Thing/mod.info",
        "name=Thing\nid=Thing\nversionMin=42\npzversion=42\n",
    );
    let original_info = sb.read("mods/Thing/mod.info");

    let plan = build_plan(&sb.path());
    let change = plan
        .changes
        .iter()
        .find(|c| c.target_path.ends_with("mod.info"))
        .expect("a bare integer versionMin must produce a change");
    assert_eq!(change.kind, FixKind::PatchFile);
    assert!(change.reversible);
    assert!(change.diff_preview.as_deref().unwrap_or("").contains("42.0"));

    assert_eq!(
        sb.read("mods/Thing/mod.info"),
        original_info,
        "preview must not touch the mod.info"
    );

    let applied = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    assert!(applied.errors.is_empty(), "{:?}", applied.errors);
    assert_eq!(
        sb.read("mods/Thing/mod.info"),
        // Both bare-integer version keys are corrected; nothing else moves.
        "name=Thing\nid=Thing\nversionMin=42.0\npzversion=42.0\n",
        "only the bare-integer version lines change"
    );

    let restored = restore_backup_impl(&sb.path(), &applied.backups[0].backup_id);
    assert!(restored.errors.is_empty());
    assert_eq!(sb.read("mods/Thing/mod.info"), original_info);
}

#[test]
fn a_non_utf8_mod_info_is_never_rewritten() {
    let sb = Sandbox::new("non_utf8");
    sb.write_load_order(&["Thing"]);
    sb.write("mods/Thing/mod.info", "name=Thing\nid=Thing\n");
    let info = sb.root.join("mods/Thing/mod.info");
    // Latin-1 bytes that are not valid UTF-8.
    std::fs::write(&info, [b'n', b'a', b'm', b'e', b'=', 0xE9, b'\n', b'i', b'd', b'=', b'T', b'h', b'i', b'n', b'g', b'\n'])
        .expect("fixture write");

    let plan = build_plan(&sb.path());
    assert!(
        !plan.changes.iter().any(|c| c.target_path.ends_with("mod.info")),
        "invalid UTF-8 must never be rewritten, only replaced"
    );
    let applied = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    assert!(!applied.errors.iter().any(|e| e.contains("mod.info")));
}

// ---------------------------------------------------------------------------
// survivor selection
// ---------------------------------------------------------------------------

#[test]
fn survivor_is_the_mod_something_else_depends_on() {
    let mut lib = manifest("Library", false);
    lib.require = Vec::new();
    let mut addon = manifest("Addon", true);
    addon.require = vec!["Library".to_string()];

    let (keep, drop_id) = pick_survivor(&[lib, addon], &["Addon".to_string(), "Library".to_string()])
        .expect("a pair must yield a survivor");
    assert_eq!(keep, "Library");
    assert_eq!(drop_id, "Addon");
}

#[test]
fn survivor_is_deterministic_under_input_order() {
    let mut a = manifest("Alpha", true);
    a.require = Vec::new();
    let mut b = manifest("Beta", true);
    b.require = Vec::new();

    let first = pick_survivor(&[a.clone(), b.clone()], &["Alpha".to_string(), "Beta".to_string()]);
    let second = pick_survivor(&[b, a], &["Alpha".to_string(), "Beta".to_string()]);
    assert_eq!(first, second, "the same pair must always resolve the same way");
    let (keep, drop_id) = first.expect("a pair must yield a survivor");
    assert_eq!(keep, "Alpha");
    assert_eq!(drop_id, "Beta");
}

#[test]
fn survivor_keeps_the_enabled_mod_when_nothing_else_separates_them() {
    let on = manifest("Alpha", true);
    let off = manifest("Beta", false);
    let (keep, drop_id) = pick_survivor(&[on, off], &["Beta".to_string(), "Alpha".to_string()])
        .expect("a pair must yield a survivor");
    assert_eq!(keep, "Alpha");
    assert_eq!(drop_id, "Beta");
}

#[test]
fn survivor_needs_at_least_two_mods() {
    assert!(pick_survivor(&[], &[]).is_none());
    assert!(pick_survivor(&[manifest("A", true)], &["A".to_string()]).is_none());
}

// ---------------------------------------------------------------------------
// reconcile
// ---------------------------------------------------------------------------

#[test]
fn reconcile_removes_the_disabled_mod_only() {
    let current = vec!["Alpha".to_string(), "Beta".to_string(), "Gamma".to_string()];
    let out = reconcile_active_list(
        &current,
        &[("Alpha".to_string(), "Beta".to_string())],
        &BTreeSet::new(),
        &[],
    );
    assert!(!ids(&out).contains(&"beta".to_string()), "Beta must be gone");
    assert!(ids(&out).contains(&"alpha".to_string()), "Alpha must stay");
    assert!(ids(&out).contains(&"gamma".to_string()), "Gamma must stay");
}

#[test]
fn reconcile_adds_the_re_enabled_mod() {
    let current = vec!["Alpha".to_string()];
    let reenable: BTreeSet<String> = ["Library".to_string()].into_iter().collect();
    let out = reconcile_active_list(&current, &[], &reenable, &[]);
    assert!(ids(&out).contains(&"library".to_string()));
    assert!(ids(&out).contains(&"alpha".to_string()));
}

#[test]
fn reconcile_does_not_duplicate_a_re_enabled_mod_already_present() {
    let current = vec!["Alpha".to_string(), "Library".to_string()];
    let reenable: BTreeSet<String> = ["Library".to_string()].into_iter().collect();
    let out = reconcile_active_list(&current, &[], &reenable, &[]);
    assert_eq!(ids(&out).iter().filter(|i| *i == "library").count(), 1);
}

#[test]
fn reconcile_brings_back_a_disabled_required_library() {
    // Beta is installed but off, and Alpha needs it: it must come back on.
    let mut alpha = manifest("Alpha", true);
    alpha.require = vec!["Beta".to_string()];
    let beta = manifest("Beta", false);
    let out = reconcile_active_list(
        &["Alpha".to_string()],
        &[],
        &BTreeSet::new(),
        &[alpha, beta],
    );
    assert!(
        ids(&out).contains(&"beta".to_string()),
        "a disabled required library must be switched back on, got {:?}",
        out
    );
}

#[test]
fn reconcile_does_not_switch_on_unrelated_disabled_mods() {
    let alpha = manifest("Alpha", true);
    let unrelated = manifest("Unrelated", false);
    let out = reconcile_active_list(&["Alpha".to_string()], &[], &BTreeSet::new(), &[alpha, unrelated]);
    assert!(
        !ids(&out).contains(&"unrelated".to_string()),
        "an unrelated disabled mod must stay off, got {:?}",
        out
    );
}

#[test]
fn reconcile_puts_required_libraries_before_their_dependents() {
    let mut addon = manifest("Addon", true);
    addon.require = vec!["Library".to_string()];
    let library = manifest("Library", true);
    // Deliberately reversed input order.
    let out = reconcile_active_list(
        &["Addon".to_string(), "Library".to_string()],
        &[],
        &BTreeSet::new(),
        &[library, addon],
    );
    let lower = ids(&out);
    let lib_pos = lower.iter().position(|i| i == "library").expect("Library present");
    let addon_pos = lower.iter().position(|i| i == "addon").expect("Addon present");
    assert!(lib_pos < addon_pos, "Library must load before Addon, got {:?}", out);
}

#[test]
fn reconcile_preserves_ids_it_knows_nothing_about() {
    let out = reconcile_active_list(
        &["Alpha".to_string(), "SomethingUnknown".to_string()],
        &[("Alpha".to_string(), "Alpha".to_string())],
        &BTreeSet::new(),
        &[],
    );
    assert!(
        ids(&out).contains(&"somethingunknown".to_string()),
        "an id with no manifest must be preserved, got {:?}",
        out
    );
}

#[test]
fn reconcile_on_an_empty_list_yields_an_empty_list() {
    let out = reconcile_active_list(&[], &[("A".to_string(), "B".to_string())], &BTreeSet::new(), &[]);
    assert!(out.is_empty());
}

// ---------------------------------------------------------------------------
// plan shape
// ---------------------------------------------------------------------------

#[test]
fn an_empty_plan_still_reports_confirmation_not_required() {
    let sb = Sandbox::new("empty_plan_shape");
    let plan = build_plan(&sb.path());
    assert!(!plan.requires_confirmation);
    assert_eq!(plan.affected_files, 0);
    assert!(plan.summary.contains("Nothing to fix"));
}

#[test]
fn affected_files_counts_distinct_paths() {
    let sb = Sandbox::new("affected_files");
    sb.standard();
    let plan = build_plan(&sb.path());
    let distinct: BTreeSet<String> = plan.changes.iter().map(|c| c.target_path.clone()).collect();
    assert_eq!(plan.affected_files, distinct.len());
    assert!(plan.affected_files >= 2, "load order plus mod.info");
}

#[test]
fn fix_kind_serializes_as_snake_case() {
    for (kind, expected) in [
        (FixKind::ReorderLoadOrder, "reorder_load_order"),
        (FixKind::DisableMod, "disable_mod"),
        (FixKind::ReenableMod, "reenable_mod"),
        (FixKind::ApplyMerge, "apply_merge"),
        (FixKind::PatchFile, "patch_file"),
    ] {
        let json = serde_json::to_string(&kind).expect("FixKind must serialize");
        assert_eq!(json, format!("\"{}\"", expected));
    }
}

#[test]
fn every_plan_field_is_serializable() {
    let sb = Sandbox::new("serialize");
    sb.standard();
    let plan = build_plan(&sb.path());
    let json = serde_json::to_string(&plan).expect("FixPlan must serialize");
    for key in [
        "plan_id",
        "summary",
        "changes",
        "requires_confirmation",
        "affected_files",
    ] {
        assert!(json.contains(key), "{} must be present in the wire format", key);
    }
    let round: FixPlan = serde_json::from_str(&json).expect("FixPlan must round-trip");
    assert_eq!(round.plan_id, plan.plan_id);

    let applied = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);
    let json = serde_json::to_string(&applied).expect("ApplyResult must serialize");
    for key in ["applied", "skipped", "backups", "errors"] {
        assert!(json.contains(key), "{} must be present", key);
    }

    let r = restore_backup_impl(&sb.path(), &applied.backups[0].backup_id);
    let json = serde_json::to_string(&r).expect("RestoreResult must serialize");
    assert!(json.contains("restored"));
    assert!(json.contains("errors"));
}

#[test]
fn apply_result_lists_the_paths_it_touched() {
    let sb = Sandbox::new("applied_paths");
    sb.standard();
    let plan = build_plan(&sb.path());
    let applied = apply_plan(&sb.path(), &plan.plan_id, &plan.plan_id);

    let changed_ids: BTreeSet<String> = plan
        .changes
        .iter()
        .map(|c| c.change_id.clone())
        .collect();
    for id in &applied.applied {
        assert!(changed_ids.contains(id), "{} must be a previewed change", id);
    }
    let mut sorted = applied.applied.clone();
    sorted.sort();
    assert_eq!(applied.applied, sorted, "applied must be sorted");
}

#[test]
fn error_codes_are_stable_strings() {
    assert_eq!(FixErrorCode::PlanStale.as_str(), "PLAN_STALE");
    assert_eq!(FixErrorCode::UnknownPlan.as_str(), "UNKNOWN_PLAN");
    assert_eq!(
        FixErrorCode::ConfirmationRequired.as_str(),
        "CONFIRMATION_REQUIRED"
    );
    assert_eq!(FixErrorCode::NothingToDo.as_str(), "NOTHING_TO_DO");
    assert_eq!(FixErrorCode::Io.as_str(), "IO_ERROR");
}

#[test]
fn every_error_starts_with_a_machine_readable_code() {
    let sb = Sandbox::new("error_codes");
    sb.write_load_order(&["Alpha", "Beta"]);
    let known = [
        FixErrorCode::PlanStale.as_str(),
        FixErrorCode::UnknownPlan.as_str(),
        FixErrorCode::ConfirmationRequired.as_str(),
        FixErrorCode::NothingToDo.as_str(),
        FixErrorCode::Io.as_str(),
    ];
    for (plan_id, token) in [("", ""), ("nope", "nope"), ("nope", "")] {
        let result = apply_plan(&sb.path(), plan_id, token);
        for e in &result.errors {
            assert!(
                known.iter().any(|k| e.starts_with(k)),
                "error {:?} must start with a known code",
                e
            );
        }
    }
}

// ---------------------------------------------------------------------------
// atomic write helper
// ---------------------------------------------------------------------------

#[test]
fn atomic_write_replaces_content_and_leaves_no_temp_file() {
    let sb = Sandbox::new("atomic");
    let target = sb.write("thing.txt", "old\n");
    write_atomic(&target, b"new\n").expect("write must succeed");
    assert_eq!(std::fs::read_to_string(&target).unwrap_or_default(), "new\n");
    let leftovers: Vec<String> = snapshot_tree(&sb.root)
        .into_iter()
        .map(|(p, _)| p)
        .filter(|p| p.contains("pzms_tmp"))
        .collect();
    assert!(leftovers.is_empty(), "no temp file may survive: {:?}", leftovers);
}

#[test]
fn backup_file_names_are_path_safe() {
    for name in ["default.txt", "ModListData.ini", "PZ Mod Studio.txt", "a b-c.d"] {
        let p = PathBuf::from("x").join(name);
        let safe = backup_file_name(&p);
        assert!(
            safe.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_'),
            "{} must be path-safe",
            safe
        );
        assert!(safe.ends_with(".original"));
    }
}

// ---------------------------------------------------------------------------
// primary directory resolution
// ---------------------------------------------------------------------------

#[test]
fn explicit_dir_wins_over_autodetect() {
    let p = primary_zomboid_dir("C:/custom/Zomboid");
    assert_eq!(p, PathBuf::from("C:/custom/Zomboid"));
}

#[test]
fn whitespace_only_dir_falls_back_without_panicking() {
    let p = primary_zomboid_dir("   ");
    assert!(!p.as_os_str().is_empty());
}