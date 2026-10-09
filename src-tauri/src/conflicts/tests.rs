use super::*;
use std::cmp::Ordering;

fn m(id: &str) -> ModManifest {
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
        enabled: true,
        is_packaged: None,
        inferred_require: Vec::new(),
    }
}

fn with(mut m: ModManifest, require: &[&str]) -> ModManifest {
    m.require = require.iter().map(|s| s.to_string()).collect();
    m
}

// NOTE: a `with_workshop` helper already exists further down this file; do not
// re-add a second one with the same name (E0428).

fn disabled(mut m: ModManifest) -> ModManifest {
    m.enabled = false;
    m
}

// ---------- compare_versions ----------

#[test]
fn version_zero_padding_is_equal() {
    assert_eq!(compare_versions("42", "42.0"), Ordering::Equal);
    assert_eq!(compare_versions("42", "42.0.0"), Ordering::Equal);
    assert_eq!(compare_versions("42.0", "42.0.0"), Ordering::Equal);
}

#[test]
fn version_minor_gt() {
    assert_eq!(compare_versions("42.15.1", "42.15"), Ordering::Greater);
    assert_eq!(compare_versions("42.15", "42.15.1"), Ordering::Less);
}

#[test]
fn version_compares_numerically_not_lexicographically() {
    assert_eq!(compare_versions("42.10", "42.9"), Ordering::Greater);
    assert_eq!(compare_versions("1.2", "1.10"), Ordering::Less);
}

#[test]
fn version_build_ordering_across_releases() {
    assert_eq!(compare_versions("41.21", "42.0"), Ordering::Less);
    assert_eq!(compare_versions("42.15.1", "41.21"), Ordering::Greater);
}

#[test]
fn version_strips_leading_v() {
    assert_eq!(compare_versions("v42.1", "42.1"), Ordering::Equal);
    assert_eq!(compare_versions("V1.0", "1.0"), Ordering::Equal);
}

#[test]
fn version_empty_is_lowest() {
    assert_eq!(compare_versions("", "1.0"), Ordering::Less);
    assert_eq!(compare_versions("1.0", ""), Ordering::Greater);
    assert_eq!(compare_versions("", ""), Ordering::Equal);
    assert_eq!(compare_versions("   ", "1"), Ordering::Less);
}

#[test]
fn version_junk_segments_are_zero() {
    assert_eq!(compare_versions("42.x", "42.0"), Ordering::Equal);
    assert_eq!(compare_versions("abc", "0"), Ordering::Equal);
}

#[test]
fn version_prerelease_suffix() {
    assert_eq!(compare_versions("1.0.0-beta", "1.0.0"), Ordering::Equal);
    assert_eq!(compare_versions("1.0.0-beta", "1.0.1"), Ordering::Less);
}

#[test]
fn version_trims_whitespace() {
    assert_eq!(compare_versions("  42.1  ", "42.1"), Ordering::Equal);
}

#[test]
fn version_single_vs_multi_segment() {
    assert_eq!(compare_versions("42", "42.0.1"), Ordering::Less);
    assert_eq!(compare_versions("42.0.1", "42"), Ordering::Greater);
}

#[test]
fn version_satisfies_min() {
    assert!(satisfies_min_version("42.15", "42.0"));
    assert!(satisfies_min_version("42.0", "42.0"));
    assert!(!satisfies_min_version("41.21", "42.0"));
}

// ---------- extract_declared_keys ----------

#[test]
fn keys_standard_item_block() {
    let k = extract_declared_keys("item MyItem {\n  weight = 1;\n}");
    assert_eq!(k.len(), 1);
    assert_eq!(k[0].keyword, "item");
    assert_eq!(k[0].key, "MyItem");
    assert_eq!(k[0].line, 1);
}

#[test]
fn keys_quoted_name() {
    let k = extract_declared_keys("item 'Foo Bar' {\n}");
    assert_eq!(k[0].key, "Foo Bar");
}

#[test]
fn keys_brace_on_next_line() {
    let k = extract_declared_keys("item MyItem\n{\n}");
    assert_eq!(k.len(), 1);
    assert_eq!(k[0].key, "MyItem");
}

#[test]
fn keys_assignment_form() {
    let k = extract_declared_keys("item MyItem = 'baseItem' {\n}");
    assert_eq!(k.len(), 1);
    assert_eq!(k[0].key, "MyItem");
}

#[test]
fn keys_multiple_declarations() {
    let k = extract_declared_keys("item A {\n}\nrecipe B {\n}\n");
    assert_eq!(k.len(), 2);
    assert_eq!(k[0].keyword, "item");
    assert_eq!(k[1].keyword, "recipe");
    assert_eq!(k[1].key, "B");
    assert_eq!(k[1].line, 3);
}

#[test]
fn keys_crlf_content() {
    let k = extract_declared_keys("item MyItem {\r\n  weight = 1;\r\n}\r\n");
    assert_eq!(k.len(), 1);
    assert_eq!(k[0].key, "MyItem");
}

#[test]
fn keys_empty_content() {
    assert!(extract_declared_keys("").is_empty());
}

#[test]
fn keys_no_declarations() {
    assert!(extract_declared_keys("weight = 10;\nname = 'x';\n").is_empty());
}

