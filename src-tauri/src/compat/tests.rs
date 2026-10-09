use super::*;
use crate::load_order::mod_info::ModManifest;

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

const SAMPLE: &str = r#"{
  "schema_version": 1,
  "incompatible_pairs": [
    {"mods": ["Alpha Mod", "Beta Mod"], "reason": "both rewrite the same Lua table", "severity": "error"},
    {"mods": ["Gamma", "Delta", "Epsilon"], "reason": "three-way conflict", "severity": "warning"}
  ],
  "required_libraries": [
    {"mod": "Alpha Mod", "library": "Core Lib", "severity": "error", "reason": "hooks its API"}
  ],
  "game_versions": [
    {"mod": "Alpha Mod", "min_game_build": "42.0", "max_game_build": null, "severity": "warning", "reason": "b42 only"}
  ]
}"#;

// --- schema parse ----------------------------------------------------------

#[test]
fn parses_a_well_formed_catalog() {
    let db = parse_database(SAMPLE).expect("sample must parse");
    assert_eq!(db.schema_version, 1);
    assert_eq!(db.incompatible_pairs.len(), 2);
    assert_eq!(db.required_libraries.len(), 1);
    assert_eq!(db.game_versions.len(), 1);
}

#[test]
fn bundled_resource_file_parses() {
    let db = parse_database(EMBEDDED_JSON).expect("embedded copy must parse");
    assert_eq!(db.schema_version, SUPPORTED_SCHEMA_VERSION);
}

#[test]
fn shipped_resource_file_parses() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(RESOURCE_FILE_NAME);
    let text = std::fs::read_to_string(&path).expect("resources/compatibility.json must exist");
    let db = parse_database(&text).expect("bundled resource must parse");
    assert_eq!(db.schema_version, SUPPORTED_SCHEMA_VERSION);
}

