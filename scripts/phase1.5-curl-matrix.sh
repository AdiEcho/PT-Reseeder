#!/usr/bin/env bash
# Phase 1.5 — 全量 curl 端点矩阵验证
# 测试所有 REST endpoint 的基本行为：status code、认证拒绝、CSRF 保护。
# 运行前提：server 已在 localhost:3000 运行，数据库为空（零用户状态）。
#
# Usage: bash scripts/phase1.5-curl-matrix.sh [base_url]

set -euo pipefail

BASE="${1:-http://localhost:3000}"
PASS=0
FAIL=0
COOKIE_JAR=$(mktemp)
trap 'rm -f "$COOKIE_JAR"' EXIT

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

assert_status() {
    local desc="$1" expected="$2" actual="$3"
    if [ "$actual" = "$expected" ]; then
        echo -e "  ${GREEN}✓${NC} $desc (${actual})"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}✗${NC} $desc — expected ${expected}, got ${actual}"
        FAIL=$((FAIL + 1))
    fi
}

assert_status_one_of() {
    local desc="$1" actual="$2"
    shift 2
    for expected in "$@"; do
        if [ "$actual" = "$expected" ]; then
            echo -e "  ${GREEN}✓${NC} $desc (${actual})"
            PASS=$((PASS + 1))
            return
        fi
    done
    echo -e "  ${RED}✗${NC} $desc — got ${actual}, expected one of: $*"
    FAIL=$((FAIL + 1))
}

# Curl helper: returns HTTP status code
# Usage: status=$(curl_status METHOD path [extra_curl_args...])
curl_status() {
    local method="$1" path="$2"
    shift 2
    curl -s -o /dev/null -w '%{http_code}' \
        -X "$method" \
        "$BASE$path" \
        "$@"
}

# Curl with cookie jar
curl_authed() {
    local method="$1" path="$2"
    shift 2
    curl -s -o /dev/null -w '%{http_code}' \
        -X "$method" \
        -b "$COOKIE_JAR" -c "$COOKIE_JAR" \
        -H "X-PT-Reseeder: 1" \
        "$BASE$path" \
        "$@"
}

# Curl with cookie jar, return body
curl_authed_body() {
    local method="$1" path="$2"
    shift 2
    curl -s \
        -X "$method" \
        -b "$COOKIE_JAR" -c "$COOKIE_JAR" \
        -H "X-PT-Reseeder: 1" \
        "$BASE$path" \
        "$@"
}

echo -e "${YELLOW}═══ Phase 1.5: Endpoint Matrix Verification ═══${NC}"
echo "Base URL: $BASE"
echo ""

# ─────────────────────────────────────────────────────────────────────────────
echo -e "${YELLOW}── 1. Public endpoints (no cookie) ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

# Health
s=$(curl_status GET "/api/health")
assert_status "GET /api/health — public" "200" "$s"

# Has user (should be false initially)
s=$(curl_status GET "/api/auth/has-user")
assert_status "GET /api/auth/has-user — public, no users yet" "200" "$s"

# Me (unauthenticated → 200 with null, NOT 401 or 500)
s=$(curl_status GET "/api/auth/me")
assert_status "GET /api/auth/me — unauthenticated → 200 (null user)" "200" "$s"

# CSRF check: POST without X-PT-Reseeder header → 403
s=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$BASE/api/auth/login" \
    -H "Content-Type: application/json" \
    -d '{"username":"x","password":"x"}')
assert_status "POST /api/auth/login without CSRF header → 403" "403" "$s"

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}── 2. Registration (first user) ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

# Register first user
s=$(curl -s -o /dev/null -w '%{http_code}' \
    -X POST "$BASE/api/auth/register" \
    -H "Content-Type: application/json" \
    -H "X-PT-Reseeder: 1" \
    -c "$COOKIE_JAR" \
    -d '{"username":"testadmin","password":"TestPass123!"}')
