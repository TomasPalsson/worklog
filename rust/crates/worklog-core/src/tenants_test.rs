use super::*;
use crate::billing_registry::{seed_if_empty, upsert_customer, Customer};
use crate::db::open_memory;
use crate::tenant_contract::HOUSE_CUSTOMER;

fn add_root(conn: &Connection, folder: &str, root: &str) {
    conn.execute(
        "INSERT INTO billing_tenant_roots (folder, root) VALUES (?1, ?2)",
        params![folder, root],
    )
    .unwrap();
}

fn make_multi_tenant(conn: &Connection, folder: &str) {
    crate::billing_registry::upsert_folder(
        conn,
        &crate::billing_registry::FolderMap {
            id: None,
            folder: folder.into(),
            customer: Some(HOUSE_CUSTOMER.into()),
            verkefni: None,
            billable: true,
            multi_tenant: true,
        },
    )
    .unwrap();
}

#[test]
fn list_roots_returns_seeded_roots_in_folder_order() {
    let conn = open_memory().unwrap();
    add_root(&conn, "genai-infra", "terraform/workspaces/*");
    add_root(&conn, "vitinn-infra", "tenants");

    let roots = list_roots(&conn).unwrap();
    assert_eq!(
        roots,
        vec![
            TenantRoot {
                folder: "genai-infra".into(),
                root: "terraform/workspaces/*".into(),
            },
            TenantRoot {
                folder: "vitinn-infra".into(),
                root: "tenants".into(),
            },
        ]
    );
}

#[test]
fn list_tenants_under_discovers_plain_and_wildcard_roots_and_matches_aliases() {
    // FR-03: `sjukra` -> Sjukra, `apro-prod` -> APRO by alias match.
    // FR-04: an unrecognised tenant directory is listed Unmatched.
    let conn = open_memory().unwrap();
    seed_if_empty(&conn).unwrap();
    make_multi_tenant(&conn, "vitinn-infra");
    make_multi_tenant(&conn, "genai-infra");
    add_root(&conn, "vitinn-infra", "tenants");
    add_root(&conn, "genai-infra", "terraform/workspaces/*");

    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();
    std::fs::create_dir_all(base.join("vitinn-infra/tenants/sjukra")).unwrap();
    std::fs::create_dir_all(base.join("vitinn-infra/tenants/byko-datalake")).unwrap();
    std::fs::create_dir_all(base.join("genai-infra/terraform/workspaces/prod/apro-prod")).unwrap();
    std::fs::create_dir_all(base.join("genai-infra/terraform/workspaces/staging")).unwrap();
    // A file next to the workspace dirs must never be treated as a tenant.
    std::fs::write(base.join("genai-infra/terraform/workspaces/README.md"), "x").unwrap();

    let mut tenants = list_tenants_under(&conn, base).unwrap();
    tenants.sort_by(|a, b| (a.folder.as_str(), a.name.as_str()).cmp(&(b.folder.as_str(), b.name.as_str())));

    assert_eq!(
        tenants,
        vec![
            Tenant {
                folder: "genai-infra".into(),
                name: "apro-prod".into(),
                customer: Some("APRÓ".into()),
                origin: TenantOrigin::Alias,
            },
            Tenant {
                folder: "vitinn-infra".into(),
                name: "byko-datalake".into(),
                customer: None,
                origin: TenantOrigin::Unmatched,
            },
            Tenant {
                folder: "vitinn-infra".into(),
                name: "sjukra".into(),
                customer: Some("Sjúkra".into()),
                origin: TenantOrigin::Alias,
            },
        ]
    );
}

#[test]
fn list_tenants_under_returns_nothing_for_a_missing_folder() {
    // Design trust boundary: missing root/folder on disk -> zero tenants,
    // never an error.
    let conn = open_memory().unwrap();
    add_root(&conn, "vitinn-infra", "tenants");
    let tmp = tempfile::tempdir().unwrap();

    let tenants = list_tenants_under(&conn, tmp.path()).unwrap();
    assert!(tenants.is_empty());
}

