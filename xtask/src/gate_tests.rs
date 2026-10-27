use super::*;

fn excluded(path: &str) -> bool {
    regex_lite_match(EXCLUDE, path)
}

// EXCLUDE is a plain alternation of literals/escapes; match each alternative as a substring.
fn regex_lite_match(pattern: &str, path: &str) -> bool {
    pattern
        .trim_matches(|c| c == '(' || c == ')')
        .split('|')
        .any(|alt| path.contains(&alt.replace("\\.", ".")))
}

#[test]
fn exclude_matches_winapi_sys_with_both_separators() {
    assert!(excluded("apps/mouser-service-win/src/winapi_sys.rs"));
    assert!(excluded("apps\\mouser-service-win\\src\\winapi_sys.rs"));
}

#[test]
fn exclude_matches_no_other_service_win_source() {
    for f in ["winapi.rs", "session.rs", "main.rs"] {
        assert!(
            !excluded(&format!("apps/mouser-service-win/src/{f}")),
            "{f}"
        );
    }
}