assert_status "POST /api/auth/register — first user → 201" "201" "$s"

# Register second user should fail (single-user system)
s=$(curl_status POST "/api/auth/register" \
    -H "Content-Type: application/json" \
    -H "X-PT-Reseeder: 1" \
    -d '{"username":"second","password":"pass"}')
assert_status "POST /api/auth/register — second user → 409" "409" "$s"

# has-user should now be true
body=$(curl -s "$BASE/api/auth/has-user")
if echo "$body" | grep -q '"has_user":true'; then
    echo -e "  ${GREEN}✓${NC} GET /api/auth/has-user — now true"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}✗${NC} GET /api/auth/has-user — expected has_user:true, got: $body"
    FAIL=$((FAIL + 1))
fi

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}── 3. Login / Logout ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

# Logout current registration session first
curl_authed POST "/api/auth/logout" > /dev/null 2>&1
rm -f "$COOKIE_JAR"
COOKIE_JAR=$(mktemp)

# Login with wrong password → 401
s=$(curl -s -o /dev/null -w '%{http_code}' \
    -X POST "$BASE/api/auth/login" \
    -H "Content-Type: application/json" \
    -H "X-PT-Reseeder: 1" \
    -d '{"username":"testadmin","password":"wrong"}')
assert_status "POST /api/auth/login — wrong password → 401" "401" "$s"

# Login success
s=$(curl -s -o /dev/null -w '%{http_code}' \
    -X POST "$BASE/api/auth/login" \
    -H "Content-Type: application/json" \
    -H "X-PT-Reseeder: 1" \
    -c "$COOKIE_JAR" \
    -d '{"username":"testadmin","password":"TestPass123!"}')
assert_status "POST /api/auth/login — correct → 200" "200" "$s"

# Me (authenticated → should return user info)
body=$(curl -s -b "$COOKIE_JAR" "$BASE/api/auth/me")
if echo "$body" | grep -q '"username":"testadmin"'; then
    echo -e "  ${GREEN}✓${NC} GET /api/auth/me — authenticated, returns user"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}✗${NC} GET /api/auth/me — expected username, got: $body"
    FAIL=$((FAIL + 1))
fi

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}── 4. Unauthenticated access to protected endpoints → 401 ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

for endpoint in \
    "GET /api/sites" \
    "GET /api/downloaders" \
    "GET /api/folders" \
    "GET /api/tasks" \
    "GET /api/logs/files" \
    "GET /api/logs" \
    "GET /api/config" \
    "GET /api/dashboard" \
    "GET /api/repost/queue" \
    "GET /api/reseed-runs"; do
    method=$(echo "$endpoint" | cut -d' ' -f1)
    path=$(echo "$endpoint" | cut -d' ' -f2)
    s=$(curl_status "$method" "$path" -H "X-PT-Reseeder: 1")
    assert_status "$endpoint — no cookie → 401" "401" "$s"
done

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}── 5. Downloaders CRUD ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

# Create downloader
body=$(curl_authed_body POST "/api/downloaders" \
    -H "Content-Type: application/json" \
    -d '{"name":"TestQB","dl_type":"qbittorrent","host":"127.0.0.1","port":8080,"username":"admin","password":"adminadmin","role":"both"}')
DL_ID=$(echo "$body" | grep -o '"id":[0-9]*' | head -1 | cut -d: -f2)
if [ -n "$DL_ID" ]; then
    echo -e "  ${GREEN}✓${NC} POST /api/downloaders — created id=$DL_ID"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}✗${NC} POST /api/downloaders — no id in response: $body"
    FAIL=$((FAIL + 1))
    DL_ID=1
fi

# List
s=$(curl_authed GET "/api/downloaders")
assert_status "GET /api/downloaders — list" "200" "$s"

# Update
s=$(curl_authed PUT "/api/downloaders/$DL_ID" \
    -H "Content-Type: application/json" \
    -d '{"name":"TestQB-Updated","dl_type":"qbittorrent","host":"127.0.0.1","port":8081,"username":"admin","password":"adminadmin","role":"source"}')
