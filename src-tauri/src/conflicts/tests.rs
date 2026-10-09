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
    }
}

fn with(mut m: ModManifest, require: &[&str]) -> ModManifest {
    m.require = require.iter().map(|s| s.to_string()).collect();
    m
}

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