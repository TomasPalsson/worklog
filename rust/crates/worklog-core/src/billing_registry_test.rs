use super::*;
use crate::db;

fn conn() -> Connection {
    db::open_memory().unwrap()
}

#[test]
fn alias_matching_respects_word_boundaries() {
    assert!(alias_matches("Villur hjá VÍS", "VÍS"));
    assert!(alias_matches("Agent fyrir RL - Atlassian", "RL"));
    // Short aliases must not fire inside longer words.
    assert!(!alias_matches("this is true", "RU"));
    assert!(!alias_matches("RUNNING the agent", "RU"));
    // Boundary characters other than space still count.
    assert!(alias_matches("Spjallbúbbla inn á ru.is", "ru.is"));
    assert!(alias_matches("MMS - lagfæringar", "MMS"));
}

#[test]
fn alias_matching_is_case_insensitive() {
    assert!(alias_matches("document analyzer fyrir sjúkra", "Sjúkra"));
    assert!(alias_matches("Tempo MCP fyrir Apró", "APRÓ"));
}

#[test]
fn customer_in_text_needs_an_unambiguous_hit() {
    let reg = Registry {
        customers: vec![
            Customer {
                id: None,
                name: "Sjúkra".into(),
                aliases: vec![],
            },
            Customer {
                id: None,
                name: "MMS".into(),
                aliases: vec![],
            },
        ],
        folders: vec![],
    };
    assert_eq!(
        reg.customer_in_text("Document analyzer fyrir Sjúkra"),
        Some("Sjúkra".into())
    );
    // Two customers named → ambiguous → left blank, never guessed.
    assert_eq!(reg.customer_in_text("Sjúkra and MMS sync"), None);
    assert_eq!(reg.customer_in_text("no customer here"), None);
}

#[test]
fn resolve_prefers_a_folder_pin_over_text() {
    let reg = Registry {
        customers: vec![Customer {
            id: None,
            name: "MMS".into(),
            aliases: vec![],
        }],
        folders: vec![FolderMap {
            id: None,
            folder: "sjukra".into(),
            customer: Some("Sjúkra".into()),
            verkefni: Some("[P] Vöktun".into()),
            billable: true,
            multi_tenant: false,
        }],
    };
    // Text mentions MMS but the folder is pinned to Sjúkra.
    let r = reg.resolve("sjukra", "MMS schema sync");
    assert_eq!(r.customer, Some("Sjúkra".into()));
    assert_eq!(r.verkefni, Some("[P] Vöktun".into()));
    assert!(r.billable);
}

#[test]
fn resolve_falls_back_to_text_for_a_shared_folder() {
    let reg = Registry {
        customers: vec![Customer {
            id: None,
            name: "Sensa".into(),
            aliases: vec![],
        }],
        folders: vec![FolderMap {
            id: None,
            folder: "genai-infra".into(),
            customer: None, // shared
            verkefni: None,
            billable: true,
            multi_tenant: false,
        }],
    };
    let r = reg.resolve("genai-infra", "Sensa - Deploy Jira MCP í Vitinn-umhverfi");
    assert_eq!(r.customer, Some("Sensa".into()));
    // verkefni is never guessed.
    assert_eq!(r.verkefni, None);
}

#[test]
fn resolve_returns_none_when_undetectable() {
    let reg = Registry::default();
    let r = reg.resolve("mystery-folder", "some work");
    assert_eq!(r.customer, None);
    assert_eq!(r.verkefni, None);
    assert!(r.billable, "unmapped folders default to billable");
}

#[test]
fn customer_crud_round_trips() {
    let c = conn();
    let id = upsert_customer(
        &c,
        &Customer {
            id: None,
            name: "Sensa".into(),
            aliases: vec!["sensa.is".into()],
        },
    )
    .unwrap();
    let all = list_customers(&c).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "Sensa");
    assert_eq!(all[0].aliases, vec!["sensa.is".to_string()]);

    // Upsert by name updates rather than duplicating.
    upsert_customer(
        &c,
        &Customer {
            id: None,
            name: "Sensa".into(),
            aliases: vec!["sensa.is".into(), "Sensa hf".into()],
        },
    )
    .unwrap();
    let all = list_customers(&c).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].aliases.len(), 2);

    assert!(delete_customer(&c, id).unwrap());
    assert!(list_customers(&c).unwrap().is_empty());
}

#[test]
fn folder_crud_blanks_normalise_to_null() {
    let c = conn();
    upsert_folder(
        &c,
        &FolderMap {
            id: None,
            folder: "genai-infra".into(),
            customer: Some("   ".into()),
            verkefni: Some("".into()),
            billable: false,
            multi_tenant: false,
        },
    )
    .unwrap();
    let all = list_folders(&c).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].customer, None, "whitespace-only clears to NULL");
    assert_eq!(all[0].verkefni, None);
    assert!(!all[0].billable);
}

