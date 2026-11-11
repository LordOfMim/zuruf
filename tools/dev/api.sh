#!/bin/bash
# Development aid: call the API of the daemon started by try-app.sh.
#   tools/dev/api.sh GET /state
#   tools/dev/api.sh POST /sessions '{"projectId":"…","name":"Test","mode":"isolated","agentAccess":false}'
method="$1"
path="$2"
body="${3:-}"
token="$(cat /tmp/zuruf-try/state/token)"
if [ -n "$body" ]; then
    curl -s -X "$method" "http://127.0.0.1:47700/api/v1$path" \
        -H "authorization: Bearer $token" -H "content-type: application/json" -d "$body"
else
    curl -s -X "$method" "http://127.0.0.1:47700/api/v1$path" -H "authorization: Bearer $token"
fi
echo
