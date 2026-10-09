#!/bin/bash
# CHK001 re-run: real recorder rows (SessionStart, first prompt) before the pin.
set -euo pipefail
export WORKLOG_HOME=/Users/tomas/.claude/jobs/590d208f/tmp/chk3
WL=/Users/tomas/.claude/jobs/590d208f/tmp/chk/bin/worklog
W=/Users/tomas/Desktop/Work/vitinn-infra/.claude/worktrees/fix-ci-ecr-pull
DB=$WORKLOG_HOME/worklog.db

# Session A: recorder SessionStart + start hint + first prompt (all "now").
echo "{\"session_id\":\"chk-a\",\"cwd\":\"$W\",\"hook_event_name\":\"SessionStart\",\"source\":\"startup\"}" | $WL hook-run
echo "{\"session_id\":\"chk-a\",\"cwd\":\"$W\",\"hook_event_name\":\"SessionStart\",\"source\":\"startup\"}" | $WL session-hint > $WORKLOG_HOME/hint-a.txt
echo "{\"session_id\":\"chk-a\",\"cwd\":\"$W\",\"hook_event_name\":\"UserPromptSubmit\",\"prompt\":\"work on the Sjúkra config\"}" | $WL hook-run

# Claude pins a minute later (the row `worklog pin Sjúkra --session chk-a` stores).
PIN_AT=$(python3 -c "import datetime;print((datetime.datetime.now(datetime.timezone.utc)+datetime.timedelta(seconds=60)).isoformat())")
sqlite3 $DB "insert into session_pins(session_id,customer,from_at,folder,branch,source) values('chk-a','Sjúkra','$PIN_AT','vitinn-infra','fix/code-interpreter-ecr-pull-through','claude');"

# Session B (after /clear): recorder SessionStart first, then the start hint (worst-case order).
echo "{\"session_id\":\"chk-b\",\"cwd\":\"$W\",\"hook_event_name\":\"SessionStart\",\"source\":\"clear\"}" | $WL hook-run
echo "{\"session_id\":\"chk-b\",\"cwd\":\"$W\",\"hook_event_name\":\"SessionStart\",\"source\":\"clear\"}" | $WL session-hint > $WORKLOG_HOME/hint-b.txt

# Later tool work for both sessions (synthetic, after the pins).
python3 - "$DB" "$W" <<'EOF'
import sqlite3,sys,datetime
db,W=sys.argv[1],sys.argv[2]
c=sqlite3.connect(db)
now=datetime.datetime.now(datetime.timezone.utc)
for sid,off in (("chk-a",2),("chk-b",40)):
    for step in range(15):
        ts=(now+datetime.timedelta(minutes=off+step*2)).isoformat()
        c.execute("insert into events(source,source_id,started_at,title,project_path,session_id) values(?,?,?,?,?,?)",
                  ("claude_turn",f"{sid}-t{step}",ts,"continue",W,sid))
c.commit()
EOF
DAY=$(python3 -c "import datetime;print(datetime.datetime.now(datetime.timezone.utc).date())")
$WL infer --day "$DAY"
sqlite3 -header $DB "select session_id,customer,source,from_at from session_pins;"
sqlite3 $DB "select id from blocks where day='$DAY';" > $WORKLOG_HOME/blocks.txt
echo "DAY=$DAY"
