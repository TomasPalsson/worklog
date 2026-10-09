//! Google Calendar 3LO login — loopback redirect + PKCE.
//!
//! Binds a one-shot listener on `127.0.0.1:0`, sends the Owner to
//! Google's consent page, catches the redirect, exchanges the code and
//! writes `google_token.json` in the shape `gcal::refresh_access_token`
//! reads. Synchronous like every collector; no new crates.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::{SecondsFormat, Utc};
use rand::RngCore;
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tracing::debug;

use super::gcal::{GcalAuth, StoredToken, SCOPE};
use crate::browser::OpenOutcome;

/// Google's consent endpoint. Tests swap in a fake via `AuthorizeOpts`.
pub const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
/// Stop waiting for the consent redirect after 300 s.
pub const CONSENT_TIMEOUT: Duration = Duration::from_secs(300);

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(5);

pub struct AuthorizeOpts {
    pub auth_url: String,
    pub timeout: Duration,
}

impl Default for AuthorizeOpts {
    fn default() -> Self {
        Self {
            auth_url: AUTH_URL.into(),
            timeout: CONSENT_TIMEOUT,
        }
    }
}

#[derive(Deserialize)]
struct CredentialsFile {
    installed: InstalledClient,
}

#[derive(Deserialize)]
struct InstalledClient {
    client_id: String,
    client_secret: String,
}

#[derive(Deserialize)]
struct ExchangeResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[derive(Default)]
struct Redirect {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// Production entry: real browser opener, writes progress to stderr.
pub fn authorize(auth: &GcalAuth, client: &Client) -> Result<PathBuf> {
    authorize_with(
        auth,
        client,
        &AuthorizeOpts::default(),
        &mut |url| crate::browser::open_url(url),
        &mut std::io::stderr(),
    )
}

/// Test seam. `open` receives the consent URL; `out` receives every
/// human-facing line (consent link, success, failure reason).
pub fn authorize_with<O, W>(
    auth: &GcalAuth,
    client: &Client,
    opts: &AuthorizeOpts,
    open: &mut O,
    out: &mut W,
) -> Result<PathBuf>
where
    O: FnMut(&str) -> Result<OpenOutcome>,
    W: Write,
{
    let result = run_login(auth, client, opts, open, out);
    match &result {
        Ok(path) => {
            let _ = writeln!(out, "Google Calendar connected ({}).", path.display());
        }
        Err(err) => {
            let _ = writeln!(out, "Google Calendar login failed: {err:#}");
        }
    }
    result
}

fn run_login<O, W>(
    auth: &GcalAuth,
    client: &Client,
    opts: &AuthorizeOpts,
    open: &mut O,
    out: &mut W,
) -> Result<PathBuf>
where
    O: FnMut(&str) -> Result<OpenOutcome>,
    W: Write,
{
    let credentials = read_credentials(&auth.credentials_path)?;
    let listener = bind_loopback()?;
    let redirect_uri = format!("http://{}", listener.local_addr()?);

    let verifier = random_token();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_token();
    let url = consent_url(
        &opts.auth_url,
        &credentials.client_id,
        &redirect_uri,
        &challenge,
        &state,
    );

    writeln!(out, "Open this link to connect Google Calendar:\n{url}")?;
    if let Err(err) = open(&url) {
        debug!(%err, "gcal auth: could not open browser");
    }

    let (code, mut stream) = wait_for_code(&listener, &state, opts.timeout)?;
    let exchanged = exchange_code(auth, client, &credentials, &code, &verifier, &redirect_uri)
        .and_then(|token| write_token(&auth.token_path, &token));
    match &exchanged {
        Ok(()) => respond(
            &mut stream,
            "Google Calendar connected. You can close this tab.",
        ),
        Err(_) => respond(&mut stream, "Login failed. See the terminal for details."),
    }
    exchanged.map(|()| auth.token_path.clone())
}

fn bind_loopback() -> Result<TcpListener> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .context("opening the local listener for the Google redirect")?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

fn read_credentials(path: &Path) -> Result<InstalledClient> {
    let hint = "download a Desktop app OAuth client from Google Cloud Console";
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading {} — {hint}", path.display()))?;
    let parsed: CredentialsFile = serde_json::from_str(&raw).with_context(|| {
        format!(
            "parsing {} (expected `installed.client_id` and `installed.client_secret`) — {hint}",
            path.display()
        )
    })?;
    Ok(parsed.installed)
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn consent_url(
    auth_url: &str,
    client_id: &str,
    redirect_uri: &str,
    challenge: &str,
    state: &str,
) -> String {
    let params = [
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", SCOPE),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("state", state),
    ];
    let query = params
        .iter()
        .map(|(key, value)| format!("{key}={}", urlencoding::encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{auth_url}?{query}")
}

/// Waits for the first request carrying `code`, `error` or `state`.
/// Anything else (favicon, probes) gets a 404 and is ignored.
fn wait_for_code(
    listener: &TcpListener,
    expected_state: &str,
    timeout: Duration,
) -> Result<(String, TcpStream)> {
    let deadline = Instant::now() + timeout;
    loop {
        let mut stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    bail!("timed out waiting for consent in the browser");
                }
                std::thread::sleep(POLL_INTERVAL);
                continue;
            }
            Err(err) => return Err(err).context("accepting the Google redirect"),
        };
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(REQUEST_READ_TIMEOUT))?;
        let Some(redirect) = read_redirect(&stream) else {
            write_response(&mut stream, "404 Not Found", "Not found");
            continue;
        };
        if redirect.state.as_deref() != Some(expected_state) {
            respond(
                &mut stream,
                "Possible forged redirect. Login stopped; nothing was saved.",
            );
            bail!("redirect state did not match — possible forged redirect, login stopped");
        }
        if let Some(error) = redirect.error {
            respond(&mut stream, "Login cancelled. You can close this tab.");
            if error == "access_denied" {
                bail!("login cancelled — consent was denied");
            }
            bail!("login cancelled — Google returned error `{error}`");
        }
        return match redirect.code {
            Some(code) => Ok((code, stream)),
            None => {
                respond(&mut stream, "Login failed: no authorization code received.");
                Err(anyhow!("Google redirect carried no authorization code"))
            }
        };
    }
}

