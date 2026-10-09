//! Tests for the load-order file reader/writer.
//!
//! The important one here is `write_mod_list_ini_reports_a_real_failure`: the
//! function used to swallow every `fs::write` error behind `let _ =` and return
//! `Ok(())`, so a completely failed save looked like a successful one.

use super::*;

struct IniSandbox {
    root: PathBuf,
}

impl IniSandbox {
    fn new(tag: &str) -> IniSandbox {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("pzms_ini_{}_{}", tag, nanos));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("mods")).expect("must create the sandbox");
        IniSandbox { root }
    }

    /// The path the function is called with: `<root>/mods/ModListData.ini`.
    fn ini_path(&self) -> String {
        self.root
            .join("mods")
            .join("ModListData.ini")
            .to_string_lossy()
            .to_string()
    }
}

impl Drop for IniSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

// ---------------------------------------------------------------------------
// P3 defect 1: errors must propagate
// ---------------------------------------------------------------------------

#[test]
fn write_mod_list_ini_reports_success_on_a_writable_tree() {
    let sb = IniSandbox::new("ok");
    let mods: Vec<String> = vec!["Alpha".to_string(), "Beta".to_string()];
    assert_eq!(
        write_mod_list_ini(&sb.ini_path(), &mods),
        Ok(()),
        "a writable tree must report success"
    );
}

#[test]
fn write_mod_list_ini_reports_a_real_failure_instead_of_faking_success() {
    let sb = IniSandbox::new("blocked");
    // A directory where a file must go: every write to it fails.
    fs::create_dir_all(sb.root.join("Lua").join("ModListData.ini"))
        .expect("must create the blocking directory");
    fs::create_dir_all(sb.root.join("mods.txt")).expect("must create the blocking directory");

    let mods: Vec<String> = vec!["Alpha".to_string()];
    match write_mod_list_ini(&sb.ini_path(), &mods) {
        Ok(()) => panic!(
            "write_mod_list_ini reported success even though its targets were \
             unwritable — this is exactly the silent-failure defect"
        ),
        Err(msg) => {
            assert!(
                msg.contains("could not write") || msg.contains("could not create"),
                "the error must say what failed, got: {}",
                msg
            );
            assert!(
                msg.contains("ModListData.ini") || msg.contains("mods.txt"),
                "the error must name a failing path, got: {}",
                msg
            );
        }
    }
}

#[test]
fn write_mod_list_ini_attempts_every_write_even_when_some_fail() {
    let sb = IniSandbox::new("partial");
    // Block only `Lua/loadorder.ini`; everything else must still be written.
    fs::create_dir_all(sb.root.join("Lua").join("loadorder.ini"))
        .expect("must create the blocking directory");

    let mods: Vec<String> = vec!["Alpha".to_string(), "Beta".to_string()];
    let result = write_mod_list_ini(&sb.ini_path(), &mods);

    assert!(result.is_err(), "the blocked file must be reported");
    // A single bad target must not stop the rest.
    assert!(
        sb.root.join("mods").join("default.txt").is_file(),
        "mods/default.txt must still have been written"
    );
    assert!(
        sb.root.join("mods.txt").is_file(),
        "mods.txt must still exist"
    );
    assert!(
        sb.root.join("Lua").join("ModListData.ini").is_file(),
        "an unblocked Lua file must still have been written"
    );
}

#[test]
fn write_mod_list_ini_error_names_every_failing_path() {
    let sb = IniSandbox::new("multi");
    fs::create_dir_all(sb.root.join("mods.txt")).expect("must block mods.txt");
    fs::create_dir_all(sb.root.join("Lua").join("ModListData.ini"))
        .expect("must block the ini");

    let mods: Vec<String> = vec!["Alpha".to_string()];
    let msg = write_mod_list_ini(&sb.ini_path(), &mods)
        .expect_err("two blocked files must be reported");
    assert!(
        msg.contains("mods.txt") && msg.contains("ModListData.ini"),
        "every failure must be named, got: {}",
        msg
    );
}

