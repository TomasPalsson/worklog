// Included into `daemon::tests::with_conn_cancel` via `include!`.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// A request whose client hung up while it queued for the connection
/// must not run once the lock frees: abandoned work piled up behind
/// the lock and kept every later request past the web's 10s timeout.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn skips_work_for_a_request_dropped_while_queued() {
    let state = state_with_block();
    let guard = state.conn.lock().await;
    let ran = Arc::new(AtomicBool::new(false));
    let ran2 = ran.clone();
    let req = tokio::spawn(with_conn(state.clone(), move |_| {
        ran2.store(true, Ordering::SeqCst);
        Ok(())
    }));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    req.abort(); // the client disconnects; hyper drops the handler future
    let _ = req.await;
    drop(guard);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let _ = state.conn.lock().await;
    assert!(!ran.load(Ordering::SeqCst), "abandoned request still ran");
}

/// A request nobody dropped still runs and returns its value.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runs_work_for_a_live_request() {
    let n = with_conn(state_with_block(), |c| {
        Ok(c.query_row("SELECT COUNT(*) FROM blocks", [], |r| r.get::<_, i64>(0))?)
    })
    .await
    .unwrap();
    assert_eq!(n, 1);
}