/// Parses `GET /?code=…&state=…`; `None` when the request carries none of
/// `code`, `error`, `state`.
fn read_redirect(stream: &TcpStream) -> Option<Redirect> {
    let mut request_line = String::new();
    BufReader::new(stream.take(8192))
        .read_line(&mut request_line)
        .ok()?;
    let mut parts = request_line.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    let (_, query) = parts.next()?.split_once('?')?;
    let mut redirect = Redirect::default();
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = urlencoding::decode(&value.replace('+', " "))
            .ok()?
            .into_owned();
        match key {
            "code" => redirect.code = Some(value),
            "state" => redirect.state = Some(value),
            "error" => redirect.error = Some(value),
            _ => {}
        }
    }
    (redirect.code.is_some() || redirect.state.is_some() || redirect.error.is_some())
        .then_some(redirect)
}

fn respond(stream: &mut TcpStream, message: &str) {
    write_response(stream, "200 OK", message);
}

fn write_response(stream: &mut TcpStream, status: &str, message: &str) {
    let body = format!("<!doctype html><meta charset=utf-8><p>{message}</p>");
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}

fn exchange_code(
    auth: &GcalAuth,
    client: &Client,
    credentials: &InstalledClient,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<StoredToken> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", credentials.client_id.as_str()),
        ("client_secret", credentials.client_secret.as_str()),
        ("code_verifier", verifier),
    ];
    let response = client
        .post(&auth.oauth_base)
        .form(&form)
        .send()
        .context("POST to OAuth token endpoint")?;
    if !response.status().is_success() {
        let status = response.status();
        // Error bodies can echo credential values; log at debug only.
        let body = response.text().unwrap_or_else(|_| "<unreadable>".into());
        debug!(%status, %body, "gcal auth: token exchange error body");
        bail!("token exchange failed ({status})");
    }
    let parsed: ExchangeResponse = response.json().context("parsing token response")?;
    let refresh_token = parsed
        .refresh_token
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "Google returned no refresh token — remove worklog's access at \
                 https://myaccount.google.com/permissions and retry"
            )
        })?;
    let expiry = Utc::now() + chrono::Duration::seconds(parsed.expires_in.unwrap_or(3600));
    Ok(StoredToken {
        token: parsed.access_token,
        refresh_token,
        token_uri: auth.oauth_base.clone(),
        client_id: credentials.client_id.clone(),
        client_secret: credentials.client_secret.clone(),
        scopes: vec![SCOPE.to_string()],
        expiry: Some(expiry.to_rfc3339_opts(SecondsFormat::Secs, true)),
    })
}

