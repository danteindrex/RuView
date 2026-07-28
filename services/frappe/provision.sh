#!/bin/bash
# Self-provisioning entrypoint for the Frappe/ERPNext container.
#
# Replaces the old manual setup.sh: on FIRST boot it initialises a bench,
# creates the site, installs ERPNext + Healthcare (for the Patient / Vital
# Signs DocTypes we reuse) + our wave_care app, then starts the dev server.
# On later boots it skips straight to `bench start`. Idempotent.
#
# NOTE: first-boot provisioning git-clones the Frappe framework, ERPNext and
# Healthcare and builds JS assets — expect ~20-30 min and a stable network.
set -e

SITE="${FRAPPE_SITE:-wave.localhost}"
ADMIN_PASS="${FRAPPE_ADMIN_PASSWORD:-Admin@2025}"
DB_ROOT_PASS="${DB_ROOT_PASSWORD:-wave_root}"
BENCH_DIR=/home/frappe/frappe-bench

cd /home/frappe

if [ ! -d "$BENCH_DIR/apps/frappe" ]; then
  echo "[provision] Initialising bench (frappe version-15)..."
  # Frappe v15 supports Python 3.10/3.11; the image defaults to 3.14 (pyenv)
  # which breaks the dependency install, so pin to the system python3.11.
  # Init directly into $BENCH_DIR (it must not pre-exist — no named volume is
  # mounted here, so the frappe user owns the path and bench can create it).
  bench init --frappe-branch version-15 --python python3.11 \
    --skip-redis-config-generation "$BENCH_DIR"
fi

cd "$BENCH_DIR"

# Repair sites/apps.txt (one app per line). A prior run's bad append can
# concatenate entries (e.g. "erpnextwave_care"), which wedges every bench
# command with ModuleNotFoundError. Rebuild it cleanly from present apps.
if [ -d "$BENCH_DIR/apps/frappe" ]; then
  {
    echo frappe
    for a in erpnext healthcare wave_care; do
      [ -d "$BENCH_DIR/apps/$a" ] && echo "$a"
    done
  } > "$BENCH_DIR/sites/apps.txt"
fi

# Point the bench at the compose service hosts.
bench set-config -g db_host "${DB_HOST:-db}"
bench set-config -g redis_cache "redis://${REDIS_CACHE:-redis-cache:6379}"
bench set-config -g redis_queue "redis://${REDIS_QUEUE:-redis-queue:6379}"
bench set-config -g redis_socketio "redis://${REDIS_SOCKETIO:-redis-socketio:6379}"

if [ ! -d "sites/$SITE" ]; then
  echo "[provision] Creating site $SITE..."
  bench new-site "$SITE" \
    --db-host "${DB_HOST:-db}" \
    --mariadb-root-password "$DB_ROOT_PASS" \
    --admin-password "$ADMIN_PASS" \
    --no-mariadb-socket
fi

# Fetch an app (pinned branch) only if its source is not already present.
get_app_once() {  # $1=app  $2=branch(optional)
  local app="$1" branch="$2"
  if [ ! -d "$BENCH_DIR/apps/$app" ]; then
    if [ -n "$branch" ]; then bench get-app --branch "$branch" "$app"
    else bench get-app "$app"; fi
  fi
}

# Install an app into the site only if not already installed (idempotent —
# lets a partial/failed provision resume without redoing completed apps).
install_app_once() {  # $1=app
  local app="$1"
  if bench --site "$SITE" list-apps 2>/dev/null | grep -qxF "$app"; then
    echo "[provision] $app already installed — skipping."
  else
    bench --site "$SITE" install-app "$app"
  fi
}

echo "[provision] Installing ERPNext + Healthcare (branch version-15)..."
get_app_once erpnext version-15
install_app_once erpnext
# Healthcare's default (develop) branch targets frappe v17 and refuses to
# install on v15. If a previously-cloned healthcare is on the wrong branch,
# drop it so the version-15 branch is fetched instead.
if [ -d "$BENCH_DIR/apps/healthcare" ]; then
  hc_branch=$(git -C "$BENCH_DIR/apps/healthcare" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)
  if [ "$hc_branch" != "version-15" ]; then
    echo "[provision] Removing healthcare branch '$hc_branch' (need version-15)..."
    rm -rf "$BENCH_DIR/apps/healthcare"
  fi
fi
get_app_once healthcare version-15
install_app_once healthcare

# wave_care is bind-mounted at apps-src/wave_care; copy it into the bench and install.
if [ -d /home/frappe/apps-src/wave_care ]; then
  if [ ! -d "$BENCH_DIR/apps/wave_care" ]; then
    echo "[provision] Staging wave_care app..."
    cp -r /home/frappe/apps-src/wave_care "$BENCH_DIR/apps/wave_care"
    ./env/bin/pip install -e "$BENCH_DIR/apps/wave_care"
  else
    # Keep the bench copy in sync with the mounted source so repo fixes
    # (e.g. fixture corrections) propagate on the next boot.
    cp -r /home/frappe/apps-src/wave_care/. "$BENCH_DIR/apps/wave_care/"
  fi
  # Register in apps.txt with a proper newline (never bare `echo >>` which
  # can concatenate onto a non-newline-terminated last line).
  grep -qxF wave_care "$BENCH_DIR/sites/apps.txt" || printf '\nwave_care\n' >> "$BENCH_DIR/sites/apps.txt"
  # Normalise: drop blank lines / dupes that the append may introduce.
  awk 'NF && !seen[$0]++' "$BENCH_DIR/sites/apps.txt" > "$BENCH_DIR/sites/apps.txt.tmp" \
    && mv "$BENCH_DIR/sites/apps.txt.tmp" "$BENCH_DIR/sites/apps.txt"
fi
if [ -d "$BENCH_DIR/apps/wave_care" ]; then
  if bench --site "$SITE" list-apps 2>/dev/null | grep -qxF wave_care; then
    echo "[provision] wave_care already installed — skipping."
  else
    # A prior crashed install may have left an orphan "Wave Care" Module Def,
    # which makes a fresh install-app fail with DuplicateEntryError. Purge any
    # partial state first, then install clean.
    bench --site "$SITE" uninstall-app wave_care --yes --no-backup --force >/dev/null 2>&1 || true
    bench --site "$SITE" execute frappe.client.delete \
      --kwargs '{"doctype":"Module Def","name":"Wave Care"}' >/dev/null 2>&1 || true
    bench --site "$SITE" install-app wave_care
  fi
fi

bench --site "$SITE" set-config developer_mode 1
bench --site "$SITE" clear-cache
echo "[provision] Site $SITE ready. Admin login: Administrator / $ADMIN_PASS"

bench use "$SITE"
echo "[provision] Starting bench (http :8000 inside container -> :4080 on host)..."
exec bench start
