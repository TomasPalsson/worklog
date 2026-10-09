//! Daemon side of the web "Connect Google Calendar" button: connection
//! flags for `GET /settings` and `POST /gcal/connect`, which runs the
//! existing 3LO login (`gcal_auth::authorize`, opens the host browser).

use std::sync::atomic::{AtomicBool, Ordering};

use axum::http::StatusCode;
use serde::Serialize;

use crate::collectors::gcal::GcalAuth;
use crate::collectors::gcal_auth;
use crate::daemon::ApiError;

static RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Serialize)]
pub struct GcalStatus {
    /// `google_token.json` exists — what the collector authenticates with.
    pub connected: bool,
    /// `google_credentials.json` (OAuth client id/secret) exists.
    pub has_client: bool,
}

pub fn status(auth: &GcalAuth) -> GcalStatus {
    GcalStatus {
        connected: auth.token_path.is_file(),
        has_client: auth.credentials_path.is_file(),
    }
}

pub async fn post_connect() -> Result<StatusCode, ApiError> {
    start(GcalAuth::from_paths()?)
}

fn start(auth: GcalAuth) -> Result<StatusCode, ApiError> {
    if !status(&auth).has_client {
        return Err(ApiError::bad_request(anyhow::anyhow!(
            "No Google OAuth client found at {}. Save google_credentials.json there and run `worklog collect gcal --auth` once in a terminal.",
            auth.credentials_path.display()
        )));
    }
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err(ApiError::Conflict(anyhow::anyhow!(
            "A Google sign-in is already in progress"
        )));
    }
    std::thread::spawn(move || {
        let result = crate::http::client().and_then(|c| gcal_auth::authorize(&auth, &c));
        if let Err(e) = result {
            tracing::warn!("gcal connect failed: {e:#}");
        }
        RUNNING.store(false, Ordering::SeqCst);
    });
    Ok(StatusCode::ACCEPTED)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth_in(dir: &std::path::Path) -> GcalAuth {
        GcalAuth {
            token_path: dir.join("google_token.json"),
            credentials_path: dir.join("google_credentials.json"),
            calendars: vec![],
            api_base: String::new(),
            oauth_base: String::new(),
        }
    }

    #[test]
    fn status_follows_the_files() {
        let dir = tempfile::tempdir().unwrap();
        let auth = auth_in(dir.path());
        let s = status(&auth);
        assert!(!s.connected && !s.has_client);
        std::fs::write(&auth.token_path, "{}").unwrap();
        std::fs::write(&auth.credentials_path, "{}").unwrap();
        let s = status(&auth);
        assert!(s.connected && s.has_client);
    }

    #[test]
    fn connect_without_credentials_is_400() {
        let dir = tempfile::tempdir().unwrap();
        let err = start(auth_in(dir.path())).unwrap_err();
        assert!(matches!(
            err,
            ApiError::BadRequest(ref e) if e.to_string().contains("worklog collect gcal --auth")
        ));
        assert!(!RUNNING.load(Ordering::SeqCst));
    }
}