#[test]
fn keys_truncated_no_brace_does_not_panic() {
    let k = extract_declared_keys("item Foo");
    assert_eq!(k.len(), 1);
    assert_eq!(k[0].key, "Foo");
}

#[test]
fn keys_unbalanced_braces_do_not_panic() {
    let _ = extract_declared_keys("item A {\nitem B {\n}\n");
    let _ = extract_declared_keys("}\n}\n}\n");
    let _ = extract_declared_keys("{{{{{{");
}

#[test]
fn keys_skip_hash_comments() {
    let k = extract_declared_keys("# item Fake {\nitem Real {\n}");
    assert_eq!(k.len(), 1);
    assert_eq!(k[0].key, "Real");
}

#[test]
fn keys_skip_lua_comments() {
    let k = extract_declared_keys("-- item Fake {\nitem Real {\n}");
    assert_eq!(k.len(), 1);
    assert_eq!(k[0].key, "Real");
}

#[test]
fn keys_nested_keyword_is_not_a_declaration() {
    // `item Type` inside a block body is a property, not a new declaration.
    let k = extract_declared_keys("item Real {\n  item Type = 'Can';\n}");
    assert_eq!(k.len(), 1);
    assert_eq!(k[0].key, "Real");
}

#[test]
fn keys_keyword_inside_string_value_is_ignored() {
    let k = extract_declared_keys("item Real {\n  description = 'item Fake {';\n}");
    assert_eq!(k.len(), 1);
    assert_eq!(k[0].key, "Real");
}

#[test]
fn keys_camel_and_snake_keywords() {
    let k = extract_declared_keys("craftRecipe MyCraft {\n}\nworld_item Spawn {\n}\n");
    assert_eq!(k.len(), 2);
    assert_eq!(k[0].keyword, "craftRecipe");
    assert_eq!(k[1].keyword, "world_item");
}

#[test]
fn keys_prefix_is_not_enough_to_match_keyword() {
    // `itemIcon` must not be read as the `item` keyword.
    assert!(extract_declared_keys("itemIcon Foo {\n}").is_empty());
}

// ---------- detect_data_key_collisions ----------

fn keys_of(mods: &[&str]) -> Vec<DataKey> {
    mods.iter()
        .map(|k| DataKey {
            keyword: "item".to_string(),
            key: k.to_string(),
            line: 1,
        })
        .collect()
}

#[test]
fn data_collision_two_mods_same_key() {
    let g = vec![
        ("ModA".to_string(), keys_of(&["Shared"])),
        ("ModB".to_string(), keys_of(&["Shared"])),
    ];
    let d = detect_data_key_collisions(&g);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].kind, ConflictKind::DataKeyCollision);
    assert_eq!(d[0].mod_ids.len(), 2);
}

#[test]
fn data_collision_same_mod_repeat_is_not_a_collision() {
    let g = vec![("ModA".to_string(), keys_of(&["Dup", "Dup"]))];
    assert!(detect_data_key_collisions(&g).is_empty());
}

#[test]
fn data_collision_different_keywords_same_name() {
    let g = vec![
        (
            "ModA".to_string(),
            vec![DataKey {
                keyword: "item".into(),
                key: "Thing".into(),
                line: 1,
            }],
        ),
        (
            "ModB".to_string(),
            vec![DataKey {
                keyword: "recipe".into(),
                key: "Thing".into(),
                line: 1,
            }],
        ),
    ];
    assert!(detect_data_key_collisions(&g).is_empty());
}

#[test]
fn data_collision_empty_input() {
    assert!(detect_data_key_collisions(&[]).is_empty());
}

#[test]
fn data_collision_single_mod() {
    let g = vec![("ModA".to_string(), keys_of(&["Only"]))];
    assert!(detect_data_key_collisions(&g).is_empty());
}

#[test]
fn data_collision_three_mods_one_entry() {
    let g = vec![
        ("A".to_string(), keys_of(&["Shared"])),
        ("B".to_string(), keys_of(&["Shared"])),
        ("C".to_string(), keys_of(&["Shared"])),
    ];
    let d = detect_data_key_collisions(&g);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].mod_ids.len(), 3);
}

#[test]
fn data_collision_ignores_blank_mod_id() {
    let g = vec![
        ("".to_string(), keys_of(&["Shared"])),
        ("B".to_string(), keys_of(&["Shared"])),
    ];
    assert!(detect_data_key_collisions(&g).is_empty());
}

// ---------- detect_incompatible_pairs ----------

fn inc(mut mm: ModManifest, ids: &[&str]) -> ModManifest {
    mm.incompatible = ids.iter().map(|s| s.to_string()).collect();
    mm
}

#[test]
fn incompatible_a_declares_b() {
    let v = vec![inc(m("A"), &["B"]), m("B")];
    let d = detect_incompatible_pairs(&v);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].severity, Severity::Error);
}

#[test]
fn incompatible_b_declares_a() {
    let v = vec![m("A"), inc(m("B"), &["A"])];
    assert_eq!(detect_incompatible_pairs(&v).len(), 1);
}

#[test]
fn incompatible_mutual_declares_yields_one_diagnostic() {
    let v = vec![inc(m("A"), &["B"]), inc(m("B"), &["A"])];
    assert_eq!(detect_incompatible_pairs(&v).len(), 1);
}

#[test]
fn incompatible_self_is_ignored() {
    let v = vec![inc(m("A"), &["A"])];
    assert!(detect_incompatible_pairs(&v).is_empty());
}

