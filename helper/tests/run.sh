#!/usr/bin/env bash
# Runs the helper's tests against a throwaway copy of the site: its own
# MariaDB and PHP's built-in server in a temp folder, all removed afterwards.
# (The same setup as feets' tests.)
#
#   helper/tests/run.sh
#
# Needs: php (with pdo_mysql), mariadb (mariadbd, mariadb-install-db), node 22+,
# and cargo for the Writer sync test at the end (skipped without it).
# On a Mac: brew install php mariadb node.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# mktemp's folder, not the repo: MariaDB's socket path must be short.
TMP="$(mktemp -d)"
DB_PORT=$((34000 + RANDOM % 1000))
WEB_PORT=$((8900 + RANDOM % 1000))
cleanup() {
  [ -n "${PHP_PID:-}" ] && kill "$PHP_PID" 2>/dev/null || true
  [ -n "${DB_PID:-}" ] && kill "$DB_PID" 2>/dev/null && wait "$DB_PID" 2>/dev/null || true
  rm -rf "$TMP"
}
trap cleanup EXIT

echo "Starting MariaDB on port ${DB_PORT}…"
mariadb-install-db --datadir="$TMP/data" --auth-root-authentication-method=normal --skip-test-db >"$TMP/db-install.log" 2>&1
mariadbd --datadir="$TMP/data" --socket="$TMP/s" --port="$DB_PORT" --bind-address=127.0.0.1 >"$TMP/db.log" 2>&1 &
DB_PID=$!
for _ in $(seq 50); do
  mariadb --socket="$TMP/s" -uroot -e 'SELECT 1' >/dev/null 2>&1 && break
  sleep 0.2
done
mariadb --socket="$TMP/s" -uroot -e "CREATE DATABASE writer; CREATE USER 'writer'@'127.0.0.1' IDENTIFIED BY 'test';
  GRANT ALL ON writer.* TO 'writer'@'127.0.0.1';"

# The site: a copy of public/, with its config one folder up as on a host.
mkdir -p "$TMP/site"
cp -R "$ROOT/public" "$TMP/site/public"
cat >"$TMP/site/writer-config.php" <<CONFIG
<?php
return [
    'app_password' => 'test-setup-password',
    'mysql' => ['host' => '127.0.0.1', 'port' => $DB_PORT, 'user' => 'writer', 'password' => 'test', 'database' => 'writer'],
];
CONFIG

echo "Starting PHP on port ${WEB_PORT}…"
php -S "127.0.0.1:$WEB_PORT" -t "$TMP/site/public" >"$TMP/php.log" 2>&1 &
PHP_PID=$!
for _ in $(seq 50); do
  curl -s "http://127.0.0.1:$WEB_PORT/api.php?action=me" >/dev/null 2>&1 && break
  sleep 0.1
done

export HELPER_URL="http://127.0.0.1:$WEB_PORT/"
export HELPER_SETUP_PASSWORD='test-setup-password'
export HELPER_DB="mariadb --socket=$TMP/s -uroot writer"
status=0
node --test --test-concurrency=1 "$ROOT"/tests/*.test.mjs || status=$?

# Then Sloth Writer's own sync code against this helper, with a token made
# the way the helper's page makes one (signed in as the tests' account).
if [ $status -eq 0 ] && command -v cargo >/dev/null; then
  echo; echo "Syncing Sloth Writer with it…"
  curl -s -c "$TMP/cookies" -H 'Content-Type: application/json' \
    -d '{"username":"writer","password":"writer-password-1"}' "${HELPER_URL}api.php?action=login" >/dev/null
  TOKEN=$(curl -s -b "$TMP/cookies" -H 'Content-Type: application/json' -d '{"label":"cargo test"}' \
    "${HELPER_URL}api.php?action=token_create" | sed -E 's/.*"token":"([0-9a-f]+)".*/\1/')
  (cd "$ROOT/.." && HELPER_TOKEN="$TOKEN" cargo test -q -p sloth-writer real_helper -- --ignored) || status=$?
fi
if [ $status -ne 0 ]; then
  echo; echo "--- php.log (last 30 lines) ---"; tail -30 "$TMP/php.log"
fi
exit $status