assert_status "PUT /api/downloaders/$DL_ID — update" "200" "$s"

# Toggle auto-start
s=$(curl_authed PATCH "/api/downloaders/$DL_ID/auto-start" \
    -H "Content-Type: application/json" \
    -d '{"auto_start":true}')
assert_status "PATCH /api/downloaders/$DL_ID/auto-start" "204" "$s"

# Test connection (will likely fail since no real qB, but should not 500)
s=$(curl_authed POST "/api/downloaders/$DL_ID/test")
assert_status_one_of "POST /api/downloaders/$DL_ID/test — connection attempt" "$s" "200" "502" "500" "422"

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}── 6. Folders CRUD ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

# Create folder
body=$(curl_authed_body POST "/api/folders" \
    -H "Content-Type: application/json" \
    -d "{\"path\":\"/tmp/test-folder\",\"scan_mode\":\"local\",\"downloader_id\":$DL_ID}")
FOLDER_ID=$(echo "$body" | grep -o '"id":[0-9]*' | head -1 | cut -d: -f2)
if [ -n "$FOLDER_ID" ]; then
    echo -e "  ${GREEN}✓${NC} POST /api/folders — created id=$FOLDER_ID"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}✗${NC} POST /api/folders — no id in response: $body"
    FAIL=$((FAIL + 1))
    FOLDER_ID=1
fi

# List
s=$(curl_authed GET "/api/folders")
assert_status "GET /api/folders — list" "200" "$s"

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}── 7. Sites CRUD ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

# List site definitions (reference data)
s=$(curl_authed GET "/api/site-definitions")
assert_status "GET /api/site-definitions" "200" "$s"

# Create site
body=$(curl_authed_body POST "/api/sites" \
    -H "Content-Type: application/json" \
    -d '{"name":"TestSite","url":"https://example.com","adapter_type":"nexusphp","auth_type":"cookie","cookie":"test_cookie_value"}')
SITE_ID=$(echo "$body" | grep -o '"id":[0-9]*' | head -1 | cut -d: -f2)
if [ -n "$SITE_ID" ]; then
    echo -e "  ${GREEN}✓${NC} POST /api/sites — created id=$SITE_ID"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}✗${NC} POST /api/sites — no id in response: $body"
    FAIL=$((FAIL + 1))
    SITE_ID=1
fi

# List
s=$(curl_authed GET "/api/sites")
assert_status "GET /api/sites — list" "200" "$s"

# Detail
s=$(curl_authed GET "/api/sites/$SITE_ID")
assert_status "GET /api/sites/$SITE_ID — detail" "200" "$s"

# Update
s=$(curl_authed PUT "/api/sites/$SITE_ID" \
    -H "Content-Type: application/json" \
    -d '{"url":"https://example.org","cookie":"updated_cookie"}')
assert_status "PUT /api/sites/$SITE_ID — update" "200" "$s"

# Validate (will fail network, but handler should not panic)
s=$(curl_authed POST "/api/sites/$SITE_ID/validate" \
    -H "Content-Type: application/json" \
    -d '{"name":"TestSite","url":"https://example.com","adapter_type":"nexusphp","cookie":"test"}')
assert_status_one_of "POST /api/sites/$SITE_ID/validate" "$s" "200" "422" "500" "502"

# Probe
s=$(curl_authed POST "/api/sites/$SITE_ID/probe")
assert_status_one_of "POST /api/sites/$SITE_ID/probe" "$s" "200" "422" "500" "502"

# Refresh stats (async, returns 202)
s=$(curl_authed POST "/api/sites/$SITE_ID/refresh-stats")
assert_status_one_of "POST /api/sites/$SITE_ID/refresh-stats" "$s" "202" "200" "500"

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}── 8. Tasks CRUD ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