#[test]
fn incompatible_unknown_id_is_ignored() {
    let v = vec![inc(m("A"), &["NotInstalled"])];
    assert!(detect_incompatible_pairs(&v).is_empty());
}

#[test]
fn incompatible_is_case_and_dash_insensitive() {
    let v = vec![inc(m("MyMod"), &["other-mod"]), m("OTHER-MOD")];
    assert_eq!(detect_incompatible_pairs(&v).len(), 1);
}

#[test]
fn incompatible_empty_list() {
    assert!(detect_incompatible_pairs(&[]).is_empty());
}

#[test]
fn incompatible_severity_is_error() {
    let v = vec![inc(m("A"), &["B"]), m("B")];
    assert_eq!(detect_incompatible_pairs(&v)[0].severity, Severity::Error);
}

// ---------- detect_duplicate_mods ----------

#[test]
fn duplicate_exact_same_id() {
    let v = vec![m("A"), m("A")];
    assert_eq!(detect_duplicate_mods(&v).len(), 1);
}

#[test]
fn duplicate_normalized_id_equivalent() {
    let v = vec![m("My-Mod"), m("MY-MOD")];
    assert_eq!(detect_duplicate_mods(&v).len(), 1);
}

/// Guards the known app-wide quirk: because spaces are deleted rather than turned
/// into separators, these two ids do NOT collide. Documented, not silently fixed.
#[test]
fn duplicate_space_and_dash_ids_do_not_collide() {
    let v = vec![m("My-Mod"), m("My Mod")];
    assert!(detect_duplicate_mods(&v).is_empty());
}

#[test]
fn duplicate_different_ids_not_flagged() {
    let v = vec![m("A"), m("B")];
    assert!(detect_duplicate_mods(&v).is_empty());
}

#[test]
fn duplicate_single_manifest() {
    assert!(detect_duplicate_mods(&[m("A")]).is_empty());
}

#[test]
fn duplicate_empty_list() {
    assert!(detect_duplicate_mods(&[]).is_empty());
}

#[test]
fn duplicate_detail_mentions_both_versions() {
    let mut a = m("A");
    a.version = Some("1.0".into());
    let mut b = m("A");
    b.version = Some("2.0".into());
    let d = detect_duplicate_mods(&[a, b]);
    let detail = d[0].detail.as_deref().unwrap_or("");
    assert!(detail.contains("1.0"), "detail was: {}", detail);
    assert!(detail.contains("2.0"), "detail was: {}", detail);
}

#[test]
fn duplicate_severity_is_warning() {
    let d = detect_duplicate_mods(&[m("A"), m("A")]);
    assert_eq!(d[0].severity, Severity::Warning);
}

// ---------- detect_missing_dependencies ----------

#[test]
fn missing_dependency_satisfied() {
    let v = vec![with(m("A"), &["B"]), m("B")];
    assert!(detect_missing_dependencies(&v).is_empty());
}

#[test]
fn missing_dependency_absent_is_error() {
    let v = vec![with(m("A"), &["Lib"])];
    let d = detect_missing_dependencies(&v);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].severity, Severity::Error);
    assert_eq!(d[0].related_mod_ids, vec!["Lib".to_string()]);
}

#[test]
fn missing_dependency_installed_but_disabled_is_warning() {
    let v = vec![with(m("A"), &["B"]), disabled(m("B"))];
    let d = detect_missing_dependencies(&v);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].severity, Severity::Warning);
}

#[test]
fn missing_dependency_ignores_load_mod_after() {
    let mut a = m("A");
    a.load_mod_after = vec!["Ghost".into()];
    let v = vec![a, m("B")];
    assert!(detect_missing_dependencies(&v).is_empty());
}

#[test]
fn missing_dependency_empty_list() {
    assert!(detect_missing_dependencies(&[]).is_empty());
}

#[test]
fn missing_dependency_self_reference_ignored() {
    let v = vec![with(m("A"), &["A"])];
    assert!(detect_missing_dependencies(&v).is_empty());
}

#[test]
fn missing_dependency_prefix_match_counts_as_found() {
    let v = vec![with(m("A"), &["MyLib"]), m("MyLibExtended")];
    assert!(detect_missing_dependencies(&v).is_empty());
}

#[test]
fn missing_dependency_cause_is_plain_language() {
    let v = vec![with(m("A"), &["GhostLib"])];
    let cause = detect_missing_dependencies(&v)[0].cause.clone();
    assert!(!cause.contains('_'), "cause leaked a symbol: {}", cause);
    assert!(cause.to_lowercase().contains("not installed"));
}

// ---------- find_dependency_cycles ----------

#[test]
fn cycle_simple_two_node() {
    let v = vec![with(m("A"), &["B"]), with(m("B"), &["A"])];
    assert_eq!(find_dependency_cycles(&v), vec![vec!["a".to_string(), "b".to_string()]]);
}

#[test]
fn cycle_three_node() {
    let v = vec![
        with(m("A"), &["B"]),
        with(m("B"), &["C"]),
        with(m("C"), &["A"]),
    ];
    assert_eq!(find_dependency_cycles(&v).len(), 1);
    assert_eq!(find_dependency_cycles(&v)[0].len(), 3);
}