#[test]
fn missing_fields_default_instead_of_failing() {
    let db = parse_database(r#"{"schema_version":1}"#).expect("bare catalog must parse");
    assert!(db.incompatible_pairs.is_empty());
    assert!(db.required_libraries.is_empty());
    assert!(db.game_versions.is_empty());
}

#[test]
fn severity_defaults_to_error_when_absent() {
    let db = parse_database(r#"{"schema_version":1,"incompatible_pairs":[{"mods":["a","b"]}]}"#)
        .expect("catalog without severity must parse");
    assert_eq!(
        db.incompatible_pairs[0].severity,
        CompatSeverity::Error,
        "an omitted severity must be treated as an error"
    );
}

#[test]
fn rejects_malformed_json_with_a_message() {
    let err = parse_database("{ this is not json ").expect_err("malformed json must be rejected");
    assert!(!err.is_empty());
}

#[test]
fn rejects_schema_version_zero() {
    let err = parse_database(r#"{"schema_version":0}"#).expect_err("v0 must be rejected");
    assert!(err.contains("schema_version"));
}

#[test]
fn rejects_future_schema_version() {
    let err = parse_database(r#"{"schema_version":99}"#).expect_err("v99 must be rejected");
    assert!(err.contains("newer"));
}

#[test]
fn empty_string_is_rejected_not_panicking() {
    assert!(parse_database("").is_err());
    assert!(parse_database("[]").is_err());
    assert!(parse_database("null").is_err());
}

// --- pair symmetry ---------------------------------------------------------

#[test]
fn pair_index_is_symmetric() {
    let db = parse_database(SAMPLE).expect("sample must parse");
    let idx = CompatIndex::build(&db);

    let forward = idx.rule_for_pair("Alpha Mod", "Beta Mod");
    let backward = idx.rule_for_pair("beta mod", "alpha mod");
    assert!(forward.is_some(), "a<b pair must be found");
    assert!(backward.is_some(), "b<a must find the same rule");

    let f = forward.expect("pair rule");
    let b = backward.expect("pair rule");
    assert_eq!(f.reason, b.reason);
    assert_eq!(f.severity, b.severity);
}

#[test]
fn pair_index_normalizes_ids_the_same_way_the_detectors_do() {
    let db = parse_database(SAMPLE).expect("sample must parse");
    let idx = CompatIndex::build(&db);
    // normalize_id lowercases and strips spaces, so case and spacing in the
    // JSON cannot decide whether a rule matches an installed mod.
    assert!(idx.rule_for_pair("ALPHA MOD", "beta mod").is_some());
    assert!(idx.rule_for_pair("alpha mod", "BETA  MOD").is_some());

    // normalize_id maps '-' to '_', so a rule written "Alpha-Mod" matches an
    // installed "Alpha Mod" only after both sides collapse the same way.
    let dashed = parse_database(
        r#"{"schema_version":1,"incompatible_pairs":[{"mods":["Alpha-Mod","Beta-Mod"],"reason":"d"}]}"#,
    )
    .expect("catalog must parse");
    let dashed_idx = CompatIndex::build(&dashed);
    assert!(dashed_idx.rule_for_pair("alpha_mod", "beta_mod").is_some());
}

#[test]
fn pair_index_dedupes_an_unordered_pair_declared_twice() {
    let db = parse_database(
        r#"{"schema_version":1,"incompatible_pairs":[
            {"mods":["x","y"],"reason":"first"},
            {"mods":["y","x"],"reason":"second"}
        ]}"#,
    )
    .expect("catalog must parse");
    let idx = CompatIndex::build(&db);
    assert_eq!(idx.pair_count(), 1, "reversed duplicate must collapse");
}

#[test]
fn n_mod_rule_covers_every_unordered_pair() {
    let db = parse_database(SAMPLE).expect("sample must parse");
    let idx = CompatIndex::build(&db);
    // "Gamma","Delta","Epsilon" expands to 3 unordered pairs.
    assert!(idx.rule_for_pair("Gamma", "Delta").is_some());
    assert!(idx.rule_for_pair("Delta", "Epsilon").is_some());
    assert!(idx.rule_for_pair("Gamma", "Epsilon").is_some());
    // Plus the one from the 2-mod rule.
    assert_eq!(idx.pair_count(), 4);
}

#[test]
fn malformed_pair_entries_are_dropped_not_fatal() {
    let db = parse_database(
        r#"{"schema_version":1,"incompatible_pairs":[
            {"mods":["only_one"]},
            {"mods":["",""],"reason":"both empty"},
            {"mods":["keep","this"],"reason":"valid"}
        ]}"#,
    )
    .expect("catalog must parse");
    let idx = CompatIndex::build(&db);
    assert_eq!(idx.pair_count(), 1, "only the 2-distinct-id rule survives");
    assert!(idx.rule_for_pair("keep", "this").is_some());
}

// --- detection -------------------------------------------------------------

#[test]
fn catalog_pair_produces_one_diagnostic_and_is_sorted() {
    let db = parse_database(SAMPLE).expect("sample must parse");
    let idx = CompatIndex::build(&db);
    let mods = vec![
        manifest("Beta Mod", true),
        manifest("Alpha Mod", true),
        manifest("Unrelated", true),
    ];
    let diags = detect_catalog_pairs(&idx, &mods);
    assert_eq!(diags.len(), 1, "Unrelated must not be dragged in");
    assert_eq!(diags[0].kind, ConflictKind::IncompatiblePair);
    assert_eq!(diags[0].severity, Severity::Error);
    // The diagnostic carries the mod ids exactly as the manifests spelled them,
    // matching what `conflicts::detect_incompatible_pairs` already emits. Only
    // the *lookup* is normalized.
    assert_eq!(diags[0].mod_ids, vec!["Alpha Mod".to_string(), "Beta Mod".to_string()]);
    assert_eq!(diags[0].severity, Severity::Error);
}

#[test]
fn catalog_pair_ignores_absent_mods() {
    let db = parse_database(SAMPLE).expect("sample must parse");
    let idx = CompatIndex::build(&db);
    let diags = detect_catalog_pairs(&idx, &[manifest("Alpha Mod", true)]);
    assert!(diags.is_empty(), "a half-installed pair is not a conflict");
}