#[test]
fn write_mod_list_ini_output_round_trips_through_the_reader() {
    let sb = IniSandbox::new("round_trip");
    let mods: Vec<String> = vec!["Alpha".to_string(), "Beta Mod".to_string()];
    write_mod_list_ini(&sb.ini_path(), &mods).expect("write must succeed");

    let read_back = read_mod_list_ini(&sb.ini_path()).expect("read must succeed");
    assert_eq!(
        read_back.active_mods.len(),
        2,
        "both ids must survive the round trip, got {:?}",
        read_back.active_mods
    );
}

#[test]
fn write_mod_list_ini_with_an_empty_list_still_succeeds() {
    let sb = IniSandbox::new("empty");
    assert_eq!(write_mod_list_ini(&sb.ini_path(), &[]), Ok(()));
}

// ---------------------------------------------------------------------------
// renderers vs parsers
// ---------------------------------------------------------------------------

#[test]
fn renderers_agree_with_their_parsers() {
    let mods = vec!["Alpha".to_string(), "Beta".to_string()];

    let plain = render_plain_list(&mods);
    assert_eq!(parse_plain_list_text(&plain), mods);

    let default_txt = render_default_txt(&mods);
    assert_eq!(parse_default_txt_text(&default_txt), mods);

    let ini = render_ini("ModList", "activeMods", &mods);
    assert_eq!(parse_ini_text(&ini), mods);
}

#[test]
fn renderers_tolerate_an_empty_list() {
    let empty: Vec<String> = Vec::new();
    assert_eq!(parse_plain_list_text(&render_plain_list(&empty)).len(), 0);
    assert_eq!(parse_default_txt_text(&render_default_txt(&empty)).len(), 0);
    assert_eq!(
        parse_ini_text(&render_ini("ModList", "activeMods", &empty)).len(),
        0
    );
}

#[test]
fn renderers_drop_blank_ids() {
    let mods = vec!["Alpha".to_string(), "   ".to_string(), "Beta".to_string()];
    assert_eq!(parse_plain_list_text(&render_plain_list(&mods)), vec!["Alpha", "Beta"]);
    assert_eq!(parse_default_txt_text(&render_default_txt(&mods)), vec!["Alpha", "Beta"]);
    assert_eq!(
        parse_ini_text(&render_ini("ModList", "activeMods", &mods)),
        vec!["Alpha", "Beta"]
    );
}

#[test]
fn render_default_txt_keeps_the_native_lua_shape() {
    let out = render_default_txt(&["Alpha".to_string()]);
    assert!(out.starts_with("VERSION = 1,"), "got: {}", out);
    assert!(out.contains("mods\n{"), "got: {}", out);
    assert!(out.contains("    mod = Alpha,"), "got: {}", out);
    assert!(out.ends_with("}\n\nmaps\n{\n}\n"), "got: {}", out);
}

#[test]
fn parsers_of_garbage_do_not_panic() {
    for junk in [
        "",
        "\u{0}\u{1}\u{2}",
        "mod = ",
        "mods",
        "mods { }",
        "[ModList]\nactiveMods=",
        &"mod = ".repeat(500),
    ] {
        let _ = parse_plain_list_text(junk);
        let _ = parse_default_txt_text(junk);
        let _ = parse_ini_text(junk);
    }
}

#[test]
fn load_order_format_is_recognised_by_file_name() {
    assert_eq!(
        LoadOrderFormat::from_file_name("mods.txt"),
        Some(LoadOrderFormat::PlainList)
    );
    assert_eq!(
        LoadOrderFormat::from_file_name("default.txt"),
        Some(LoadOrderFormat::DefaultTxt)
    );
    assert_eq!(
        LoadOrderFormat::from_file_name("ModListData.ini"),
        Some(LoadOrderFormat::Ini)
    );
    assert_eq!(LoadOrderFormat::from_file_name("whatever.txt"), None);
    assert_eq!(LoadOrderFormat::from_file_name(""), None);
}

#[test]
fn reading_a_missing_file_yields_an_empty_list_not_an_error() {
    let missing = std::env::temp_dir().join("pzms_ini_absent_zzz");
    let _ = fs::remove_dir_all(&missing);
    let path = missing.join("mods").join("ModListData.ini");
    let data = read_mod_list_ini(&path.to_string_lossy()).expect("read must not error");
    assert!(data.active_mods.is_empty());
    assert!(!missing.exists(), "reading must not create anything");
}