#[test]
fn cycle_self_loop() {
    let v = vec![with(m("A"), &["A2"]), with(m("A2"), &["A2"])];
    let c = find_dependency_cycles(&v);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0], vec!["a2".to_string()]);
}

#[test]
fn cycle_acyclic_graph_is_empty() {
    let v = vec![with(m("A"), &["B"]), with(m("B"), &["C"]), m("C")];
    assert!(find_dependency_cycles(&v).is_empty());
}

#[test]
fn cycle_detected_through_a_workshop_id_requirement() {
    // A requires B by its Steam Workshop id, not by name. Cycle detection must
    // see this edge, otherwise a real load-order deadlock is reported as clean.
    let v = vec![
        with_workshop(with(m("A"), &["2200148440"]), "1111111111"),
        with_workshop(with(m("B"), &["1111111111"]), "2200148440"),
    ];
    let c = find_dependency_cycles(&v);
    assert_eq!(c.len(), 1, "A -> B (workshop id) -> A should be one cycle");
    assert!(c[0].contains(&"a".to_string()));
    assert!(c[0].contains(&"b".to_string()));
}

#[test]
fn cycle_detected_through_a_suffix_matched_requirement() {
    // The real-world `require=modoptions` / `B42_GlobalModOptions` shape. The
    // old local exact+prefix resolver could not see this edge at all.
    let v = vec![
        with(m("Brita"), &["modoptions"]),
        m("B42_GlobalModOptions"),
        with(m("B42_GlobalModOptions"), &["Brita"]),
    ];
    let c = find_dependency_cycles(&v);
    assert_eq!(c.len(), 1, "suffix-matched requirement should close the loop");
}

#[test]
fn cycle_resolution_agrees_with_the_missing_dependency_detector() {
    // The two must never disagree: a requirement that resolves to an installed
    // mod cannot simultaneously be reported as a missing dependency AND used as
    // a cycle edge. Both now delegate to the same resolver.
    let v = vec![
        with(m("Brita"), &["modoptions"]),
        m("B42_GlobalModOptions"),
    ];
    assert!(
        detect_missing_dependencies(&v).is_empty(),
        "modoptions must resolve to the installed B42_GlobalModOptions"
    );
    assert!(
        find_dependency_cycles(&v).is_empty(),
        "no cycle here, and neither detector should invent one"
    );
}

#[test]
fn cycle_still_detected_when_the_requirement_uses_a_longer_installed_name() {
    let v = vec![
        with(m("A"), &["B_Library_v2"]),
        with(m("B_Library_v2"), &["A"]),
    ];
    assert_eq!(find_dependency_cycles(&v).len(), 1);
}

#[test]
fn cycle_only_returns_the_cycle_not_acyclic_parts() {
    let v = vec![
        with(m("A"), &["B"]),
        with(m("B"), &["A"]),
        m("Lonely"),
        with(m("L2"), &["L3"]),
        m("L3"),
    ];
    let c = find_dependency_cycles(&v);
    assert_eq!(c.len(), 1);
    assert!(!c[0].contains(&"lonely".to_string()));
    assert!(!c[0].contains(&"l2".to_string()));
}

#[test]
fn cycle_disconnected_graphs() {
    let v = vec![
        with(m("A"), &["B"]),
        with(m("B"), &["A"]),
        with(m("X"), &["Y"]),
        with(m("Y"), &["X"]),
    ];
    assert_eq!(find_dependency_cycles(&v).len(), 2);
}

#[test]
fn cycle_empty_list() {
    assert!(find_dependency_cycles(&[]).is_empty());
}

#[test]
fn cycle_is_order_independent() {
    let base = vec![
        with(m("A"), &["B"]),
        with(m("B"), &["C"]),
        with(m("C"), &["A"]),
        m("D"),
    ];
    let mut reversed = base.clone();
    reversed.reverse();
    assert_eq!(find_dependency_cycles(&base), find_dependency_cycles(&reversed));
}

#[test]
fn cycle_uses_load_mod_after_edges() {
    let mut a = m("A");
    a.load_mod_after = vec!["B".into()];
    let mut b = m("B");
    b.load_mod_after = vec!["A".into()];
    assert_eq!(find_dependency_cycles(&[a, b]).len(), 1);
}

#[test]
fn cycles_to_diagnostics_severity_and_ids() {
    let d = cycles_to_diagnostics(&[vec!["a".to_string(), "b".to_string()]]);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].severity, Severity::Error);
    assert_eq!(d[0].kind, ConflictKind::CircularDependency);
    assert_eq!(d[0].mod_ids.len(), 2);
}

#[test]
fn cycles_to_diagnostics_skips_empty_cycles() {
    assert!(cycles_to_diagnostics(&[vec![]]).is_empty());
}

// ---------- detect_malformed_version_directives ----------

#[test]
fn malformed_bare_integer_is_flagged() {
    let mut a = m("A");
    a.pzversion = Some("42".into());
    let d = detect_malformed_version_directives(&a);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].kind, ConflictKind::MalformedVersionDirective);
}

#[test]
fn malformed_decimal_form_is_clean() {
    let mut a = m("A");
    a.pzversion = Some("42.00".into());
    assert!(detect_malformed_version_directives(&a).is_empty());
}

#[test]
fn malformed_multi_dot_version_is_clean() {
    let mut a = m("A");
    a.pzversion = Some("42.15".into());
    assert!(detect_malformed_version_directives(&a).is_empty());
}

