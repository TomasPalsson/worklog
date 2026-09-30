#!/bin/bash
# CHK001 walkthrough against a throwaway daemon on 127.0.0.1:9433 (same calls the add-on makes).
B=http://127.0.0.1:9433
O="Origin: moz-extension://0b1f3c6e-test"
J="Content-Type: application/json"
hb() { curl -s -H "$O" -H "$J" -X POST "$B/browser/heartbeat" -d "{\"ts\":\"$1\",\"url\":\"https://github.com/TomasPalsson/worklog\",\"title\":\"worklog repo\",\"container\":null,\"incognito\":false}"; echo; }
now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
plus1=$(date -u -v+1M +%Y-%m-%dT%H:%M:%SZ)
echo "1) status before start:"; curl -s -H "$O" "$B/browser/status"; echo
echo "2) heartbeat before start (expect filtered outside_work_hours):"; hb "$now"
echo "3) preflight OPTIONS /browser/recording:"; curl -s -o /dev/null -D - -X OPTIONS -H "$O" -H "Access-Control-Request-Method: POST" -H "Access-Control-Request-Headers: content-type" "$B/browser/recording" | grep -i "^HTTP\|access-control"
echo "4) Start recording:"; curl -s -i -H "$O" -H "$J" -X POST "$B/browser/recording" -d '{"on":true}' | grep -i "^HTTP\|access-control\|{"
echo "5) ~2 minutes of browsing (3 heartbeats over 2 distinct minutes):"; hb "$now"; hb "$now"; hb "$plus1"
echo "6) status after browsing (expect minutes_today 2, recording_until tomorrow 17:00Z):"; curl -s -H "$O" "$B/browser/status"; echo
echo "7) no origin -> 403:"; curl -s -o /dev/null -w "%{http_code}\n" "$B/browser/status"
echo "8) evil origin -> 403:"; curl -s -o /dev/null -w "%{http_code}\n" -H "Origin: http://evil.test" -H "$J" -X POST "$B/browser/recording" -d '{"on":true}'
echo "9) Stop recording:"; curl -s -H "$O" -H "$J" -X POST "$B/browser/recording" -d '{"on":false}'; echo
echo "10) heartbeat after stop (expect filtered):"; hb "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "11) review site reachable at http://127.0.0.1:3333:"; curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:3333