/// Temp file in the same directory, then rename, so a failed write never
/// touches an existing token.
fn write_token(path: &Path, token: &StoredToken) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut temp_name = path.as_os_str().to_owned();
    temp_name.push(".tmp");
    let temp_path = PathBuf::from(temp_name);
    let json = serde_json::to_string_pretty(token).context("serialising token")?;
    // 0600 from creation — the file carries refresh_token + client_secret. A stale
    // temp keeps its old mode, so drop it first.
    let _ = std::fs::remove_file(&temp_path);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(&temp_path)
        .and_then(|mut file| file.write_all(json.as_bytes()))
        .with_context(|| format!("writing {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path).with_context(|| format!("replacing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;
    use std::sync::Mutex;
    use tempfile::{tempdir, TempDir};

    const CLIENT_ID: &str = "client-123.apps.googleusercontent.com";
    const CLIENT_SECRET: &str = "s3cret";
    const FIVE_SECONDS: Duration = Duration::from_secs(5);

    fn setup(server: &MockServer) -> (TempDir, GcalAuth) {
        let dir = tempdir().unwrap();
        let credentials_path = dir.path().join("google_credentials.json");
        std::fs::write(
            &credentials_path,
            json!({"installed": {"client_id": CLIENT_ID, "client_secret": CLIENT_SECRET}})
                .to_string(),
        )
        .unwrap();
        let auth = GcalAuth {
            token_path: dir.path().join("google_token.json"),
            credentials_path,
            calendars: vec!["primary".into()],
            api_base: server.url("/calendar/v3"),
            oauth_base: server.url("/token"),
        };
        (dir, auth)
    }

    fn query_param(url: &str, key: &str) -> String {
        let query = url.split_once('?').unwrap().1;
        query
            .split('&')
            .find_map(|pair| {
                let (k, v) = pair.split_once('=')?;
                (k == key).then(|| urlencoding::decode(v).unwrap().into_owned())
            })
            .unwrap_or_else(|| panic!("no {key} in {url}"))
    }

    struct Run {
        result: Result<PathBuf>,
        out: String,
        consent_url: String,
        browser_responses: Vec<String>,
    }

    /// Fake browser: on open, records the PKCE challenge and sends each of
    /// `requests` (with `{state}` substituted) to the redirect listener in order.
    fn run(
        auth: &GcalAuth,
        timeout: Duration,
        outcome: OpenOutcome,
        requests: &[&str],
        challenge_sink: &Mutex<String>,
    ) -> Run {
        let client = crate::http::client().unwrap();
        let opts = AuthorizeOpts {
            auth_url: "https://consent.test/auth".into(),
            timeout,
        };
        let mut consent_url = String::new();
        let mut handles = Vec::new();
        let mut out = Vec::new();
        let result = authorize_with(
            auth,
            &client,
            &opts,
            &mut |url| {
                consent_url = url.to_string();
                *challenge_sink.lock().unwrap() = query_param(url, "code_challenge");
                let redirect_uri = query_param(url, "redirect_uri");
                let address = redirect_uri.trim_start_matches("http://").to_string();
                let state = urlencoding::encode(&query_param(url, "state")).into_owned();
                let paths: Vec<String> = requests
                    .iter()
                    .map(|path| path.replace("{state}", &state))
                    .collect();
                handles.push(std::thread::spawn(move || {
                    paths
                        .into_iter()
                        .map(|path| {
                            let mut stream = TcpStream::connect(&address).unwrap();
                            write!(stream, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
                            let mut body = String::new();
                            stream.read_to_string(&mut body).unwrap();
                            body
                        })
                        .collect::<Vec<_>>()
                }));
                Ok(outcome)
            },
            &mut out,
        );
        let browser_responses = handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect();
        Run {
            result,
            out: String::from_utf8(out).unwrap(),
            consent_url,
            browser_responses,
        }
    }

    fn run_simple(
        auth: &GcalAuth,
        timeout: Duration,
        outcome: OpenOutcome,
        requests: &[&str],
    ) -> Run {
        run(auth, timeout, outcome, requests, &Mutex::new(String::new()))
    }

    fn token_response() -> serde_json::Value {
        json!({"access_token": "ya29.new", "refresh_token": "1//refresh", "expires_in": 3599})
    }

    fn mock_exchange(
        server: &MockServer,
        status: u16,
        body: serde_json::Value,
    ) -> httpmock::Mock<'_> {
        server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(status).json_body(body);
        })
    }

    fn write_existing(auth: &GcalAuth) -> Vec<u8> {
        let bytes = b"{\"old\":\"token\"}".to_vec();
        std::fs::write(&auth.token_path, &bytes).unwrap();
        bytes
    }

    #[test]
    fn happy_path_writes_token_the_refresh_routine_accepts() {
        let server = MockServer::start();
        let (_dir, auth) = setup(&server);
        write_existing(&auth);
        static CHALLENGE: Mutex<String> = Mutex::new(String::new());
        let exchange = server.mock(|when, then| {
            when.method(POST)
                .path("/token")
                .body_contains("grant_type=authorization_code")
                .body_contains("code=the-code")
                .body_contains(format!("client_id={}", urlencoding::encode(CLIENT_ID)))
                .body_contains("client_secret=s3cret")
                .matches(|req| {
                    // FR-05: the posted verifier must hash to the challenge in the consent URL.
                    let body = String::from_utf8(req.body.clone().unwrap()).unwrap();
                    let verifier = body
                        .split('&')
                        .find_map(|pair| pair.strip_prefix("code_verifier="))
                        .unwrap_or("");
                    let hashed = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
                    !verifier.is_empty() && hashed == *CHALLENGE.lock().unwrap()
                });
            then.status(200).json_body(token_response());
        });

        let run = run(
            &auth,
            FIVE_SECONDS,
            OpenOutcome::Spawned,
            &["/?code=the-code&state={state}"],
            &CHALLENGE,
        );

        exchange.assert_hits(1);
        assert_eq!(run.result.unwrap(), auth.token_path);
        assert!(run.out.contains(&run.consent_url), "link printed");
        assert!(run.out.contains("connected"));
        assert!(run.browser_responses[0].contains("connected"));

        // FR-08: exact shape; the refresh routine accepts it without a refresh call.
        let stored: StoredToken =
            serde_json::from_str(&std::fs::read_to_string(&auth.token_path).unwrap()).unwrap();
        assert_eq!(stored.token, "ya29.new");
        assert_eq!(stored.refresh_token, "1//refresh");
        assert_eq!(stored.token_uri, auth.oauth_base);
        assert_eq!(stored.client_id, CLIENT_ID);
        assert_eq!(stored.client_secret, CLIENT_SECRET);
        assert_eq!(stored.scopes, vec![SCOPE.to_string()]);
        assert!(stored.expiry.is_some());
        let client = crate::http::client().unwrap();
        assert_eq!(
            super::super::gcal::refresh_access_token(&auth, &client).unwrap(),
            "ya29.new"
        );
        exchange.assert_hits(1);

        assert_owner_only(&auth.token_path);
        assert!(!auth.token_path.with_extension("json.tmp").exists());
    }

    #[cfg(unix)]
    fn assert_owner_only(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        // FR-09. Catches: leaving the umask default (0644).
        let mode = std::fs::metadata(path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(not(unix))]
    fn assert_owner_only(_path: &Path) {}

    #[test]
    fn consent_url_asks_for_read_only_offline_access_on_loopback() {
        // FR-04: scope, offline, forced consent, S256, loopback redirect.
        let server = MockServer::start();
        let (_dir, auth) = setup(&server);
        mock_exchange(&server, 200, token_response());
        let run = run_simple(
            &auth,
            FIVE_SECONDS,
            OpenOutcome::Spawned,
            &["/?code=c&state={state}"],
        );
        let url = &run.consent_url;
        assert_eq!(query_param(url, "scope"), SCOPE);
        assert_eq!(query_param(url, "access_type"), "offline");
        assert_eq!(query_param(url, "prompt"), "consent");
        assert_eq!(query_param(url, "response_type"), "code");
        assert_eq!(query_param(url, "code_challenge_method"), "S256");
        assert_eq!(query_param(url, "client_id"), CLIENT_ID);
        assert!(query_param(url, "redirect_uri").starts_with("http://127.0.0.1:"));
    }

    #[test]
    fn unsupported_opener_still_prints_the_consent_link() {
        // Catches: printing the link only when the opener succeeds.
        let server = MockServer::start();
        let (_dir, auth) = setup(&server);
        mock_exchange(&server, 200, token_response());
        let run = run_simple(
            &auth,
            FIVE_SECONDS,
            OpenOutcome::Unsupported,
            &["/?code=c&state={state}"],
        );
        run.result.unwrap();
        assert!(run.out.contains("https://consent.test/auth?client_id="));
    }

    #[test]
    fn forged_state_stops_at_once_and_leaves_token_unchanged() {
        // Catches: ignoring state, or waiting for another redirect after a mismatch
        // (the 5 s timeout would fire instead and the error would say "timed out").
        let server = MockServer::start();
        let (_dir, auth) = setup(&server);
        let before = write_existing(&auth);
        let exchange = mock_exchange(&server, 200, token_response());
        let run = run_simple(
            &auth,
            FIVE_SECONDS,
            OpenOutcome::Spawned,
            &["/?code=c&state=forged"],
        );
        let err = format!("{:#}", run.result.unwrap_err());
        assert!(err.contains("forged"), "{err}");
        assert!(run.out.contains("forged"));
        assert!(run.browser_responses[0].contains("forged"));
        exchange.assert_hits(0);
        assert_eq!(std::fs::read(&auth.token_path).unwrap(), before);
    }

    #[test]
    fn code_without_state_is_rejected() {
        // Catches: treating a missing state as "nothing to compare".
        let server = MockServer::start();
        let (_dir, auth) = setup(&server);
        let run = run_simple(&auth, FIVE_SECONDS, OpenOutcome::Spawned, &["/?code=c"]);
        assert!(format!("{:#}", run.result.unwrap_err()).contains("forged"));
        assert!(!auth.token_path.exists());
    }

    #[test]
    fn denied_consent_reports_cancelled_in_browser_and_terminal() {
        // Catches: swallowing error=access_denied and waiting until timeout.
        let server = MockServer::start();
        let (_dir, auth) = setup(&server);
        let before = write_existing(&auth);
        let run = run_simple(
            &auth,
            FIVE_SECONDS,
            OpenOutcome::Spawned,
            &["/?error=access_denied&state={state}"],
        );
        let err = format!("{:#}", run.result.unwrap_err());
        assert!(err.contains("cancelled"), "{err}");
        assert!(run.out.contains("cancelled"));
        assert!(run.browser_responses[0].contains("cancelled"));
        assert_eq!(std::fs::read(&auth.token_path).unwrap(), before);
    }

    #[test]
    fn unrelated_requests_do_not_end_the_wait() {
        // Catches: ending the wait on the first connection (favicon, probe)
        // or on a lookalike key such as `xcode=`.
        let server = MockServer::start();
        let (_dir, auth) = setup(&server);
        mock_exchange(&server, 200, token_response());
        let run = run_simple(
            &auth,
            FIVE_SECONDS,
            OpenOutcome::Spawned,
            &["/favicon.ico", "/?xcode=1", "/?code=c&state={state}"],
        );
        run.result.unwrap();
        assert!(run.browser_responses[0].starts_with("HTTP/1.1 404"));
        assert!(run.browser_responses[1].starts_with("HTTP/1.1 404"));
        assert!(run.browser_responses[2].starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn consent_never_finished_times_out_and_leaves_token_unchanged() {
        // Catches: blocking forever / ignoring the injected timeout.
        let server = MockServer::start();
        let (_dir, auth) = setup(&server);
        let before = write_existing(&auth);
        let run = run_simple(&auth, Duration::from_millis(200), OpenOutcome::Spawned, &[]);
        let err = format!("{:#}", run.result.unwrap_err());
        assert!(err.contains("timed out"), "{err}");
        assert_eq!(std::fs::read(&auth.token_path).unwrap(), before);
    }

    #[test]
    fn listener_binds_loopback_only() {
        // Catches: binding 0.0.0.0 while the redirect URI still says 127.0.0.1.
        let listener = bind_loopback().unwrap();
        assert!(listener.local_addr().unwrap().ip().is_loopback());
    }

    #[test]
    fn default_opts_use_the_documented_constants() {
        let opts = AuthorizeOpts::default();
        assert_eq!(opts.auth_url, AUTH_URL);
        assert_eq!(opts.timeout, Duration::from_secs(300));
    }

    #[test]
    fn missing_credentials_names_the_path_and_writes_nothing() {
        // Catches: a generic "login failed" without the path.
        let server = MockServer::start();
        let (_dir, auth) = setup(&server);
        std::fs::remove_file(&auth.credentials_path).unwrap();
        let run = run_simple(&auth, FIVE_SECONDS, OpenOutcome::Spawned, &[]);
        let err = format!("{:#}", run.result.unwrap_err());
        assert!(
            err.contains(&auth.credentials_path.display().to_string()),
            "{err}"
        );
        assert!(err.contains("Google Cloud Console"), "{err}");
        assert!(!auth.token_path.exists());
    }

    #[test]
    fn unparsable_credentials_names_the_path() {
        // Catches: accepting a `web` client silently.
        let server = MockServer::start();
        let (_dir, auth) = setup(&server);
        std::fs::write(&auth.credentials_path, r#"{"web": {"client_id": "x"}}"#).unwrap();
        let run = run_simple(&auth, FIVE_SECONDS, OpenOutcome::Spawned, &[]);
        let err = format!("{:#}", run.result.unwrap_err());
        assert!(
            err.contains(&auth.credentials_path.display().to_string()),
            "{err}"
        );
    }

    #[test]
    fn token_endpoint_error_carries_status_and_leaves_token_unchanged() {
        // Catches: swallowing the error status; leaking the response body.
        let server = MockServer::start();
        let (_dir, auth) = setup(&server);
        let before = write_existing(&auth);
        server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(400).body("invalid_grant LEAKED-SECRET");
        });
        let run = run_simple(
            &auth,
            FIVE_SECONDS,
            OpenOutcome::Spawned,
            &["/?code=c&state={state}"],
        );
        let err = format!("{:#}", run.result.unwrap_err());
        assert!(err.contains("400"), "{err}");
        assert!(!err.contains("LEAKED-SECRET"));
        assert!(!run.out.contains("LEAKED-SECRET"));
        assert!(run.browser_responses[0].contains("failed"));
        assert_eq!(std::fs::read(&auth.token_path).unwrap(), before);
    }

    #[test]
    fn response_without_refresh_token_tells_owner_how_to_reset_consent() {
        // Catches: writing a token with a missing or empty refresh_token.
        for body in [
            json!({"access_token": "a", "expires_in": 3599}),
            json!({"access_token": "a", "refresh_token": "", "expires_in": 3599}),
        ] {
            let server = MockServer::start();
            let (_dir, auth) = setup(&server);
            let before = write_existing(&auth);
            mock_exchange(&server, 200, body);
            let run = run_simple(
                &auth,
                FIVE_SECONDS,
                OpenOutcome::Spawned,
                &["/?code=c&state={state}"],
            );
            let err = format!("{:#}", run.result.unwrap_err());
            assert!(err.contains("myaccount.google.com"), "{err}");
            assert_eq!(std::fs::read(&auth.token_path).unwrap(), before);
        }
    }
}