#[test]
fn malformed_absent_pzversion_is_clean() {
    assert!(detect_malformed_version_directives(&m("A")).is_empty());
}

#[test]
fn malformed_single_dot_zero_is_clean() {
    let mut a = m("A");
    a.pzversion = Some("42.0".into());
    assert!(detect_malformed_version_directives(&a).is_empty());
}

#[test]
fn malformed_suggests_the_decimal_fix() {
    let mut a = m("A");
    a.pzversion = Some("42".into());
    let d = detect_malformed_version_directives(&a);
    let sugg = d[0].suggestion.as_deref().unwrap_or("");
    assert!(sugg.contains("42.0"), "suggestion was: {}", sugg);
}

// ---------- analyze ----------

#[test]
fn analyze_empty_slice_does_not_panic() {
    assert!(analyze(&[]).is_empty());
}

#[test]
fn analyze_collects_multiple_detectors() {
    let mut bad = with(m("Lib"), &["Ghost"]);
    bad.pzversion = Some("42".into());
    let v = vec![bad, inc(m("A"), &["Lib"]), with(m("Lib"), &["A"])];
    let d = analyze(&v);
    assert!(d.len() >= 3, "expected several diagnostics, got {}", d.len());
}

#[test]
fn analyze_has_no_duplicate_kind_and_mod_pairs() {
    let v = vec![m("A"), m("A"), inc(m("B"), &["A"])];
    let mut seen: Vec<(ConflictKind, String)> = Vec::new();
    for d in analyze(&v) {
        let key = (d.kind, d.mod_ids.join("|"));
        assert!(!seen.contains(&key), "duplicated diagnostic: {:?}", key);
        seen.push(key);
    }
}

#[test]
fn severity_orders_error_before_warning() {
    assert!(Severity::Error < Severity::Warning);
    assert!(Severity::Warning < Severity::Info);
    let mut v = vec![Severity::Info, Severity::Error, Severity::Warning];
    v.sort();
    assert_eq!(v, vec![Severity::Error, Severity::Warning, Severity::Info]);
}

#[test]
fn normalize_id_matches_app_convention() {
    assert_eq!(normalize_id("My-Mod"), "my_mod");
    assert_eq!(normalize_id("  Spaced  "), "spaced");
}

/// The app-wide convention (`vfs::scan_conflicts`, `topological_sort`) turns dashes
/// into underscores but *deletes* spaces, so "My-Mod" and "My Mod" do NOT normalize
/// to the same value. Detection has to agree with the existing load-order output, so
/// this asymmetry is preserved deliberately rather than "fixed" in one place only.
#[test]
fn normalize_id_space_and_dash_asymmetry_is_preserved() {
    assert_eq!(normalize_id("My-Mod"), "my_mod");
    assert_eq!(normalize_id("My Mod"), "mymod");
}
// ===========================================================================
// BUG 1 — dependency resolution ignored Steam Workshop ids and the inferred
// requirements fabricated by apply_known_dependency_heuristics looked like
// author mistakes.
// ===========================================================================

/// A manifest with an explicit workshop id.
fn with_workshop(mut mm: ModManifest, workshop_id: &str) -> ModManifest {
    mm.workshop_id = Some(workshop_id.to_string());
    mm
}

/// A manifest whose requirement is marked as inferred by PZ Mod Studio.
fn with_inferred(mut mm: ModManifest, require: &[&str], inferred: &[&str]) -> ModManifest {
    mm.require = require.iter().map(|s| s.to_string()).collect();
    mm.inferred_require = inferred.iter().map(|s| s.to_string()).collect();
    mm
}

fn missing_for<'a>(diags: &'a [ModDiagnostic], mod_id: &str) -> Vec<&'a ModDiagnostic> {
    diags
        .iter()
        .filter(|d| d.kind == ConflictKind::MissingDependency && d.mod_ids.contains(&mod_id.to_string()))
        .collect()
}

// ---------- the exact cases from the bug report ----------

#[test]
fn injected_modoptions_resolves_against_b42_globalmodoptions() {
    // The headline false positive: `require=modoptions` (injected) against an
    // installed `B42_GlobalModOptions`. `starts_with("modoptions")` is false, so
    // this used to be reported as a MISSING dependency for an installed mod.
    let mods = vec![
        with_inferred(m("Brita"), &["modoptions"], &["modoptions"]),
        m("B42_GlobalModOptions"),
    ];
    let diags = detect_missing_dependencies(&mods);
    assert!(
        missing_for(&diags, "Brita").is_empty(),
        "an installed B42_GlobalModOptions must satisfy require=modoptions, got {:?}",
        missing_for(&diags, "Brita")
    );
}

#[test]
fn injected_tsarslib_resolves_against_tsar_tsarslib() {
    let mods = vec![
        with_inferred(m("Autotsar_Trailers"), &["Tsarslib"], &["Tsarslib"]),
        m("Tsar_Tsarslib"),
    ];
    let diags = detect_missing_dependencies(&mods);
    assert!(
        missing_for(&diags, "Autotsar_Trailers").is_empty(),
        "suffix matching must resolve Tsarslib, got {:?}",
        missing_for(&diags, "Autotsar_Trailers")
    );
}