#[test]
fn catalog_pair_skips_when_both_entries_are_the_same_mod() {
    let db = parse_database(r#"{"schema_version":1,"incompatible_pairs":[{"mods":["dup"],"reason":"self"}]}"#)
        .expect("catalog must parse");
    let idx = CompatIndex::build(&db);
    let diags = detect_catalog_pairs(&idx, &[manifest("Dup", true), manifest("dup", true)]);
    assert!(diags.is_empty(), "duplicates are the duplicate detector's job");
}

#[test]
fn library_constraint_fires_when_library_missing() {
    let db = parse_database(SAMPLE).expect("sample must parse");
    let idx = CompatIndex::build(&db);
    let diags = detect_catalog_libraries(&idx, &[manifest("Alpha Mod", true)]);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].kind, ConflictKind::RequiredLibraryVersion);
    assert_eq!(diags[0].related_mod_ids, vec!["Core Lib".to_string()]);
}

#[test]
fn library_constraint_silent_when_library_present_and_enabled() {
    let db = parse_database(SAMPLE).expect("sample must parse");
    let idx = CompatIndex::build(&db);
    let diags = detect_catalog_libraries(
        &idx,
        &[manifest("Alpha Mod", true), manifest("Core Lib", true)],
    );
    assert!(diags.is_empty(), "satisfied constraint must not be reported");
}

#[test]
fn library_constraint_yields_to_manifest_detector_when_library_disabled() {
    // When the library is installed but off, `conflicts::detect_missing_dependencies`
    // already speaks. Emitting both would double-report the same problem.
    let db = parse_database(SAMPLE).expect("sample must parse");
    let idx = CompatIndex::build(&db);
    let diags = detect_catalog_libraries(
        &idx,
        &[manifest("Alpha Mod", true), manifest("Core Lib", false)],
    );
    assert!(diags.is_empty(), "disabled-but-installed is the manifest path");
}

#[test]
fn game_version_range_flags_build_below_min() {
    let db = parse_database(SAMPLE).expect("sample must parse");
    let idx = CompatIndex::build(&db);
    let diags = detect_catalog_game_versions(&idx, &[manifest("Alpha Mod", true)], Some("41.15"));
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].kind, ConflictKind::GameVersionMismatch);
    assert_eq!(diags[0].severity, Severity::Warning);
}

#[test]
fn game_version_range_silent_on_supported_build() {
    let db = parse_database(SAMPLE).expect("sample must parse");
    let idx = CompatIndex::build(&db);
    let diags = detect_catalog_game_versions(&idx, &[manifest("Alpha Mod", true)], Some("42.30"));
    assert!(diags.is_empty());
}

#[test]
fn game_version_range_flags_build_above_max() {
    let db = parse_database(
        r#"{"schema_version":1,"game_versions":[{"mod":"Old","max_game_build":"41.99"}]}"#,
    )
    .expect("catalog must parse");
    let idx = CompatIndex::build(&db);
    assert_eq!(
        detect_catalog_game_versions(&idx, &[manifest("Old", true)], Some("42.0")).len(),
        1
    );
    assert!(detect_catalog_game_versions(&idx, &[manifest("Old", true)], Some("41.15")).is_empty());
}

#[test]
fn game_version_unknown_build_emits_nothing() {
    let db = parse_database(SAMPLE).expect("sample must parse");
    let idx = CompatIndex::build(&db);
    for build in [None, Some(""), Some("   ")] {
        let diags = detect_catalog_game_versions(&idx, &[manifest("Alpha Mod", true)], build);
        assert!(diags.is_empty(), "an unknown build is not a mismatch");
    }
}

#[test]
fn version_comparison_uses_existing_numeric_semantics() {
    let db = parse_database(SAMPLE).expect("sample must parse");
    let idx = CompatIndex::build(&db);
    let mods = vec![manifest("Alpha Mod", true)];
    // 42.10 > 42.9, so it must be accepted.
    assert!(detect_catalog_game_versions(&idx, &mods, Some("42.10")).is_empty());
    // 41 == 41.0 == 41.0.0, all below 42.0.
    for b in ["41", "41.0", "41.0.0"] {
        assert_eq!(
            detect_catalog_game_versions(&idx, &mods, Some(b)).len(),
            1,
            "{} must be flagged below min",
            b
        );
    }
}

// --- fallback --------------------------------------------------------------

#[test]
fn load_from_missing_path_falls_back_to_embedded() {
    let missing = std::path::PathBuf::from("definitely/not/here/compatibility.json");
    let rules = load_from_path(&missing);
    assert!(rules.is_available());
    assert_eq!(
        rules.status,
        CompatStatus::LoadedEmbedded {
            rule_count: rules.rule_count()
        }
    );
}

