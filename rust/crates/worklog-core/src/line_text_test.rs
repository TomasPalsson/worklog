//! Tests for T020 — `line_text::validate` / `generate_for_day` /
//! `set_manual` / `text_for` (spec 006, D-12, D-14, FR-26, FR-31, FR-35).

use super::*;
use crate::billing_registry::{upsert_folder, FolderMap};
use crate::db;
use crate::estimate::FixedInvoker;
use crate::models::Event;
use anyhow::anyhow;
use rusqlite::Connection;
use serde_json::json;

fn home_work(sub: &str) -> String {
    format!(
        "{}/Desktop/Work/{sub}",
        dirs::home_dir().unwrap().to_string_lossy()
    )
}

fn pin_folder(conn: &Connection, folder: &str, customer: &str) {
    upsert_folder(
        conn,
        &FolderMap {
            id: None,
            folder: folder.to_string(),
            customer: Some(customer.to_string()),
            verkefni: None,
            billable: true,
            multi_tenant: false,
        },
    )
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
fn seed_block(
    conn: &Connection,
    day: &str,
    started_at: &str,
    ended_at: &str,
    duration_seconds: i64,
    folder: &str,
    source_id: &str,
) -> i64 {
    conn.execute(
        "INSERT INTO blocks (day, started_at, ended_at, duration_seconds) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![day, started_at, ended_at, duration_seconds],
    )
    .unwrap();
    let block_id = conn.last_insert_rowid();
    let event_id = crate::repo::upsert_event(
        conn,
        &Event {
            project_path: Some(home_work(folder)),
            ..Event::minimal("shell", source_id, started_at, "ls")
        },
    )
    .unwrap();
    conn.execute(
        "INSERT INTO block_events (block_id, event_id) VALUES (?1, ?2)",
        rusqlite::params![block_id, event_id],
    )
    .unwrap();
    block_id
}

const GOOD_TEXT: &str =
    "Lauk verkefninu í dag. Allt gekk vel og viðskiptavinurinn var ánægður.";
const ENGLISH_TEXT: &str = "Fixed the login bug. Added tests.";

struct ErrInvoker;
impl crate::estimate::ModelInvoker for ErrInvoker {
    fn invoke(
        &self,
        _s: &str,
        _u: &str,
        _sc: &serde_json::Value,
        _m: &str,
    ) -> anyhow::Result<serde_json::Value> {
        Err(anyhow!("model unreachable"))
    }
}

/// The prompt must tell the model to describe the day task by task from
/// `work_items` — at most 3, biggest first, small leftovers folded into
/// "auk minni verkefna" or dropped — never joining two unrelated tasks
/// into one sentence, never ordering by time of day, and never
/// attributing a detail to the wrong task.
#[test]
fn line_text_system_prompt_describes_work_items_task_by_task() {
    assert!(SYSTEM_PROMPT_IS.contains("work_items"));
    assert!(SYSTEM_PROMPT_IS.contains("auk minni verkefna"));
    assert!(SYSTEM_PROMPT_IS.contains("þrjú"), "cap at 3 tasks");
    assert!(
        SYSTEM_PROMPT_IS.to_lowercase().contains("tímaröð"),
        "must forbid ordering by time of day"
    );
    assert!(
        SYSTEM_PROMPT_IS.contains("blanda saman") || SYSTEM_PROMPT_IS.contains("ólíkum verkefnum"),
        "must forbid joining unrelated tasks in one sentence"
    );
    // Every pre-existing rule survives the rewrite.
    assert!(SYSTEM_PROMPT_IS.contains("{\"text\""));
    assert!(SYSTEM_PROMPT_IS.contains("PR-"));
    assert!(SYSTEM_PROMPT_IS.contains("skráarnöfn"));
    assert!(SYSTEM_PROMPT_IS.contains("verkfærum"));
    assert!(SYSTEM_PROMPT_IS.contains("íslensku"));
}

/// "lagaði vandamál í keyrsluumhverfi" says nothing: each task must name
/// what was wrong/changed, where, and the effect — when the clues show it.
#[test]
fn line_text_system_prompt_asks_for_specifics() {
    assert!(SYSTEM_PROMPT_IS.contains("hvaða umhverfi"));
    assert!(
        SYSTEM_PROMPT_IS.contains("vandamál í keyrsluumhverfi"),
        "vague counter-example"
    );
    assert!(SYSTEM_PROMPT_IS.contains("finna upp"), "still never invent");
    assert!(!SYSTEM_PROMPT_IS.contains("stafir"), "no char cap");
}

/// Icelandic devs say "autorouterinn", not a coined "sjálfvirkri
/// val-á-líkani": the prompt must keep English tech terms (inflected the
/// Icelandic way) instead of forcing a translation nobody uses.
#[test]
fn line_text_system_prompt_keeps_english_tech_terms() {
    assert!(
        !SYSTEM_PROMPT_IS.contains("forðastu ensk tæknihugtök"),
        "must not force translating English tech terms"
    );
    assert!(
        SYSTEM_PROMPT_IS.contains("autorouterinn"),
        "example of an inflected loanword"
    );
    assert!(
        SYSTEM_PROMPT_IS.contains("nýyrði"),
        "must forbid coining new words"
    );
    // The model wrote "kóðakeyrsla" for the code interpreter and
    // "framleiðsluumhverfið" for production: say what devs actually say.
    assert!(SYSTEM_PROMPT_IS.contains("en ekki \"kóðakeyrsla\""));
    assert!(SYSTEM_PROMPT_IS.contains("í production"));
    assert!(
        !SYSTEM_PROMPT_IS.contains("framleiðsluumhverfið,"),
        "no Icelandic-coined examples"
    );
    assert!(
        SYSTEM_PROMPT_IS.contains("germynd"),
        "active voice, like devs talk"
    );
}

#[test]
fn line_text_validate_accepts_good_icelandic_text() {
    assert_eq!(validate(GOOD_TEXT).unwrap(), GOOD_TEXT);
}

#[test]
fn line_text_validate_rejects_one_sentence() {
    assert!(validate("Verkefninu var lokið í dag").is_err());
}

#[test]
fn line_text_validate_rejects_four_sentences() {
    let text = "Það var gaman. Ég gerði hitt. Svo gerði ég þetta. Loks kláraði ég verkið.";
    assert!(validate(text).is_err());
}

/// No length cap: the owner wants detail, and the cap only threw away
/// otherwise good replies.
#[test]
fn line_text_validate_accepts_long_text() {
    let text = format!(
        "Lagaði villu í production. Uppfærði autorouterinn {}.",
        "og fleira ".repeat(60)
    );
    assert!(validate(&text).is_ok());
}

#[test]
fn line_text_validate_allows_numbers() {
    assert!(validate("Uppfærði í útgáfu 3. Lagaði 2 villur.").is_ok());
}

#[test]
fn line_text_validate_rejects_hash() {
    assert!(validate("#44").is_err());
}

#[test]
fn line_text_validate_rejects_path() {
    assert!(validate("src/main.rs").is_err());
}

#[test]
fn line_text_validate_rejects_bare_filename() {
    assert!(validate("main.rs").is_err());
}

#[test]
fn line_text_validate_rejects_tool_name() {
    assert!(validate("Bash").is_err());
}

#[test]
fn line_text_validate_rejects_english_only() {
    assert!(validate(ENGLISH_TEXT).is_err());
}

#[test]
fn line_text_validate_rejects_json_wrapper() {
    let wrapped = format!("{{\"text\": \"{GOOD_TEXT}\"}}");
    assert!(validate(&wrapped).is_err());
}

#[test]
fn line_text_unwraps_text_nested_as_json_inside_text() {
    // Seen from real `claude -p`: the schema's `text` held the whole
    // `{"text": "..."}` object again as a string.
    let conn = db::open_memory().unwrap();
    let day = "2026-06-01";
    pin_folder(&conn, "line-text-folder-a", "Acme Corp");
    seed_block(
        &conn,
        day,
        "2026-06-01T09:00:00+00:00",
        "2026-06-01T09:30:00+00:00",
        1800,
        "line-text-folder-a",
        "l1",
    );
    let nested = json!({ "text": GOOD_TEXT }).to_string();
    let invoker = FixedInvoker(json!({ "text": nested }));
    let report = generate_for_day(&conn, day, &invoker, "model").unwrap();
    let (text, _) = text_for(&conn, &report.generated[0]).unwrap().unwrap();
    assert_eq!(text, GOOD_TEXT);
}

#[test]
fn line_text_generate_for_day_stores_one_row_per_line_key() {
    let conn = db::open_memory().unwrap();
    let day = "2026-06-01";
    pin_folder(&conn, "line-text-folder-a", "Acme Corp");
    pin_folder(&conn, "line-text-folder-b", "Beta ehf");
    seed_block(
        &conn,
        day,
        "2026-06-01T09:00:00+00:00",
        "2026-06-01T09:30:00+00:00",
        1800,
        "line-text-folder-a",
        "l1",
    );
    seed_block(
        &conn,
        day,
        "2026-06-01T10:00:00+00:00",
        "2026-06-01T10:30:00+00:00",
        1800,
        "line-text-folder-b",
        "l2",
    );

    let invoker = FixedInvoker(json!({"text": GOOD_TEXT}));
    let report = generate_for_day(&conn, day, &invoker, "model").unwrap();

    assert_eq!(
        report.generated.len(),
        2,
        "not_generated: {:?}",
        report.not_generated
    );
    assert!(report.not_generated.is_empty());
    for key in &report.generated {
        let (text, origin) = text_for(&conn, key).unwrap().unwrap();
        assert_eq!(text, GOOD_TEXT);
        assert_eq!(origin, LineTextOrigin::Generated);
    }
}

#[test]
fn line_text_manual_never_overwritten() {
    let conn = db::open_memory().unwrap();
    let day = "2026-06-02";
    let folder = "line-text-manual-folder";
    pin_folder(&conn, folder, "Acme Corp");
    seed_block(
        &conn,
        day,
        "2026-06-02T09:00:00+00:00",
        "2026-06-02T09:30:00+00:00",
        1800,
        folder,
        "m1",
    );
    let key = BillingLineKey {
        day: day.to_string(),
        folder: folder.to_string(),
        customer: "Acme Corp".to_string(),
    };
    let manual_text = "Handskrifaður texti sem eigandinn skrifaði sjálfur.";
    set_manual(&conn, &key, manual_text).unwrap();

    let invoker = FixedInvoker(json!({"text": GOOD_TEXT}));
    let report = generate_for_day(&conn, day, &invoker, "model").unwrap();

    assert!(!report.generated.contains(&key));
    assert!(!report.not_generated.iter().any(|(k, _)| k == &key));
    let (text, origin) = text_for(&conn, &key).unwrap().unwrap();
    assert_eq!(text, manual_text);
    assert_eq!(origin, LineTextOrigin::Manual);
}

#[test]
fn line_text_failure_keeps_previous_text_on_invoker_error() {
    let conn = db::open_memory().unwrap();
    let day = "2026-06-03";
    let folder = "line-text-fail-folder";
    pin_folder(&conn, folder, "Acme Corp");
    seed_block(
        &conn,
        day,
        "2026-06-03T09:00:00+00:00",
        "2026-06-03T09:30:00+00:00",
        1800,
        folder,
        "f1",
    );
    let key = BillingLineKey {
        day: day.to_string(),
        folder: folder.to_string(),
        customer: "Acme Corp".to_string(),
    };

    let good_invoker = FixedInvoker(json!({"text": GOOD_TEXT}));
    let first = generate_for_day(&conn, day, &good_invoker, "model").unwrap();
    assert!(first.generated.contains(&key));
    let before = text_for(&conn, &key).unwrap().unwrap();

    let report = generate_for_day(&conn, day, &ErrInvoker, "model").unwrap();

    assert!(!report.generated.contains(&key));
    assert!(report.not_generated.iter().any(|(k, _)| k == &key));
    let after = text_for(&conn, &key).unwrap().unwrap();
    assert_eq!(before, after);
}

#[test]
fn line_text_failure_keeps_previous_text_on_invalid_reply() {
    let conn = db::open_memory().unwrap();
    let day = "2026-06-04";
    let folder = "line-text-invalid-folder";
    pin_folder(&conn, folder, "Acme Corp");
    seed_block(
        &conn,
        day,
        "2026-06-04T09:00:00+00:00",
        "2026-06-04T09:30:00+00:00",
        1800,
        folder,
        "i1",
    );
    let key = BillingLineKey {
        day: day.to_string(),
        folder: folder.to_string(),
        customer: "Acme Corp".to_string(),
    };

    let good_invoker = FixedInvoker(json!({"text": GOOD_TEXT}));
    generate_for_day(&conn, day, &good_invoker, "model").unwrap();
    let before = text_for(&conn, &key).unwrap().unwrap();

    let bad_invoker = FixedInvoker(json!({"text": ENGLISH_TEXT}));
    let report = generate_for_day(&conn, day, &bad_invoker, "model").unwrap();

    assert!(!report.generated.contains(&key));
    assert!(report.not_generated.iter().any(|(k, _)| k == &key));
    let after = text_for(&conn, &key).unwrap().unwrap();
    assert_eq!(before, after);
}

#[test]
fn line_text_set_manual_empty_deletes_row() {
    let conn = db::open_memory().unwrap();
    let key = BillingLineKey {
        day: "2026-06-05".to_string(),
        folder: "line-text-delete-folder".to_string(),
        customer: "Acme Corp".to_string(),
    };
    set_manual(&conn, &key, "Eitthvað sem eigandinn skrifaði.").unwrap();
    assert!(text_for(&conn, &key).unwrap().is_some());

    set_manual(&conn, &key, "   ").unwrap();
    assert!(text_for(&conn, &key).unwrap().is_none());
}

#[test]
fn line_text_text_for_round_trips_origin() {
    let conn = db::open_memory().unwrap();
    let key = BillingLineKey {
        day: "2026-06-06".to_string(),
        folder: "line-text-roundtrip-folder".to_string(),
        customer: "Acme Corp".to_string(),
    };
    set_manual(&conn, &key, "Handskrifaður texti með ö og allt sem virkar.").unwrap();
    let (text, origin) = text_for(&conn, &key).unwrap().unwrap();
    assert_eq!(text, "Handskrifaður texti með ö og allt sem virkar.");
    assert_eq!(origin, LineTextOrigin::Manual);
}

#[test]
fn line_text_invoker_error_reports_every_line_key_not_generated() {
    // FR-35: when the model invoker itself can't be built (misconfigured
    // provider), every distinct billing line of the day must surface in
    // `not_generated` with that reason — not silently vanish into an
    // empty report (the previous behaviour daemon.rs's `run_estimate`
    // fell back to on any `Err` from `generate_with_default_provider`).
    let conn = db::open_memory().unwrap();
    let day = "2026-06-07";
    pin_folder(&conn, "line-text-folder-err-a", "Acme Corp");
    pin_folder(&conn, "line-text-folder-err-b", "Beta ehf");
    seed_block(
        &conn,
        day,
        "2026-06-07T09:00:00+00:00",
        "2026-06-07T09:30:00+00:00",
        1800,
        "line-text-folder-err-a",
        "e1",
    );
    seed_block(
        &conn,
        day,
        "2026-06-07T10:00:00+00:00",
        "2026-06-07T10:30:00+00:00",
        1800,
        "line-text-folder-err-b",
        "e2",
    );

    let report = report_for_invoker_error(&conn, day, "provider not configured").unwrap();
    assert!(report.generated.is_empty());
    assert_eq!(report.not_generated.len(), 2);
    assert!(report
        .not_generated
        .iter()
        .all(|(_, reason)| reason == "provider not configured"));
}

#[test]
fn line_text_invoker_error_skips_manual_origin_lines() {
    // NF-5: a hand-edited line would have been skipped by generate_for_day
    // too (FR-31) — an invoker-construction failure must not report it as
    // "not generated" either, since generation was never going to touch
    // it in the first place.
    let conn = db::open_memory().unwrap();
    let day = "2026-06-08";
    pin_folder(&conn, "line-text-folder-manual", "Acme Corp");
    pin_folder(&conn, "line-text-folder-auto", "Beta ehf");
    seed_block(
        &conn,
        day,
        "2026-06-08T09:00:00+00:00",
        "2026-06-08T09:30:00+00:00",
        1800,
        "line-text-folder-manual",
        "e1",
    );
    seed_block(
        &conn,
        day,
        "2026-06-08T10:00:00+00:00",
        "2026-06-08T10:30:00+00:00",
        1800,
        "line-text-folder-auto",
        "e2",
    );
    let manual_key = BillingLineKey {
        day: day.to_string(),
        folder: "line-text-folder-manual".to_string(),
        customer: "Acme Corp".to_string(),
    };
    set_manual(
        &conn,
        &manual_key,
        "Eitthvað sem eigandinn skrifaði sjálfur.",
    )
    .unwrap();

    let report = report_for_invoker_error(&conn, day, "provider not configured").unwrap();
    assert!(report.generated.is_empty());
    assert_eq!(report.not_generated.len(), 1, "{:?}", report.not_generated);
    assert!(
        report.not_generated.iter().all(|(k, _)| k != &manual_key),
        "the hand-edited line must never be reported as not_generated"
    );
}

#[test]
fn prompt_tells_the_model_about_other_tickets_today() {
    assert!(SYSTEM_PROMPT_IS.contains("other_tickets_today"));
}

const GOOD_TEXTS: [&str; 8] = [
    "Tengdi VÍS Geitin frá Boost.ai við LibreChat í VÍS-tenantnum í gegnum LiteLLM og lagaði A2A-skilaboð þannig að agentinn fái nýjustu notandaskilaboðin; takmarkaði jafnframt aðgang með netfangi og setti upp samhengi fyrir hvert spjall. Undirbjó uppfærslu LiteLLM í v1.104.0 fyrir apro-prod.",
    "Vann að svari fyrir SJTR um logging storage í APRÓ AI. Tók mið af spurningum um hvaða upplýsingar skrást þegar starfsfólk notar Run Code og hvort hægt sé að fá afrit af loggunum.",
    "Endurskoðaði umfangsmat og kostnaðartillögu fyrir LiteLLM-uppsetningu RL fyrir marga sjóði: bætti við breytilegum tíma fyrir fundi og kynningar, hækkaði Enterprise-kostnaðinn og skýrði að mögulegur Keydesk-kostnaður væri óákveðinn. Kannaði í LibreChat hvort hægt sé að takmarka sýnileika módela við ákveðna user groups og hvort notendur geti tilheyrt fleiri en einum hópi.",
    "Bætti Claude Haiku 5.5 við model-stillingar MMS-tenantins, samhliða Sonnet og Opus 5.5. Þetta gerir Haiku 5.5 aðgengilegt í þeirri tenant-uppsetningu.",
    "Útbjó aggregation fyrir Matís til að laga custom agentinn í production, svo hægt sé að keyra uppfærsluna á Matís-gagnagrunninum. Skoðaði líka hvernig agentinn fær tools og hvort hann geti notað bæði innbyggða SearchNexNG-vefleit og Anthropic-vefleit.",
    "Setti upp og virkjaði Data retention fyrir Sjúkra með sex mánaða varðveislu samtalssögu. Staðfesti að breytingarnar væru komnar í main og lokaði worktree-inu.",
    "Uppfærði LibreChat-stillingu Hertz-tenantins í production þannig að hægt sé að hengja allt að 50 skjöl við eitt spjall. Þetta gerir Hertz-notendum kleift að senda fleiri skjöl í einu spjalli.",
    "Lagaði í LibreChat að inline-tenglar á skrár sem code interpreterinn býr til opnist sem 404 í stað þess að sækja viðhengi úr spjallinu; tenglar án samsvarandi viðhengis birtast nú sem venjulegur texti. Bætti við prófunum, backporta uppfærsluna í LibreChat 0.8.8 og byggði nýja LibreChat-mynd.",
];

#[test]
fn appendix_b_good_texts_pass_unchanged() {
    for text in GOOD_TEXTS {
        assert_eq!(validate(text).as_deref(), Ok(text), "{text}");
    }
}

#[test]
fn appendix_b_bad_texts_are_rejected_by_the_new_rules() {
    let bad = [
        "Vann að samráði við Coripharma um Code Interpreter á APRÓ AI platform. Engar upplýsingar liggja fyrir um að fundurinn hafi verið haldinn.",
        "Bætti Claude Sonnet five point five við Claude Desktop profile Sjúkra. Þannig geta notendur Sjúkra valið líkanið.",
        "Mældu aukinn biðtíma þar til Gemini 3.8 byrjar að svara. Sendi Hera tillögur að úrbótum í Slack.",
        "Rannsakaði hvers vegna code interpreterinn skilar skráarslóðum. Engin breyting eða staðfest lausn liggur fyrir í vísbendingunum.",
        "Yfirfór hvernig setja á upp shareable conversation links í LibreChat. Fylgdist með rolloutinu en vísbendingarnar staðfesta ekki hvort það lauk.",
    ];
    for text in bad {
        assert!(validate(text).is_err(), "{text}");
    }
}

#[test]
fn every_evidence_phrase_is_rejected_in_any_case() {
    for phrase in [
        "liggja fyrir",
        "liggur fyrir",
        "vísbending",
        "staðfesta ekki",
        "staðfestir ekki",
        "ekki staðfest",
        "óljóst hvort",
        "ekki ljóst hvort",
    ] {
        for variant in [phrase.to_string(), phrase.to_uppercase()] {
            let text = format!("Lagaði villu í production. Það {variant} enn.");
            let reason = unsafe_reason(&text).unwrap_or_default();
            assert!(reason.contains("vísbend"), "{variant}: {reason}");
        }
    }
}

#[test]
fn spelled_version_numbers_are_rejected_but_prose_numbers_pass() {
    assert!(unsafe_reason("Bætti við five point five í dag.").is_some());
    assert!(unsafe_reason("Bætti við point five í dag.").is_some());
    assert!(unsafe_reason("Bætti við Sonnet sex komma fimm í dag.").is_some());
    assert!(unsafe_reason("Bætti við komma sjö í dag.").is_some());
    assert!(unsafe_reason("Bætti við Þrír komma fimm í dag.").is_some());
    // no word boundary would reject these
    assert!(unsafe_reason("Bætti við five points í dag.").is_none());
    assert!(unsafe_reason("Bætti við gone point í dag.").is_none());
    // a number word without point/komma is ordinary prose
    assert!(unsafe_reason("Tók einn hóp og sex mánaða sögu.").is_none());
    assert!(unsafe_reason("Þrír hópar, komma í texta.").is_none());
}

#[test]
fn slack_is_rejected_as_a_whole_word_only() {
    assert!(unsafe_reason("Svaraði í Slack í dag.").is_some());
    assert!(unsafe_reason("Svaraði í slack í dag.").is_some());
    assert!(unsafe_reason("Svaraði í SLACK.").is_some());
    assert!(unsafe_reason("Lagaði slackers-lista í dag.").is_none());
}

#[test]
fn plural_first_verb_is_rejected_by_its_first_word_only() {
    assert!(unsafe_reason("Mældu biðtíma.").is_some());
    assert!(unsafe_reason("Mældu, svo skoðaði.").is_some());
    assert!(unsafe_reason("Fru skoðaði.").is_some()); // exactly 3 letters
    assert!(unsafe_reason("Fu skoðaði.").is_none()); // just under 3
    assert!(unsafe_reason("Mældi biðtíma og skoðaði hvort þeir mældu.").is_none());
    assert!(unsafe_reason("Úr-vinnsla skoðuð.").is_none()); // not alphabetic
    assert!(unsafe_reason("").is_none());
}

#[test]
fn ordinary_icelandic_words_that_are_customer_aliases_pass() {
    let text = "Festi kerfið í production og skoðaði sýni úr gagnagrunninum. Þetta gerir ferlið stöðugra.";
    assert_eq!(validate(text).as_deref(), Ok(text));
}