#[test]
fn customer_map_from_only_carries_alias_and_link_origins() {
    let tenants = vec![
        Tenant {
            folder: "vitinn-infra".into(),
            name: "sjukra".into(),
            customer: Some("Sjúkra".into()),
            origin: TenantOrigin::Alias,
        },
        Tenant {
            folder: "vitinn-infra".into(),
            name: "byko-datalake".into(),
            customer: Some("MMS".into()),
            origin: TenantOrigin::Link,
        },
        Tenant {
            folder: "vitinn-infra".into(),
            name: "uat".into(),
            customer: None,
            origin: TenantOrigin::Ignored,
        },
        Tenant {
            folder: "vitinn-infra".into(),
            name: "mystery".into(),
            customer: None,
            origin: TenantOrigin::Unmatched,
        },
    ];

    let map = customer_map_from(tenants);
    assert_eq!(map.len(), 2);
    assert_eq!(
        map.get(&("vitinn-infra".to_string(), "sjukra".to_string())),
        Some(&"Sjúkra".to_string())
    );
    assert_eq!(
        map.get(&("vitinn-infra".to_string(), "byko-datalake".to_string())),
        Some(&"MMS".to_string())
    );
    assert!(!map.contains_key(&("vitinn-infra".to_string(), "uat".to_string())));
    assert!(!map.contains_key(&("vitinn-infra".to_string(), "mystery".to_string())));
}

#[test]
fn link_tenant_a_link_beats_an_alias_and_ignored_beats_both() {
    let conn = open_memory().unwrap();
    seed_if_empty(&conn).unwrap();
    make_multi_tenant(&conn, "vitinn-infra");
    add_root(&conn, "vitinn-infra", "tenants");

    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();
    // `sjukra` would alias-match Sjukra on its own.
    std::fs::create_dir_all(base.join("vitinn-infra/tenants/sjukra")).unwrap();

    link_tenant(
        &conn,
        &TenantLink {
            folder: "vitinn-infra".into(),
            tenant: "sjukra".into(),
            customer: Some("MMS".into()),
            ignored: false,
        },
    )
    .unwrap();
    let tenants = list_tenants_under(&conn, base).unwrap();
    assert_eq!(tenants[0].origin, TenantOrigin::Link);
    assert_eq!(tenants[0].customer, Some("MMS".into()));

    link_tenant(
        &conn,
        &TenantLink {
            folder: "vitinn-infra".into(),
            tenant: "sjukra".into(),
            customer: None,
            ignored: true,
        },
    )
    .unwrap();
    let tenants = list_tenants_under(&conn, base).unwrap();
    assert_eq!(tenants[0].origin, TenantOrigin::Ignored);
    assert_eq!(tenants[0].customer, None);
}

#[test]
fn link_tenant_clearing_the_link_falls_back_to_alias_matching() {
    let conn = open_memory().unwrap();
    seed_if_empty(&conn).unwrap();
    make_multi_tenant(&conn, "vitinn-infra");
    add_root(&conn, "vitinn-infra", "tenants");
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();
    std::fs::create_dir_all(base.join("vitinn-infra/tenants/sjukra")).unwrap();

    link_tenant(
        &conn,
        &TenantLink {
            folder: "vitinn-infra".into(),
            tenant: "sjukra".into(),
            customer: Some("MMS".into()),
            ignored: false,
        },
    )
    .unwrap();
    // customer: None + ignored: false removes the link.
    link_tenant(
        &conn,
        &TenantLink {
            folder: "vitinn-infra".into(),
            tenant: "sjukra".into(),
            customer: None,
            ignored: false,
        },
    )
    .unwrap();

    let tenants = list_tenants_under(&conn, base).unwrap();
    assert_eq!(tenants[0].origin, TenantOrigin::Alias);
    assert_eq!(tenants[0].customer, Some("Sjúkra".into()));
}

#[test]
fn link_tenant_rejects_an_unknown_customer() {
    let conn = open_memory().unwrap();
    make_multi_tenant(&conn, "vitinn-infra");
    let err = link_tenant(
        &conn,
        &TenantLink {
            folder: "vitinn-infra".into(),
            tenant: "sjukra".into(),
            customer: Some("Nobody".into()),
            ignored: false,
        },
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "Customer no longer exists");
}

#[test]
fn link_tenant_rejects_a_folder_that_is_not_multi_tenant() {
    let conn = open_memory().unwrap();
    upsert_customer(
        &conn,
        &Customer {
            id: None,
            name: "Sjúkra".into(),
            aliases: vec![],
        },
    )
    .unwrap();
    let err = link_tenant(
        &conn,
        &TenantLink {
            folder: "sjukra".into(),
            tenant: "prod".into(),
            customer: Some("Sjúkra".into()),
            ignored: false,
        },
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "Folder is not multi-tenant");
}
