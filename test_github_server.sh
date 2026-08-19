#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fixture_pid=""
container_name=""
smoke_root="$(mktemp -d)"
image_name="openspec-ui-github-smoke"

cleanup() {
  if [[ -n "$container_name" ]]; then
    docker rm -f "$container_name" >/dev/null 2>&1 || true
  fi
  if [[ -n "$fixture_pid" ]]; then
    kill "$fixture_pid" >/dev/null 2>&1 || true
    wait "$fixture_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$smoke_root"
}
trap cleanup EXIT

free_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
}

fixture_port="$(free_port)"
app_port="$(free_port)"
cache_path="$smoke_root/cache"
mkdir -p "$cache_path"
chmod 0777 "$cache_path"

python3 "$repo_root/tests/fixtures/github_api.py" --port "$fixture_port" &
fixture_pid=$!

openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$smoke_root/github-app.pem" >/dev/null 2>&1
printf '%s' 'fixture-webhook-secret' > "$smoke_root/github-webhook"
chmod 0644 "$smoke_root/github-app.pem" "$smoke_root/github-webhook"

sed \
  -e 's#"OWNER/REPOSITORY"#"ToruAI/openspec-ui"#' \
  -e 's#"pullRequestTargets": \["demo/main", "main"\]#"pullRequestTargets": ["demo/main"]#' \
  -e "s#\"cachePath\": \"/data/openspec-ui\"#\"cachePath\": \"/data/openspec-ui\",\n    \"apiBaseUrl\": \"http://host.docker.internal:$fixture_port\"#" \
  -e 's#"bindAddress": "127.0.0.1"#"bindAddress": "0.0.0.0"#' \
  "$repo_root/openspec-ui.github.example.json" > "$smoke_root/config.json"

docker build -t "$image_name" "$repo_root"

start_container() {
  container_name="openspec-ui-github-smoke-$RANDOM"
  docker run -d --name "$container_name" \
    --add-host=host.docker.internal:host-gateway \
    -p "127.0.0.1:$app_port:3000" \
    -e OPENSPEC_UI_CONFIG=/app/github-config.json \
    -e GITHUB_APP_ID=1 \
    -e GITHUB_APP_INSTALLATION_ID=2 \
    -e GITHUB_APP_PRIVATE_KEY_FILE=/run/secrets/github-app.pem \
    -e GITHUB_WEBHOOK_SECRET_FILE=/run/secrets/github-webhook \
    -v "$smoke_root/config.json:/app/github-config.json:ro" \
    -v "$cache_path:/data/openspec-ui" \
    -v "$smoke_root/github-app.pem:/run/secrets/github-app.pem:ro" \
    -v "$smoke_root/github-webhook:/run/secrets/github-webhook:ro" \
    "$image_name" >/dev/null
}

wait_for_healthy() {
  for _ in $(seq 1 90); do
    if curl -fsS "http://127.0.0.1:$app_port/api/sync-health" 2>/dev/null | \
      python3 -c 'import json,sys; h=json.load(sys.stdin); raise SystemExit(0 if h.get("state") == "healthy" and h.get("activeRevision") else 1)' 2>/dev/null; then
      return
    fi
    sleep 1
  done
  docker logs "$container_name"
  return 1
}

wait_for_state() {
  expected="$1"
  for _ in $(seq 1 30); do
    if curl -fsS "http://127.0.0.1:$app_port/api/sync-health" 2>/dev/null | \
      EXPECTED="$expected" python3 -c 'import json,os,sys; h=json.load(sys.stdin); raise SystemExit(0 if h.get("state") == os.environ["EXPECTED"] else 1)' 2>/dev/null; then
      return
    fi
    sleep 1
  done
  docker logs "$container_name"
  return 1
}

send_webhook() {
  delivery="$1"
  body='{"repository":{"full_name":"ToruAI/openspec-ui"},"ref":"refs/heads/demo/main"}'
  signature="$(BODY="$body" python3 -c 'import hashlib,hmac,os; print("sha256=" + hmac.new(b"fixture-webhook-secret", os.environ["BODY"].encode(), hashlib.sha256).hexdigest())')"
  curl -fsS \
    -H "Content-Type: application/json" \
    -H "X-GitHub-Event: push" \
    -H "X-GitHub-Delivery: $delivery" \
    -H "X-Hub-Signature-256: $signature" \
    --data "$body" \
    "http://127.0.0.1:$app_port/api/github/webhook" >/dev/null
}

start_container
wait_for_healthy

sse_output="$smoke_root/sse-output"
curl -sSN --max-time 15 "http://127.0.0.1:$app_port/api/events" > "$sse_output" &
sse_pid=$!
sleep 1
curl -fsS -X POST "http://127.0.0.1:$fixture_port/__fixture/version/2" >/dev/null
send_webhook "fixture-version-2"
for _ in $(seq 1 15); do
  if grep -Eq 'data: ?changed' "$sse_output"; then
    break
  fi
  sleep 1
done
grep -Eq 'data: ?changed' "$sse_output"
kill "$sse_pid" >/dev/null 2>&1 || true
wait "$sse_pid" >/dev/null 2>&1 || true

curl -fsS "http://127.0.0.1:$app_port/api/changes" | \
  python3 -c 'import json,sys; changes=json.load(sys.stdin)["changes"]; canonical=next(c for c in changes if c["name"] == "canonical-change"); assert canonical["archiveWarning"]["pullRequestNumber"] == 9'

curl -fsS -X POST "http://127.0.0.1:$fixture_port/__fixture/fail" >/dev/null
send_webhook "fixture-failure"
wait_for_state degraded
curl -fsS "http://127.0.0.1:$app_port/api/sync-health" | \
  python3 -c 'import json,sys; h=json.load(sys.stdin); assert h["servingLastKnownGood"] is True and h["activeRevision"]'

curl -fsS -X POST "http://127.0.0.1:$fixture_port/__fixture/recover" >/dev/null
send_webhook "fixture-recovery"
wait_for_healthy
first_revision="$(curl -fsS "http://127.0.0.1:$app_port/api/sync-health" | python3 -c 'import json,sys; print(json.load(sys.stdin)["activeRevision"])')"

docker rm -f "$container_name" >/dev/null
container_name=""
start_container
wait_for_healthy

curl -fsS "http://127.0.0.1:$app_port/api/sync-health" | \
  FIRST_REVISION="$first_revision" python3 -c 'import json,os,sys; h=json.load(sys.stdin); assert h["activeRevision"] == os.environ["FIRST_REVISION"]'
curl -fsS "http://127.0.0.1:$app_port/api/specs" | \
  python3 -c 'import json,sys; specs=json.load(sys.stdin)["specs"]; assert len(specs) == 1 and specs[0]["github"]["refName"] == "demo/main"'
curl -fsS "http://127.0.0.1:$app_port/api/changes" | \
  python3 -c 'import json,sys; changes=json.load(sys.stdin)["changes"]; assert any(c.get("github", {}).get("pullRequest", {}).get("number") == 12 for c in changes)'

status="$(curl -sS -o /dev/null -w '%{http_code}' \
  -H 'Content-Type: application/json' \
  -d '{"title":"must remain read only","description":"fixture"}' \
  "http://127.0.0.1:$app_port/api/ideas")"
test "$status" = "403"

echo "GitHub server container smoke test passed"