#[test]
fn load_from_corrupt_path_falls_back_to_embedded() {
    let dir = std::env::temp_dir().join("pzms_compat_corrupt");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(RESOURCE_FILE_NAME);
    std::fs::write(&path, "{ not json at all ").expect("must be able to write the corrupt fixture");

    let rules = load_from_path(&path);
    assert!(rules.is_available(), "a corrupt file must not be fatal");
    assert!(matches!(rules.status, CompatStatus::LoadedEmbedded { .. }));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn load_from_valid_path_uses_resource_status() {
    let dir = std::env::temp_dir().join("pzms_compat_valid");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(RESOURCE_FILE_NAME);
    std::fs::write(&path, SAMPLE).expect("must be able to write the fixture");

    let rules = load_from_path(&path);
    assert!(matches!(rules.status, CompatStatus::LoadedResource { .. }));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn candidate_paths_are_unique_and_end_with_the_resource_name() {
    let paths = candidate_resource_paths(None);
    assert!(!paths.is_empty());
    for p in &paths {
        assert_eq!(
            p.file_name().and_then(|n| n.to_str()),
            Some(RESOURCE_FILE_NAME),
            "candidate {} must be the resource file itself",
            p.display()
        );
    }
    let mut deduped = paths.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(deduped.len(), paths.len(), "no duplicate candidates");
}

#[test]
fn explicit_resource_dir_is_searched_first() {
    let dir = std::path::PathBuf::from("/tmp/does-not-exist-pzms");
    let paths = candidate_resource_paths(Some(&dir));
    assert_eq!(paths[0], dir.join(RESOURCE_FILE_NAME));
}

#[test]
fn load_default_always_yields_a_usable_ruleset() {
    let rules = load_default(None);
    assert!(
        rules.is_available(),
        "the shipped catalog or its embedded mirror must always load: {}",
        rules.status
    );
    assert!(rules.rule_count() > 0, "the shipped catalog must not be empty");
}

#[test]
fn global_rules_load_exactly_once() {
    let a = global_rules();
    let b = global_rules();
    assert!(std::ptr::eq(a, b), "global rules must be a single cached value");
}

#[test]
fn unavailable_rules_are_reported_as_unavailable_not_as_all_clear() {
    let rules = CompatRules::empty("test");
    assert!(!rules.is_available());
    assert!(!rules.status.is_available());
    let status_text = rules.status.to_string();
    assert!(status_text.contains("unavailable"));
    assert!(status_text.contains("test"));
    // And detection must return nothing rather than pretend everything is fine.
    let diags = analyze_with_catalog(&rules, &[manifest("Alpha Mod", true)], Some("41.0"));
    assert!(diags.is_empty());
}

#[test]
fn status_serializes_with_a_state_tag() {
    let json = serde_json::to_value(CompatStatus::Unavailable {
        reason: "nope".to_string(),
    })
    .expect("status must serialize");
    assert_eq!(json["state"], "unavailable");
    assert_eq!(json["reason"], "nope");

    let json = serde_json::to_value(CompatStatus::LoadedResource { rule_count: 3 })
        .expect("status must serialize");
    assert_eq!(json["state"], "loaded_resource");
    assert_eq!(json["rule_count"], 3);
}

#[test]
fn analyze_with_catalog_is_empty_for_uninstalled_mods() {
    let rules = CompatRules {
        db: parse_database(SAMPLE).expect("sample must parse"),
        status: CompatStatus::LoadedResource { rule_count: 4 },
    };
    let diags = analyze_with_catalog(&rules, &[manifest("Totally Other", true)], Some("42.0"));
    assert!(diags.is_empty());
}

#[test]
fn analyze_with_catalog_covers_pairs_libraries_and_versions() {
    let rules = CompatRules {
        db: parse_database(SAMPLE).expect("sample must parse"),
        status: CompatStatus::LoadedResource { rule_count: 4 },
    };
    let mods = vec![manifest("Alpha Mod", true), manifest("Beta Mod", true)];
    let diags = analyze_with_catalog(&rules, &mods, Some("41.15"));
    // 1 pair + 1 missing library + 1 below-min build.
    assert_eq!(diags.len(), 3);
}