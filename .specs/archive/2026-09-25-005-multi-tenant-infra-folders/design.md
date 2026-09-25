# Design — Multi-tenant infra folders

## 1. Contract file + language

Contract: `rust/crates/worklog-core/src/tenant_contract.rs` (Rust, `use crate::tenant_contract::*`). Typechecked on 9d2df6b + this file. Web mirror types live in `web/lib/tenants.ts` (T007 owns it; copy field names verbatim, snake_case on the wire).

| canonical | identifier | plural | defined in | banned synonyms |
|---|---|---|---|---|
| multi-tenant folder | `multi_tenant: bool` on `FolderMap` | — | billing_registry.rs | shared folder, mixed folder |
| tenant root | `TenantRoot` | `roots` | tenant_contract.rs | tenant dir, prefix |
| tenant | `Tenant` | `tenants` | tenant_contract.rs | workspace, client |
| tenant link | `TenantLink` | `links` | tenant_contract.rs | mapping, alias |
| clue | `Clue` | `clues` | tenant_contract.rs | hint, signal, evidence |
| clue strength | `ClueStrength` | — | tenant_contract.rs | weight, priority |
| slice | `CustomerSlice` | `slices` | tenant_contract.rs | part, allocation, portion |
| customer shares | `CustomerShares` | — | tenant_contract.rs | split, allocation (collides with `overlap_allocations` and `/blocks/:id/split`) |
| house customer | `HOUSE_CUSTOMER` | — | tenant_contract.rs | apro, default customer |

- Instants: `DateTime<Utc>` in Rust; slice intervals are epoch seconds `[start, end)`, the same shape as `billing::block_interval`.
- Shares are fractions `0..=1`, never percent. The UI shows percent and converts at the action boundary.
- All worklog-core code is sync (rusqlite); daemon handlers wrap it in the existing `with_conn`.

## 2. Trust boundaries

| boundary | untrusted input | parse fn | failure |
|---|---|---|---|
| `POST /billing/tenants/link` | `TenantLink` JSON | `tenants::link_tenant` | 400 per request: unknown folder / unknown customer |
| `POST /blocks/:id/customer-shares` | `{shares: {name: f64}}` | `tenant_shares::save_shares` | 400: shares don't sum to 1 ±0.001, value ≤ 0, unknown customer |
| `events.details` / `title` (DB rows) | free text written by collectors | `tenant_clues::clues_for_block` | per clue: unparseable → no clue, never an error |
| tenant dirs on disk | dir names | `tenants::list_tenants` | missing root/folder → zero tenants |

Branch and path text only; never the event `title` of a browser/Slack row (attacker-set).

## 3. Error taxonomy

`anyhow::bail!` with these exact messages; daemon maps them to 400 with body `{"error": <message>}` (the existing `ApiError` shape): `"Customer no longer exists"`, `"Folder is not multi-tenant"`, `"Shares must add up to 100%"`, `"Share must be above 0"`. Anything else stays 500.

## 4. Module boundaries

- `tenant_contract.rs` · types only · may import: std, chrono, serde.
- `tenants.rs` · roots, discovery, links · may import: tenant_contract, billing_registry, billing (`work_prefix`) · exports: `list_roots`, `list_tenants`, `tenant_customer_map`, `link_tenant`.
- `tenant_clues.rs` · clue extraction · may import: tenant_contract, tenants, billing_registry · exports: `clues_for_block`.
- `tenant_split.rs` · the split rules (FR-06–FR-09, FR-12) · may import: tenant_contract only · exports: `split_block` (pure).
- `tenant_shares.rs` · hand-set shares storage + carving · may import: tenant_contract · exports: `load_shares`, `save_shares`, `clear_shares`, `slices_from_shares`.
- `billing.rs` · calls `tenant_slices_for_block` (in `tenant_split.rs`, the one DB-touching wrapper) and groups slices by customer.
- `daemon_tenants.rs` · child module of `daemon.rs` via `#[path]`, so it reuses the private `with_conn`/`ApiError`.

Anything not listed is a bug.

## 5. Shared resources

- DB handle: `&rusqlite::Connection` passed in; nothing opens its own.
- Migrations: T001 owns every schema change in this feature (bump `SCHEMA_VERSION` once).
- Tables (T001): `billing_folder_map.multi_tenant INTEGER NOT NULL DEFAULT 0`; `billing_tenant_roots(folder, root, UNIQUE(folder, root))`; `billing_tenant_links(folder, tenant, customer NULL, ignored INTEGER DEFAULT 0, UNIQUE(folder, tenant))`; `block_customer_shares(day, started_at, shares JSON, UNIQUE(day, started_at))`.

## 6. Deliberately duplicated

- Interval union: `billing::union_seconds` stays in billing.rs; `tenant_split` doesn't import it — its slices are already disjoint.
- Share-sum validation lives in both `tenant_shares::save_shares` (authoritative) and the UI (instant feedback). Don't have the UI call the daemon to validate.

## 7. Decisions

- In the context of `tenant_split`, facing "summary clues have no timestamp", we chose "timestamped clues own the nearest seconds; a summary clue only decides a block with no timestamped clue" and rejected a per-minute vote, to achieve one rule for D-02 and D-07, accepting that a mid-block summary mention can't move minutes.
- In the context of manual shares, facing "infer deletes and recreates blocks per day", we chose a `(day, started_at)` key and rejected `block_id`, to survive re-infer, accepting A8 (a moved start drops shares).
- In the context of billing grouping, facing "D-01 needs per-block logic", we chose to expand each multi-tenant block into slices before grouping and rejected post-hoc line rewriting, to keep the union rule per line. Makes hard: billing.rs `rows_for_day`.

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
|-----------|-----------|--------------------------------------|
| 4 new modules | 400-line size guard; billing.rs is 1 632 lines | One `tenants.rs` would pass 400 lines |

