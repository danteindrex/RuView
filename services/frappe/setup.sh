#!/bin/bash
# RuView Care — Frappe/ERPNext setup
# Run once after docker compose up to initialize the Frappe site

set -e

SITE=${FRAPPE_SITE:-ruview.localhost}
ADMIN_PASS=${FRAPPE_ADMIN_PASSWORD:-Admin@2025}

echo "[1/5] Creating Frappe site..."
docker compose exec frappe bench new-site "$SITE" \
  --db-host db \
  --db-name "${DB_NAME:-ruview_frappe}" \
  --db-password "${DB_PASSWORD:-ruview_pass}" \
  --admin-password "$ADMIN_PASS" \
  --no-mariadb-socket

echo "[2/5] Installing ERPNext + Healthcare (Patient / Vital Signs DocTypes)..."
docker compose exec frappe bench get-app erpnext
docker compose exec frappe bench --site "$SITE" install-app erpnext
docker compose exec frappe bench get-app healthcare
docker compose exec frappe bench --site "$SITE" install-app healthcare

echo "[3/5] Installing ruview_care app..."
docker compose exec frappe bench --site "$SITE" install-app ruview_care

echo "[4/5] Running migrations..."
docker compose exec frappe bench --site "$SITE" migrate

echo "[5/5] Generating API key for FastAPI bridge..."
docker compose exec frappe bench --site "$SITE" execute \
  "frappe.utils.generate_hash" --args "[]"

echo "Done. Access Frappe at http://${SITE}:8080"
echo "Login: Administrator / $ADMIN_PASS"
