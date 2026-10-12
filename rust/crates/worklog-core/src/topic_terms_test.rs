use super::*;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn tokenize_strips_jira_keys_but_not_lookalikes() {
    // catches: leaving "abc-123" -> tokens "abc"; stripping lowercase keys too
    assert_eq!(tokenize("ABC-123 login"), s(&["login"]));
    assert_eq!(tokenize("PROJ2-45 login"), s(&["login"]));
    assert_eq!(tokenize("abc-123 login"), s(&["abc", "login"]));
}

#[test]
fn tokenize_keeps_icelandic_intact() {
    // catches: ascii-only split breaking "þjónusta" into fragments
    assert_eq!(
        tokenize("Þjónusta ÆÐI, ýmis"),
        s(&["þjónusta", "æði", "ýmis"])
    );
}

#[test]
fn tokenize_drops_short_digits_and_stopwords() {
    // catches: > vs >= on length 3; digits kept; stopwords kept
    assert_eq!(tokenize("go app 2024 1234 login12"), s(&["app", "login12"]));
    assert_eq!(
        tokenize("work fixed the new auth over och með"),
        s(&["auth"])
    );
}

#[test]
fn unique_term_outranks_common_term() {
    // catches: plain tf ranking (alpha ties uniq on tf and wins on term order)
    let docs = vec![
        s(&["alpha", "alpha", "uniq", "uniq"]),
        s(&["alpha", "beta", "beta", "beta"]),
    ];
    let k = keywords(&docs, 2);
    assert_eq!(k[0][0], "uniq");
    assert_eq!(k[1][0], "beta");
    assert_eq!(k[0].len(), 2);
}

#[test]
fn keywords_ties_by_term_asc_top_caps_and_empty_ok() {
    // catches: unstable/hash order on ties; ignoring top
    let docs = vec![s(&["zeta", "beta", "mu"]), vec![]];
    assert_eq!(keywords(&docs, 2), vec![s(&["beta", "mu"]), vec![]]);
    assert!(keywords(&[], 5).is_empty());
}

#[test]
fn mmr_never_returns_equal_descriptions() {
    // catches: no dedupe (case/whitespace-insensitive)
    let vecs = vec![
        vec![1.0, 0.0],
        vec![1.0, 0.0],
        vec![0.9, 0.1],
        vec![0.0, 1.0],
    ];
    let d = s(&["Fix login", " fix LOGIN ", "other", "far"]);
    let r = representatives(&vecs, &d, 4);
    assert!(r.contains(&0) && !r.contains(&1));
    assert_eq!(r.len(), 3);
}

#[test]
fn mmr_returns_at_most_n_and_first_is_closest_to_centroid() {
    // catches: n ignored; picking farthest; non-deterministic tie (lower index wins)
    let vecs = vec![
        vec![1.0, 1.0],
        vec![1.0, 1.0],
        vec![1.0, 0.0],
        vec![0.0, 1.0],
    ];
    let d = s(&["a", "b", "c", "d"]);
    let r = representatives(&vecs, &d, 2);
    assert_eq!(r.len(), 2);
    assert_eq!(r[0], 0);
    assert!(representatives(&vecs, &d, 0).is_empty());
    assert_eq!(representatives(&vecs, &d, 99).len(), 4);
    assert!(representatives(&[], &[], 3).is_empty());
}

#[test]
fn mmr_penalises_redundancy() {
    // hand-computed: centroid (0.83,0.27); 1 is closest, 0 is its near-duplicate (0.45) vs 2 (0.50)
    // catches: pure centroid-similarity ranking (would give [1, 0])
    let vecs = vec![vec![1.0, 0.0], vec![0.99, 0.01], vec![0.5, 0.8]];
    let d = s(&["a", "b", "c"]);
    assert_eq!(representatives(&vecs, &d, 2), vec![1, 2]);
}