## Contract for T001 — Schema, seed and the multi_tenant flag
CONTRACT   rust/crates/worklog-core/src/tenant_contract.rs — import from it. A type you need that is not there is an escalation, never a local declaration.
NAMES      multi-tenant folder = `multi_tenant: bool` (banned: shared folder) · tenant root = `TenantRoot`
MODULE     sql/schema.sql, db.rs, billing_registry.rs · add `#[serde(default)] pub multi_tenant: bool` to `FolderMap`; read/write it in `load`/`upsert_folder`
ALSO       declare `pub mod tenants; pub mod tenant_shares; pub mod tenant_clues; pub mod tenant_split;` in lib.rs and create each file with only a `//!` doc line, so later tasks never touch lib.rs
CALLS      seed: `vitinn-infra` root `tenants`; `genai-infra` root `terraform/workspaces/*`; both `multi_tenant = 1`; insert the folder row with `customer = 'APRÓ'` only if missing, never overwrite an existing pin
THE FIVE   (1) NEVER invent an error type, field name or result shape that already exists in the contract — copy the literal declaration. (2) NEVER type a boundary function's parameter as the narrow type; the narrow type is only ever the RETURN of a fallible function. (3) NEVER add a mode, flag or extra required parameter to a shared abstraction the design handed you — duplicate it inside your task and say so. (4) NEVER refactor or rename outside the task's `files:` list — a change to an unlisted file is a defect. (5) NEVER abbreviate inside an identifier. Spell the word.

## Contract for T002 — Tenant discovery and links
CONTRACT   rust/crates/worklog-core/src/tenant_contract.rs — import from it.
NAMES      `Tenant`, `TenantOrigin`, `TenantLink`, `TenantRoot` (banned: workspace, mapping)
MODULE     tenants.rs · exports: `list_roots(conn) -> Result<Vec<TenantRoot>>` · `list_tenants(conn) -> Result<Vec<Tenant>>` · `tenant_customer_map(conn) -> Result<HashMap<(String, String), String>>` (only Alias/Link) · `link_tenant(conn, &TenantLink) -> Result<()>`
CALLS      alias match = `Registry::customer_in_text(name)`; a link beats an alias; Ignored beats both
THE FIVE   same five as T001.

## Contract for T003 — Clues and the split rules
CONTRACT   rust/crates/worklog-core/src/tenant_contract.rs — import from it.
NAMES      `Clue`, `ClueStrength`, `CustomerSlice`, `SplitOrigin`, `HOUSE_CUSTOMER`
MODULE     tenant_clues.rs: `clues_for_block(conn, block_id, folder, &HashMap<(String,String),String>, &Registry) -> Result<Vec<Clue>>` · tenant_split.rs: `split_block(start: i64, end: i64, clues: &[Clue]) -> Option<Vec<CustomerSlice>>` (pure; `None` = no timestamped clue) and `tenant_slices_for_block(conn, &Block, folder, &Registry) -> Result<Option<Vec<CustomerSlice>>>` (shares first, then clues; `None` = not multi-tenant)
CALLS      rule order: drop house clues if any other → per minute keep the strongest → each second to the nearest clue, a tie goes to the earlier clue
THE FIVE   same five as T001.

## Contract for T004 — Hand-set shares
CONTRACT   rust/crates/worklog-core/src/tenant_contract.rs — import from it.
NAMES      `CustomerShares` (banned: split, allocation)
MODULE     tenant_shares.rs · `load_shares(conn, day, started_at) -> Result<Option<CustomerShares>>` · `save_shares(conn, &CustomerShares, &Registry) -> Result<()>` · `clear_shares(conn, day, started_at) -> Result<()>` · `slices_from_shares(start, end, &CustomerShares) -> Vec<CustomerSlice>` (consecutive, customer-name order, the remainder second goes to the last slice)
THE FIVE   same five as T001.

## Contract for T005 — Billing uses slices
CONTRACT   rust/crates/worklog-core/src/tenant_contract.rs — import from it.
MODULE     billing.rs `rows_for_day` · for each slice push `(customer, verkefni, task)` groups with the slice intervals; `Fallback` with `customer: None` goes through today's `registry.resolve`
THE FIVE   same five as T001.

## Contract for T006 — Daemon routes
CONTRACT   rust/crates/worklog-core/src/tenant_contract.rs — import from it.
MODULE     daemon_tenants.rs (child of daemon.rs) · `GET /billing/tenants` → `Vec<Tenant>` · `POST /billing/tenants/link` ← `TenantLink` · `GET /blocks/:id/customer-slices` → `Vec<CustomerSlice>` · `POST /blocks/:id/customer-shares` ← `{shares}` · `POST /blocks/:id/customer-shares/clear`
THE FIVE   same five as T001.

## Contract for T007 — Billing panel UI
MODULE     web/lib/tenants.ts (types mirror the contract + daemon calls) · web/app/tenant-actions.ts ("use server") · web/components/BillingTenantSection.tsx · a multi-tenant checkbox in BillingFolderSection.tsx
THE FIVE   same five as T001.

## Contract for T008 — Block split editor
MODULE     web/components/BlockCustomerSplit.tsx, mounted in BlockCard.tsx · imports types and calls only from web/lib/tenants.ts and web/app/tenant-actions.ts
THE FIVE   same five as T001.