#[test]
fn seed_runs_once_and_never_overwrites() {
    let c = conn();
    assert!(seed_if_empty(&c).unwrap(), "first call seeds");
    let customers = list_customers(&c).unwrap();
    assert!(customers.iter().any(|x| x.name == "Sjúkra"));
    let folders = list_folders(&c).unwrap();
    let website = folders.iter().find(|f| f.folder == "apro-website").unwrap();
    assert_eq!(website.customer, Some("APRÓ".into()));
    assert_eq!(
        website.verkefni,
        Some("Vefsíður APRÓ og dótturfélaga".into())
    );
    // genai-infra is seeded as shared (no pinned customer).
    let shared = folders.iter().find(|f| f.folder == "genai-infra").unwrap();
    assert_eq!(shared.customer, None);

    // A second call is a no-op even after the user edits.
    upsert_folder(
        &c,
        &FolderMap {
            id: None,
            folder: "apro-website".into(),
            customer: Some("Edited".into()),
            verkefni: None,
            billable: true,
            multi_tenant: false,
        },
    )
    .unwrap();
    assert!(!seed_if_empty(&c).unwrap(), "second call does nothing");
    let folders = list_folders(&c).unwrap();
    let website = folders.iter().find(|f| f.folder == "apro-website").unwrap();
    assert_eq!(
        website.customer,
        Some("Edited".into()),
        "seed must not clobber a user edit"
    );
}

#[test]
fn seed_tenant_roots_flags_vitinn_and_genai_infra_multi_tenant() {
    // FR-01, FR-02: a fresh db lists both folders flagged multi_tenant
    // with their tenant roots.
    let c = conn();
    assert!(seed_tenant_roots_if_empty(&c).unwrap(), "first call seeds");

    let roots: Vec<(String, String)> = c
        .prepare("SELECT folder, root FROM billing_tenant_roots ORDER BY folder")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(
        roots,
        vec![
            (
                "genai-infra".to_string(),
                "terraform/workspaces/*".to_string()
            ),
            ("vitinn-infra".to_string(), "tenants".to_string()),
        ]
    );

    let folders = list_folders(&c).unwrap();
    let vitinn = folders.iter().find(|f| f.folder == "vitinn-infra").unwrap();
    assert!(vitinn.multi_tenant);
    assert_eq!(
        vitinn.customer,
        Some("APRÓ".into()),
        "missing pin defaults to the house customer"
    );

    let genai = folders.iter().find(|f| f.folder == "genai-infra").unwrap();
    assert!(genai.multi_tenant);

    // A second call is a no-op.
    assert!(
        !seed_tenant_roots_if_empty(&c).unwrap(),
        "second call does nothing"
    );
}

#[test]
fn seed_tenant_roots_never_overwrites_an_existing_pin() {
    // A2/A1: a folder already pinned by the user (or an earlier
    // seed_if_empty) keeps its customer — only multi_tenant flips on.
    let c = conn();
    upsert_folder(
        &c,
        &FolderMap {
            id: None,
            folder: "genai-infra".into(),
            customer: None, // shared, as seed_if_empty leaves it
            verkefni: None,
            billable: true,
            multi_tenant: false,
        },
    )
    .unwrap();

    assert!(seed_tenant_roots_if_empty(&c).unwrap());

    let folders = list_folders(&c).unwrap();
    let genai = folders.iter().find(|f| f.folder == "genai-infra").unwrap();
    assert_eq!(genai.customer, None, "existing pin must not be overwritten");
    assert!(genai.multi_tenant);
}

#[test]
fn customer_named_matches_name_or_alias_case_insensitively() {
    let reg = Registry {
        customers: vec![Customer {
            id: None,
            name: "Sjúkra".into(),
            aliases: vec!["Sjukratryggingar".into()],
        }],
        folders: vec![],
    };
    assert_eq!(reg.customer_named("sjúkra"), Some("Sjúkra".into()));
    assert_eq!(reg.customer_named("SJÚKRA"), Some("Sjúkra".into()));
    assert_eq!(
        reg.customer_named("sjukratryggingar"),
        Some("Sjúkra".into())
    );
    // A substring match is not enough — customer_named needs the whole
    // name/alias, unlike customer_in_text's free-text search.
    assert_eq!(reg.customer_named("Sjukratryggingar hf"), None);
    assert_eq!(reg.customer_named("Sjukra tryggingar"), None);
    assert_eq!(reg.customer_named(""), None);
}
