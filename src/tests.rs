use super::*;

/// `set_var` is `unsafe` in edition 2024. Every test uses its own unique
/// variable names, so nothing here races with another test.
fn set(key: &str, value: &str) {
    // SAFETY: unique names per test; no other code reads these variables
    // through libc concurrently.
    unsafe { env::set_var(key, value) }
}

#[test]
fn parses_defaults_and_optional() {
    set("TYPEDENV_T1_PORT", " 8081 ");
    assert_eq!(var::<u16>("TYPEDENV_T1_PORT").unwrap(), 8081);
    assert_eq!(var_or("TYPEDENV_T1_MISSING", 8080u16).unwrap(), 8080);
    assert_eq!(var_opt::<u16>("TYPEDENV_T1_MISSING").unwrap(), None);
}

#[test]
fn missing_required_is_error() {
    let e = var::<u16>("TYPEDENV_T2_MISSING").unwrap_err();
    assert_eq!(
        e,
        EnvError::Missing {
            name: "TYPEDENV_T2_MISSING".into(),
        }
    );
}

#[test]
fn invalid_value_is_not_swallowed_by_default() {
    set("TYPEDENV_T3_BAD", "abc");
    let e = var_or("TYPEDENV_T3_BAD", 1u16).unwrap_err();
    assert!(matches!(e, EnvError::Invalid { .. }));
    assert_eq!(e.name(), Some("TYPEDENV_T3_BAD"));
    assert!(e.to_string().contains("`abc`"));
}

#[test]
fn bool_forms() {
    for (raw, want) in [
        ("1", true),
        ("YES", true),
        ("on", true),
        ("0", false),
        ("No", false),
    ] {
        set("TYPEDENV_T4_BOOL", raw);
        assert_eq!(var_bool("TYPEDENV_T4_BOOL").unwrap(), want, "{raw}");
    }
    set("TYPEDENV_T4_BOOL", "maybe");
    assert!(var_bool("TYPEDENV_T4_BOOL").is_err());
    assert!(var_bool_or("TYPEDENV_T4_UNSET", true).unwrap());
}

#[test]
fn loader_collects_all_errors() {
    set("TYPEDENV_T5_PORT", "abc");
    set("TYPEDENV_T5_DEBUG", "maybe");

    let mut cfg = Loader::new();
    let _port: u16 = cfg.var_or("TYPEDENV_T5_PORT", 8080);
    let _debug = cfg.var_bool_or("TYPEDENV_T5_DEBUG", false);
    let _db: String = cfg.var("TYPEDENV_T5_DB");
    let _ok: u16 = cfg.var_or("TYPEDENV_T5_FINE", 1);

    let errs = cfg.finish().unwrap_err();
    assert_eq!(errs.len(), 3);
    let text = errs.to_string();
    assert!(text.starts_with("3 configuration errors:"));
    assert!(text.contains("TYPEDENV_T5_DB"));
}

#[test]
fn loader_ok_when_everything_valid() {
    set("TYPEDENV_T6_PORT", "9000");
    let mut cfg = Loader::new();
    let port: u16 = cfg.var("TYPEDENV_T6_PORT");
    assert_eq!(port, 9000);
    assert!(cfg.finish().is_ok());
}

#[test]
fn list_parsing() {
    set("TYPEDENV_T7_HOSTS", " a, b ,, c, ");
    assert_eq!(
        var_list::<String>("TYPEDENV_T7_HOSTS", ',').unwrap(),
        ["a", "b", "c"]
    );
    set("TYPEDENV_T7_EMPTY", "");
    assert!(
        var_list::<u16>("TYPEDENV_T7_EMPTY", ',')
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        var_list_or("TYPEDENV_T7_UNSET", ',', vec![1u16]).unwrap(),
        [1]
    );
    assert!(matches!(
        var_list::<u16>("TYPEDENV_T7_UNSET2", ','),
        Err(EnvError::Missing { .. })
    ));
}

#[test]
fn list_error_names_the_item() {
    set("TYPEDENV_T8_PORTS", "80,http,443");
    let e = var_list::<u16>("TYPEDENV_T8_PORTS", ',').unwrap_err();
    let msg = e.to_string();
    assert!(msg.contains("TYPEDENV_T8_PORTS"), "{msg}");
    assert!(msg.contains("item 2 (`http`)"), "{msg}");
}

#[test]
fn loader_lists() {
    set("TYPEDENV_T9_TAGS", "x,y");
    let mut cfg = Loader::new();
    let tags: Vec<String> = cfg.var_list("TYPEDENV_T9_TAGS", ',');
    let dflt: Vec<u16> = cfg.var_list_or("TYPEDENV_T9_UNSET", ',', vec![7]);
    assert_eq!(tags, ["x", "y"]);
    assert_eq!(dflt, [7]);
    assert!(cfg.finish().is_ok());
}