#[test]
fn numeric_requirement_resolves_against_a_workshop_id() {
    // The normal form in real mod.info: a bare numeric workshop id, while the
    // installed mod's `id` is a folder name that has nothing to do with it.
    let mods = vec![
        with(m("SomeModFolderName"), &["2200148440"]),
        with_workshop(m("Brita"), "2200148440"),
    ];
    let diags = detect_missing_dependencies(&mods);
    assert!(
        missing_for(&diags, "SomeModFolderName").is_empty(),
        "require=2200148440 must resolve via workshop_id, got {:?}",
        missing_for(&diags, "SomeModFolderName")
    );
}

#[test]
fn numeric_requirement_resolves_against_a_numeric_folder_id() {
    // Some installs have no usable `id=`, so the folder is named after the
    // workshop id. The resolver must treat a numeric `id` as a workshop id.
    let mods = vec![
        with(m("Consumer"), &["2200148440"]),
        m("2200148440"),
    ];
    assert!(
        missing_for(&detect_missing_dependencies(&mods), "Consumer").is_empty(),
        "a numeric folder id must satisfy a numeric requirement"
    );
}

#[test]
fn numeric_requirement_with_leading_zeros_resolves() {
    let mods = vec![
        with(m("Consumer"), &["02200148440"]),
        with_workshop(m("Brita"), "2200148440"),
    ];
    assert!(
        missing_for(&detect_missing_dependencies(&mods), "Consumer").is_empty(),
        "leading zeros must not defeat a numeric match"
    );
}

#[test]
fn numeric_requirement_ignores_surrounding_whitespace() {
    let mods = vec![
        with(m("Consumer"), &["  2200148440  "]),
        with_workshop(m("Brita"), "2200148440"),
    ];
    assert!(missing_for(&detect_missing_dependencies(&mods), "Consumer").is_empty());
}

#[test]
fn numeric_requirement_does_not_substring_match_a_longer_workshop_id() {
    // Guard against the digit-prefix trap: 2200148440 must NOT bind to
    // 22001484401 just because one starts with the other.
    let mods = vec![
        with(m("Consumer"), &["2200148440"]),
        with_workshop(m("Decoy"), "22001484401"),
    ];
    assert_eq!(
        missing_for(&detect_missing_dependencies(&mods), "Consumer").len(),
        1,
        "a longer workshop id must not satisfy a shorter numeric requirement"
    );
}

#[test]
fn numeric_requirement_against_an_absent_mod_is_still_reported() {
    let mods = vec![with(m("Consumer"), &["2200148440"])];
    assert_eq!(
        missing_for(&detect_missing_dependencies(&mods), "Consumer").len(),
        1
    );
}

// ---------- resolution precedence ----------

#[test]
fn exact_id_beats_workshop_id_beats_prefix_beats_suffix_beats_substring() {
    use crate::load_order::mod_info::{match_strength, MatchStrength};

    let exact = m("Arsenal(26)GunFighter");
    assert_eq!(match_strength("arsenal(26)gunfighter", &exact), Some(MatchStrength::ExactId));

    let workshop = with_workshop(m("Folder_Name"), "2200148440");
    assert_eq!(match_strength("2200148440", &workshop), Some(MatchStrength::WorkshopId));

    let prefix = m("Arsenal_GunFighter");
    assert_eq!(match_strength("Arsenal", &prefix), Some(MatchStrength::IdPrefix));

    let suffix = m("B42_GlobalModOptions");
    assert_eq!(match_strength("modoptions", &suffix), Some(MatchStrength::IdSuffix));

    let substring = m("MyLibraryModOptionsExtra");
    assert_eq!(
        match_strength("librarymod", &substring),
        Some(MatchStrength::IdSubstring)
    );

    // Stronger tiers must rank numerically higher (Ord is "best first").
    assert!(MatchStrength::ExactId < MatchStrength::WorkshopId);
    assert!(MatchStrength::WorkshopId < MatchStrength::IdPrefix);
    assert!(MatchStrength::IdPrefix < MatchStrength::IdSuffix);
    assert!(MatchStrength::IdSuffix < MatchStrength::IdSubstring);
}

#[test]
fn an_exact_id_match_beats_a_stronger_looking_suffix_on_another_mod() {
    use crate::load_order::mod_info::resolve_requirement;
    let mods = vec![
        m("GunFighter"),                       // ExactId for "gunfighter"
        m("Arsenal_GunFighter_Extra"),         // IdPrefix for "gunfighter"
    ];
    let hit = resolve_requirement("GunFighter", &mods, None).expect("must resolve");
    assert_eq!(hit.id, "GunFighter", "exact id must win over a prefix match");
}

#[test]
fn a_workshop_id_match_beats_a_name_match_on_another_mod() {
    use crate::load_order::mod_info::resolve_requirement;
    let mods = vec![
        with_workshop(m("Decoy"), "2200148440"),  // WorkshopId
        m("2200148440_ish"),                       // would be a name match
    ];
    let hit = resolve_requirement("2200148440", &mods, None).expect("must resolve");
    assert_eq!(hit.id, "Decoy");
}

#[test]
fn short_requirements_never_substring_match() {
    use crate::load_order::mod_info::{match_strength, MIN_SUBSTRING_MATCH_LEN};
    assert_eq!(MIN_SUBSTRING_MATCH_LEN, 4);
    // `ui` is shorter than the guard, so it must not bind to a mod whose id
    // merely contains those letters.
    assert_eq!(match_strength("ui", &m("Building_Decor_Items")), None);
}