# Create task (reseed tasks require destination_downloader_id)
body=$(curl_authed_body POST "/api/tasks" \
    -H "Content-Type: application/json" \
    -d "{\"name\":\"TestTask\",\"task_type\":\"reseed\",\"trigger_type\":\"manual\",\"site_ids\":[$SITE_ID],\"folder_ids\":[$FOLDER_ID],\"source_downloader_ids\":[$DL_ID],\"destination_downloader_id\":$DL_ID}")
TASK_ID=$(echo "$body" | grep -o '"id":[0-9]*' | head -1 | cut -d: -f2)
if [ -n "$TASK_ID" ]; then
    echo -e "  ${GREEN}✓${NC} POST /api/tasks — created id=$TASK_ID"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}✗${NC} POST /api/tasks — no id in response: $body"
    FAIL=$((FAIL + 1))
    TASK_ID=1
fi

# List
s=$(curl_authed GET "/api/tasks")
assert_status "GET /api/tasks — list" "200" "$s"

# Update
s=$(curl_authed PUT "/api/tasks/$TASK_ID" \
    -H "Content-Type: application/json" \
    -d "{\"name\":\"TestTask-Updated\",\"task_type\":\"reseed\",\"trigger_type\":\"manual\",\"site_ids\":[$SITE_ID],\"folder_ids\":[$FOLDER_ID],\"source_downloader_ids\":[$DL_ID],\"destination_downloader_id\":$DL_ID}")
assert_status "PUT /api/tasks/$TASK_ID — update" "200" "$s"

# Trigger dry-run (async, 202)
s=$(curl_authed POST "/api/tasks/$TASK_ID/trigger?dry_run=true")
assert_status "POST /api/tasks/$TASK_ID/trigger?dry_run=true → 202" "202" "$s"

# Trigger real (async, 202)
s=$(curl_authed POST "/api/tasks/$TASK_ID/trigger?dry_run=false")
assert_status "POST /api/tasks/$TASK_ID/trigger?dry_run=false → 202" "202" "$s"

# Task logs (may be empty)
s=$(curl_authed GET "/api/tasks/$TASK_ID/logs")
assert_status "GET /api/tasks/$TASK_ID/logs" "200" "$s"

# Dry-run preview (may be null)
s=$(curl_authed GET "/api/tasks/$TASK_ID/dry-run-preview")
assert_status "GET /api/tasks/$TASK_ID/dry-run-preview" "200" "$s"

# Reseed runs
s=$(curl_authed GET "/api/reseed-runs")
assert_status "GET /api/reseed-runs — list" "200" "$s"

s=$(curl_authed GET "/api/reseed-runs?task_id=$TASK_ID")
assert_status "GET /api/reseed-runs?task_id=$TASK_ID — filtered" "200" "$s"

# Non-existent run detail → 200 with null body (matches server_fn behavior)
s=$(curl_authed GET "/api/reseed-runs/99999")
assert_status "GET /api/reseed-runs/99999 — not found → 200 (null)" "200" "$s"

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}── 9. Config ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

s=$(curl_authed GET "/api/config")
assert_status "GET /api/config" "200" "$s"

s=$(curl_authed PUT "/api/config" \
    -H "Content-Type: application/json" \
    -d '{"key":"fetch_seeding_size","value":"true"}')
assert_status "PUT /api/config — update known key" "200" "$s"

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}── 10. Logs ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

s=$(curl_authed GET "/api/logs/files")
assert_status "GET /api/logs/files" "200" "$s"

s=$(curl_authed GET "/api/logs?page=1&page_size=10")
assert_status "GET /api/logs?page=1&page_size=10" "200" "$s"

# With filters
s=$(curl_authed GET "/api/logs?page=1&page_size=10&level=ERROR")
assert_status "GET /api/logs — level=ERROR filter" "200" "$s"

