# Worklog Firefox add-on

Reports focused, active, work-hours tab time to the local worklog daemon
(`http://127.0.0.1:9323/browser/heartbeat`) once a minute. Work hours are
enforced by the daemon, not the add-on. Private windows, the "Personal"
container, and paused state all suppress heartbeats before anything is sent.

## Load locally (unsigned)

1. Open `about:debugging#/runtime/this-firefox`.
2. Click "Load Temporary Add-on…".
3. Select `manifest.json` in this directory.

The add-on is removed on browser restart when loaded this way.

## Build

```bash
web-ext build --source-dir extension/firefox
```

## Sign for permanent install

```bash
web-ext sign --source-dir extension/firefox --channel unlisted \
  --api-key "$AMO_JWT_ISSUER" --api-secret "$AMO_JWT_SECRET"
```

Requires an addons.mozilla.org API key/secret pair.

## Popup

Click the toolbar icon to see whether the current tab is being counted, the
minutes recorded today and, when recording was forced on, the time left. The
single filled button adapts: Pause, Resume, Start recording or Stop recording.
"Start recording" overrides the work-hours filter until the end of the next
work day; the daemon enforces and expires it. "Open worklog" opens the review
site at `http://127.0.0.1:3333`. Pause is stored per browser and stops
heartbeats until resumed.