// ---------- ambiguity and determinism ----------

#[test]
fn ambiguity_resolves_to_one_candidate_deterministically() {
    use crate::load_order::mod_info::resolve_requirement;
    let mods = vec![
        m("Zeta_Library"),
        m("Alpha_Library"),
        m("Mid_Library"),
    ];
    let first = resolve_requirement("Library", &mods, None).expect("must resolve").id.clone();
    // Repeated calls on the same slice must agree, always.
    for _ in 0..25 {
        assert_eq!(
            resolve_requirement("Library", &mods, None).expect("must resolve").id,
            first,
            "the winner must not vary between calls"
        );
    }
    // And the rule is documented: smallest normalized id wins a tie.
    assert_eq!(first, "Alpha_Library");
}

#[test]
fn ambiguity_is_stable_under_input_reordering() {
    use crate::load_order::mod_info::resolve_requirement;
    let a = m("Zeta_Library");
    let b = m("Alpha_Library");
    let c = m("Mid_Library");

    let forward = resolve_requirement("Library", &[a.clone(), b.clone(), c.clone()], None)
        .expect("must resolve")
        .id
        .clone();
    let reversed = resolve_requirement("Library", &[c, b, a], None)
        .expect("must resolve")
        .id
        .clone();
    assert_eq!(
        forward, reversed,
        "the same mods in a different order must resolve identically"
    );
}

#[test]
fn an_enabled_candidate_beats_a_disabled_one_when_both_match() {
    use crate::load_order::mod_info::resolve_requirement;
    let mods = vec![
        disabled(m("Alpha_Library")), // smaller id, but OFF
        m("Zeta_Library"),            // larger id, but ON
    ];
    let hit = resolve_requirement("Library", &mods, None).expect("must resolve");
    assert_eq!(hit.id, "Zeta_Library", "an enabled candidate must win");
}

#[test]
fn a_stronger_match_wins_even_when_the_other_candidate_is_enabled() {
    use crate::load_order::mod_info::resolve_requirement;
    let mods = vec![
        m("Library"),                        // ExactId, DISABLED
        disabled(m("Extra_Library_Stuff")),   // IdSuffix, enabled
    ];
    let hit = resolve_requirement("Library", &mods, None).expect("must resolve");
    assert_eq!(
        hit.id, "Library",
        "match strength must outrank the enabled preference"
    );
}

// ---------- exclusion ----------

#[test]
fn self_requirement_is_excluded_by_the_resolver() {
    use crate::load_order::mod_info::resolve_requirement;
    let mods = vec![m("Solo"), m("Solo_Library")];
    // With no exclusion the best match is the exact id.
    assert_eq!(
        resolve_requirement("Solo", &mods, None).expect("must resolve").id,
        "Solo"
    );
    // Excluding it (index 0) must fall through to the other candidate.
    assert_eq!(
        resolve_requirement("Solo", &mods, Some(0)).expect("must resolve").id,
        "Solo_Library"
    );
    // With nothing else to match, it resolves to nothing at all.
    assert!(resolve_requirement("Solo", &[m("Solo")], Some(0)).is_none());
}

#[test]
fn a_self_referencing_require_is_never_reported_as_missing() {
    // `detect_missing_dependencies` also has its own guard; both must hold.
    let mods = vec![with(m("Loop"), &["Loop"])];
    assert!(missing_for(&detect_missing_dependencies(&mods), "Loop").is_empty());
}

#[test]
fn resolution_of_an_empty_requirement_is_none() {
    use crate::load_order::mod_info::resolve_requirement;
    let mods = vec![m("Anything")];
    for req in ["", "   ", "\t"] {
        assert!(resolve_requirement(req, &mods, None).is_none());
    }
}

#[test]
fn resolution_of_an_empty_manifest_list_is_none() {
    use crate::load_order::mod_info::resolve_requirement;
    assert!(resolve_requirement("Anything", &[], None).is_none());
    assert!(resolve_requirement("Anything", &[], Some(0)).is_none());
}

// ---------- severity and attribution ----------

#[test]
fn an_unresolvable_author_declared_requirement_is_still_an_error() {
    let mods = vec![with(m("Needy"), &["TotallyAbsentLibrary"])];
    let diags = detect_missing_dependencies(&mods);
    let hits = missing_for(&diags, "Needy");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].severity, Severity::Error);
    assert!(
        !hits[0].detail.as_deref().unwrap_or("").contains("inferred"),
        "an author-declared requirement must not be attributed to us: {:?}",
        hits[0].detail
    );
}

#[test]
fn an_unresolvable_inferred_requirement_is_a_warning_not_an_error() {
    let mods = vec![with_inferred(
        m("Brita"),
        &["SomethingNobodyInstalled"],
        &["SomethingNobodyInstalled"],
    )];
    let diags = detect_missing_dependencies(&mods);
    let hits = missing_for(&diags, "Brita");
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].severity,
        Severity::Warning,
        "we guessed this name; we do not get to call it an error"
    );
    assert!(
        hits[0].detail.as_deref().unwrap_or("").contains("inferred"),
        "the detail must attribute the inference: {:?}",
        hits[0].detail
    );
    assert!(
        hits[0].suggestion.as_deref().unwrap_or("").contains("may be safe to ignore"),
        "the suggestion must not send the user on a wild goose chase: {:?}",
        hits[0].suggestion
    );
}