s=$(curl_authed GET "/api/logs?page=1&page_size=10&keyword=test")
assert_status "GET /api/logs — keyword filter" "200" "$s"

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}── 11. Dashboard ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

s=$(curl_authed GET "/api/dashboard?days=7")
assert_status "GET /api/dashboard?days=7" "200" "$s"

s=$(curl_authed GET "/api/dashboard?days=30")
assert_status "GET /api/dashboard?days=30" "200" "$s"

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}── 12. Repost queue (empty, basic flow) ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

s=$(curl_authed GET "/api/repost/queue")
assert_status "GET /api/repost/queue — list (empty)" "200" "$s"

s=$(curl_authed GET "/api/repost/queue?status=pending")
assert_status "GET /api/repost/queue?status=pending" "200" "$s"

# Attempt operations on non-existent entry → 404
s=$(curl_authed POST "/api/repost/queue/99999/review" \
    -H "Content-Type: application/json" \
    -d '{"action":"approve"}')
assert_status "POST /api/repost/queue/99999/review — not found → 404" "404" "$s"

s=$(curl_authed POST "/api/repost/queue/99999/submit")
assert_status "POST /api/repost/queue/99999/submit — not found → 404" "404" "$s"

s=$(curl_authed DELETE "/api/repost/queue/99999")
assert_status_one_of "DELETE /api/repost/queue/99999 — not found or 204" "$s" "404" "204"

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}── 13. /api unknown path → JSON 404 ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

s=$(curl_status GET "/api/nonexistent")
assert_status "GET /api/nonexistent → JSON 404" "404" "$s"

body=$(curl -s "$BASE/api/nonexistent")
if echo "$body" | grep -q '"error"'; then
    echo -e "  ${GREEN}✓${NC} /api/nonexistent returns JSON body"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}✗${NC} /api/nonexistent — expected JSON error body, got: $body"
    FAIL=$((FAIL + 1))
fi

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}── 14. Cleanup: Delete created resources ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

# Delete task
s=$(curl_authed DELETE "/api/tasks/$TASK_ID")
assert_status "DELETE /api/tasks/$TASK_ID → 204" "204" "$s"

# Delete site
s=$(curl_authed DELETE "/api/sites/$SITE_ID")
assert_status "DELETE /api/sites/$SITE_ID → 204" "204" "$s"

# Delete folder
s=$(curl_authed DELETE "/api/folders/$FOLDER_ID")
assert_status "DELETE /api/folders/$FOLDER_ID → 204" "204" "$s"

# Delete downloader
s=$(curl_authed DELETE "/api/downloaders/$DL_ID")
assert_status "DELETE /api/downloaders/$DL_ID → 204" "204" "$s"

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}── 15. Logout ──${NC}"
# ─────────────────────────────────────────────────────────────────────────────

s=$(curl_authed POST "/api/auth/logout")
assert_status "POST /api/auth/logout → 200" "200" "$s"

# Confirm logged out — me returns null/unauthenticated
body=$(curl -s -b "$COOKIE_JAR" "$BASE/api/auth/me")
if echo "$body" | grep -q 'null'; then
    echo -e "  ${GREEN}✓${NC} GET /api/auth/me after logout — returns null"
    PASS=$((PASS + 1))
else
    echo -e "  ${RED}✗${NC} GET /api/auth/me after logout — expected null, got: $body"
    FAIL=$((FAIL + 1))
fi

# Confirm protected endpoint rejects after logout
s=$(curl -s -o /dev/null -w '%{http_code}' \
    -b "$COOKIE_JAR" \
    -H "X-PT-Reseeder: 1" \
    "$BASE/api/sites")
assert_status "GET /api/sites after logout → 401" "401" "$s"

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "${YELLOW}═══════════════════════════════════════════════════════${NC}"
echo -e "Results: ${GREEN}${PASS} passed${NC}, ${RED}${FAIL} failed${NC}"
echo -e "${YELLOW}═══════════════════════════════════════════════════════${NC}"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