#[cfg(feature = "dotenv")]
#[test]
fn dotenv_parser_syntax() {
    let src = "\
# comment
A=1
export B = two
C=\"hello world\" # trailing
D='raw $x #notcomment'
E=val # comment
F=\"line\\nbreak\"
G=
H=# only comment
";
    let (vars, errs) = parse_dotenv(src);
    assert!(errs.is_empty(), "{errs:?}");
    assert_eq!(vars["A"], "1");
    assert_eq!(vars["B"], "two");
    assert_eq!(vars["C"], "hello world");
    assert_eq!(vars["D"], "raw $x #notcomment");
    assert_eq!(vars["E"], "val");
    assert_eq!(vars["F"], "line\nbreak");
    assert_eq!(vars["G"], "");
    assert_eq!(vars["H"], "");
}

#[cfg(feature = "dotenv")]
#[test]
fn dotenv_parser_reports_line_errors() {
    let (vars, errs) = parse_dotenv("OK=1\nBAD\nQ=\"abc\n=nokey\nZ=\"x\" junk\n");
    assert_eq!(vars.len(), 1);
    let lines: Vec<usize> = errs.iter().map(|(n, _)| *n).collect();
    assert_eq!(lines, [2, 3, 4, 5]);
}

#[cfg(feature = "dotenv")]
#[test]
fn dotenv_file_loading_and_precedence() {
    let path = env::temp_dir().join(format!("typedenv-test-{}.env", std::process::id()));
    std::fs::write(
        &path,
        "TYPEDENV_T10_PORT=7000\nTYPEDENV_T10_FROM_ENV=file\nTYPEDENV_T10_DB=postgres://x\n",
    )
    .unwrap();
    set("TYPEDENV_T10_FROM_ENV", "env");

    let mut cfg = Loader::new().with_dotenv(&path);
    let port: u16 = cfg.var("TYPEDENV_T10_PORT");
    let db: String = cfg.var("TYPEDENV_T10_DB");
    let which: String = cfg.var("TYPEDENV_T10_FROM_ENV");
    let _ = std::fs::remove_file(&path);

    assert_eq!(port, 7000);
    assert_eq!(db, "postgres://x");
    assert_eq!(which, "env"); // real env wins over the file
    assert!(cfg.finish().is_ok());
    // the process environment was not modified
    assert!(env::var("TYPEDENV_T10_PORT").is_err());
}

#[cfg(feature = "dotenv")]
#[test]
fn dotenv_missing_file_is_ignored_but_syntax_errors_are_reported() {
    let cfg = Loader::new().with_dotenv("definitely/not/here.env");
    assert!(cfg.finish().is_ok());

    let path = env::temp_dir().join(format!("typedenv-bad-{}.env", std::process::id()));
    std::fs::write(&path, "NOEQUALS\n").unwrap();
    let errs = Loader::new().with_dotenv(&path).finish().unwrap_err();
    let _ = std::fs::remove_file(&path);
    assert_eq!(errs.len(), 1);
    assert!(errs.to_string().contains(":1: invalid .env syntax"));
}

#[test]
fn zero_allocation_cow_keys() {
    // 1. Static literal key produces Cow::Borrowed on error (0 heap allocations)
    let err = var::<u16>("TYPEDENV_T11_BORROWED_MISSING").unwrap_err();
    match &err {
        EnvError::Missing { name } => {
            assert!(matches!(name, Cow::Borrowed(_)));
            assert_eq!(name, "TYPEDENV_T11_BORROWED_MISSING");
        }
        _ => panic!("expected missing error"),
    }
    assert_eq!(err.name(), Some("TYPEDENV_T11_BORROWED_MISSING"));
    assert_eq!(
        err.name_cow(),
        Some(&Cow::Borrowed("TYPEDENV_T11_BORROWED_MISSING"))
    );

    // 2. Static literal key with invalid value produces Cow::Borrowed
    set("TYPEDENV_T11_INVALID", "not-a-number");
    let err_inv = var::<u16>("TYPEDENV_T11_INVALID").unwrap_err();
    match &err_inv {
        EnvError::Invalid { name, .. } => {
            assert!(matches!(name, Cow::Borrowed(_)));
            assert_eq!(name, "TYPEDENV_T11_INVALID");
        }
        _ => panic!("expected invalid error"),
    }
    assert_eq!(
        err_inv.name_cow(),
        Some(&Cow::Borrowed("TYPEDENV_T11_INVALID"))
    );

    // 3. Dynamic owned String key produces Cow::Owned
    let dyn_key = format!("TYPEDENV_T11_DYNAMIC_{}", 42);
    let err_dyn = var::<u16>(dyn_key).unwrap_err();
    match &err_dyn {
        EnvError::Missing { name } => {
            assert!(matches!(name, Cow::Owned(_)));
            assert_eq!(name, "TYPEDENV_T11_DYNAMIC_42");
        }
        _ => panic!("expected missing error"),
    }

    // 4. Loader preserves Cow::Borrowed for static keys
    let mut loader = Loader::new();
    let _v1: u16 = loader.var("TYPEDENV_T11_LOADER_MISSING");
    let _v2: u16 = loader.var_or("TYPEDENV_T11_INVALID", 999);
    let errs = loader.finish().unwrap_err();
    assert_eq!(errs.len(), 2);
    assert!(matches!(
        errs[0].name_cow(),
        Some(Cow::Borrowed("TYPEDENV_T11_LOADER_MISSING"))
    ));
    assert!(matches!(
        errs[1].name_cow(),
        Some(Cow::Borrowed("TYPEDENV_T11_INVALID"))
    ));
}