#[test]
fn an_inferred_requirement_that_does_resolve_is_not_reported_at_all() {
    let mods = vec![
        with_inferred(m("Brita"), &["modoptions"], &["modoptions"]),
        m("B42_GlobalModOptions"),
    ];
    assert!(missing_for(&detect_missing_dependencies(&mods), "Brita").is_empty());
}

#[test]
fn a_requirement_the_author_also_declared_is_attributed_to_the_author() {
    // If the mod.info already required it, `inject_inferred_require` must not
    // mark it inferred — the author said so.
    let mods = vec![with_inferred(
        m("Brita"),
        &["Arsenal(26)GunFighter[MAIN MOD 2.0]"],
        &[],
    )];
    let diags = detect_missing_dependencies(&mods);
    for d in &diags {
        assert!(
            !d.detail.as_deref().unwrap_or("").contains("inferred"),
            "an author-declared require must stay author-declared: {:?}",
            d.detail
        );
    }
}

// ---------- the real heuristics, end to end ----------

#[test]
fn the_real_heuristic_marks_what_it_injects() {
    let mut mm = m("Brita");
    crate::load_order::mod_info::apply_known_dependency_heuristics(&mut mm);
    assert!(
        mm.inferred_require.iter().any(|r| r == "modoptions"),
        "Brita must record its injected modoptions requirement, got {:?}",
        mm.inferred_require
    );
    assert!(
        mm.require.iter().any(|r| r == "modoptions"),
        "the injected requirement must still be in `require` for the sorter"
    );
}

#[test]
fn the_real_heuristic_is_idempotent() {
    let mut mm = m("Brita");
    crate::load_order::mod_info::apply_known_dependency_heuristics(&mut mm);
    let after_first = mm.require.clone();
    let inferred_first = mm.inferred_require.clone();
    crate::load_order::mod_info::apply_known_dependency_heuristics(&mut mm);
    assert_eq!(mm.require, after_first, "re-applying must not duplicate requires");
    assert_eq!(mm.inferred_require, inferred_first);
}

#[test]
fn the_real_brita_heuristic_no_longer_produces_a_false_missing_dependency() {
    // The user-reported symptom, reproduced with the genuine heuristic input.
    let mut brita = with_workshop(m("Brita"), "2200148440");
    crate::load_order::mod_info::apply_known_dependency_heuristics(&mut brita);
    let mods = vec![brita, m("B42_GlobalModOptions")];
    let diags = detect_missing_dependencies(&mods);

    // Brita's heuristic injects two requirements. Only the modoptions one is
    // installed here; the GunFighter one is genuinely absent, so it is still
    // reported (as a Warning, correctly attributed). The assertion is
    // specifically that `modoptions` is no longer flagged.
    let modoptions_reported = diags.iter().any(|d| {
        d.kind == ConflictKind::MissingDependency
            && d.detail.as_deref().unwrap_or("").contains("modoptions")
    });
    assert!(
        !modoptions_reported,
        "the injected modoptions requirement must resolve against \
         B42_GlobalModOptions, got {:?}",
        diags
    );
}

#[test]
fn an_absent_inferred_requirement_is_reported_as_an_attributed_warning() {
    // The other half of the Brita fixture: GunFighter really is not installed.
    let mut brita = with_workshop(m("Brita"), "2200148440");
    crate::load_order::mod_info::apply_known_dependency_heuristics(&mut brita);
    let diags = detect_missing_dependencies(&vec![brita]);
    let gunfighter: Vec<_> = diags
        .iter()
        .filter(|d| d.detail.as_deref().unwrap_or("").contains("GunFighter"))
        .collect();
    assert_eq!(gunfighter.len(), 1);
    assert_eq!(
        gunfighter[0].severity,
        Severity::Warning,
        "an absent inferred requirement must not be an error"
    );
    assert!(gunfighter[0].detail.as_deref().unwrap_or("").contains("inferred"));
}

#[test]
fn the_junk_word_filter_does_not_eat_a_numeric_requirement() {
    use crate::load_order::mod_info::is_numeric_workshop_id;
    assert!(is_numeric_workshop_id("2200148440"));
    assert!(is_numeric_workshop_id("  42 "));
    assert!(!is_numeric_workshop_id("modoptions"));
    assert!(!is_numeric_workshop_id(""));
    assert!(!is_numeric_workshop_id("2200148440x"));

    // And end to end: parse a real mod.info and check the number survived.
    let dir = std::env::temp_dir().join(format!(
        "pzms_junk_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("must create the fixture dir");
    let info = dir.join("mod.info");
    std::fs::write(
        &info,
        "id=Numeric\nrequire=2200148440,Please Update To B42 Version Of The Game,visit steam\n",
    )
    .expect("must write the fixture");
    let parsed = crate::load_order::mod_info::parse_mod_info(&info).expect("must parse");
    assert!(
        parsed.require.iter().any(|r| r == "2200148440"),
        "the numeric workshop id must survive the junk filter, got {:?}",
        parsed.require
    );
    assert!(
        !parsed.require.iter().any(|r| r.eq_ignore_ascii_case("please")),
        "prose must still be filtered"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
