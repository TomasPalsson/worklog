use super::*;

fn acc(id: &str, name: &str) -> AllowedAccount {
    AllowedAccount {
        id: id.into(),
        name: name.into(),
    }
}

#[test]
fn fold_strips_accents_and_case() {
    assert_eq!(fold("[P] Vitinn VÖRUÞRÓUN"), "[p] vitinn voruthroun");
}

#[test]
fn matching_is_accent_blind_and_needs_every_word() {
    let all = [acc("1", "[P] Vitinn vöruþróun")];
    for q in ["vöruþróun", "voruthroun", "VITINN voru", ""] {
        assert_eq!(matching(&all, q).len(), 1, "{q}");
    }
    assert!(matching(&all, "apro voruthroun").is_empty());
}

#[test]
fn resolve_digits_pass_through() {
    assert_eq!(resolve_account(&[], "42").unwrap(), "42");
}

#[test]
fn resolve_rejects_a_blank_value_instead_of_picking_the_only_account() {
    let one = [acc("7", "Alpha")];
    for v in ["", " ", "\t"] {
        assert!(resolve_account(&one, v).is_err(), "{v:?}");
        assert!(!is_account_id(v), "{v:?}");
    }
    assert!(is_account_id("155"));
    assert!(!is_account_id("vöruþróun"));
}

#[test]
fn resolve_one_match() {
    let all = [acc("7", "Alpha"), acc("8", "Beta")];
    assert_eq!(resolve_account(&all, "beta").unwrap(), "8");
}

#[test]
fn resolve_zero_matches() {
    let e = resolve_account(&[acc("7", "Alpha")], "zzz").unwrap_err();
    assert!(e.to_string().contains("account allowed"));
}

#[test]
fn resolve_many_lists_ids() {
    let all = [acc("7", "Alpha one"), acc("8", "Alpha two")];
    let e = resolve_account(&all, "alpha").unwrap_err().to_string();
    assert!(e.contains("7  Alpha one") && e.contains("8  Alpha two"));
}
