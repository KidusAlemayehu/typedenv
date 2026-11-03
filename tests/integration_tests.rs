use std::env;
use typedenv::{EnvError, Loader, var, var_bool, var_bool_or, var_opt, var_or};

#[test]
fn test_var_lookups() {
    let key_ok = "ENVTYPED_TEST_PORT_OK";
    let key_bad = "ENVTYPED_TEST_PORT_BAD";
    let key_missing = "ENVTYPED_TEST_PORT_MISSING";

    unsafe {
        env::set_var(key_ok, " 8080 ");
        env::set_var(key_bad, "not-a-number");
        env::remove_var(key_missing);
    }

    // var
    assert_eq!(var::<u16>(key_ok).unwrap(), 8080);
    assert_eq!(var::<String>(key_ok).unwrap(), "8080");
    assert!(matches!(var::<u16>(key_bad), Err(EnvError::Invalid { .. })));
    assert!(matches!(
        var::<u16>(key_missing),
        Err(EnvError::Missing { .. })
    ));

    // var_or: malformed is an error!
    assert_eq!(var_or::<u16>(key_ok, 3000).unwrap(), 8080);
    assert_eq!(var_or::<u16>(key_missing, 3000).unwrap(), 3000);
    assert!(matches!(
        var_or::<u16>(key_bad, 3000),
        Err(EnvError::Invalid { .. })
    ));

    // var_opt
    assert_eq!(var_opt::<u16>(key_ok).unwrap(), Some(8080));
    assert_eq!(var_opt::<u16>(key_missing).unwrap(), None);
    assert!(matches!(
        var_opt::<u16>(key_bad),
        Err(EnvError::Invalid { .. })
    ));

    // var_bool and var_bool_or
    let key_bool = "ENVTYPED_TEST_FLAG";
    unsafe {
        env::set_var(key_bool, "yes");
    }
    assert!(var_bool(key_bool).unwrap());
    assert!(!var_bool_or("ENVTYPED_TEST_UNSET_BOOL", false).unwrap());

    unsafe {
        env::remove_var(key_ok);
        env::remove_var(key_bad);
        env::remove_var(key_bool);
    }
}

#[test]
fn test_loader_accumulates_errors() {
    let key_valid = "ENVTYPED_LOADER_VALID";
    let key_invalid = "ENVTYPED_LOADER_INVALID";
    let key_missing = "ENVTYPED_LOADER_MISSING";

    unsafe {
        env::set_var(key_valid, "1234");
        env::set_var(key_invalid, "xyz");
        env::remove_var(key_missing);
    }

    let mut loader = Loader::new();
    let valid: u32 = loader.var(key_valid);
    let missing: u32 = loader.var(key_missing);
    let invalid: u32 = loader.var(key_invalid);
    let default_unset: u32 = loader.var_or("ENVTYPED_UNSET", 99);
    let default_invalid: u32 = loader.var_or(key_invalid, 99);
    let opt_missing: Option<u32> = loader.var_opt("ENVTYPED_UNSET");
    let opt_invalid: Option<u32> = loader.var_opt(key_invalid);
    let bool_missing = loader.var_bool("ENVTYPED_UNSET_BOOL");

    assert_eq!(valid, 1234);
    assert_eq!(missing, 0); // placeholder
    assert_eq!(invalid, 0); // placeholder
    assert_eq!(default_unset, 99);
    assert_eq!(default_invalid, 99);
    assert_eq!(opt_missing, None);
    assert_eq!(opt_invalid, None);
    assert!(!bool_missing);

    let res = loader.finish();
    assert!(res.is_err());
    let errs = res.unwrap_err();

    // Check error count:
    // key_missing (1) + key_invalid (2) + default_invalid (3) + opt_invalid (4) + bool_missing (5)
    assert_eq!(errs.len(), 5);
    assert!(!errs.is_empty());

    // Test Deref & Iteration
    let slice: &[EnvError] = &errs;
    assert_eq!(slice.len(), 5);

    let mut count = 0;
    for _ in &errs {
        count += 1;
    }
    assert_eq!(count, 5);

    // Test display output
    let msg = errs.to_string();
    assert!(msg.contains("5 configuration errors:"));
    assert!(msg.contains(key_missing));
    assert!(msg.contains(key_invalid));

    // Test into_vec
    let vec = errs.into_vec();
    assert_eq!(vec.len(), 5);

    unsafe {
        env::remove_var(key_valid);
        env::remove_var(key_invalid);
    }
}

#[test]
fn test_zero_allocation_cow_keys() {
    use std::borrow::Cow;

    let static_key = "ENVTYPED_INTEG_MISSING";
    let err = var::<u16>(static_key).unwrap_err();
    assert_eq!(err.name(), Some(static_key));
    assert_eq!(err.name_cow(), Some(&Cow::Borrowed(static_key)));

    let owned_key = format!("ENVTYPED_INTEG_OWNED_{}", 99);
    let err_owned = var::<u16>(owned_key).unwrap_err();
    assert_eq!(err_owned.name(), Some("ENVTYPED_INTEG_OWNED_99"));
    assert_eq!(
        err_owned.name_cow(),
        Some(&Cow::Owned("ENVTYPED_INTEG_OWNED_99".to_string()))
    );

    let cow_direct: Cow<'static, str> = Cow::Borrowed("ENVTYPED_INTEG_DIRECT");
    let err_cow = var::<u16>(cow_direct).unwrap_err();
    assert_eq!(
        err_cow.name_cow(),
        Some(&Cow::Borrowed("ENVTYPED_INTEG_DIRECT"))
    );
}
