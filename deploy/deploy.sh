#!/usr/bin/env bash
# Deploy API + snapshotter only to root@88.198.16.144 (web → Cloudflare Pages)
# Usage: ./deploy/deploy.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SERVER="${DEPLOY_HOST:-root@88.198.16.144}"
REMOTE_DIR="${REMOTE_DIR:-/opt/lumenlp}"
API_PORT="${API_PORT:-3301}"

echo "=== Sync source → ${SERVER}:${REMOTE_DIR} ==="
ssh "$SERVER" "mkdir -p ${REMOTE_DIR}/data ${REMOTE_DIR}/deploy ${REMOTE_DIR}/logs"
rsync -az --delete \
  --exclude target/ \
  --exclude node_modules/ \
  --exclude apps/web/node_modules/ \
  --exclude apps/web/.next/ \
  --exclude apps/web/out/ \
  --exclude data/ \
  --exclude .git/ \
  --exclude .superpowers/ \
  --exclude '*.db' \
  "$ROOT/" "$SERVER:${REMOTE_DIR}/"

echo "=== Upload systemd + nginx units ==="
scp "$ROOT/deploy/lumenlp-api.service" \
    "$ROOT/deploy/lumenlp-indexer.service" \
    "$ROOT/deploy/lumenlp-snapshotter.service" \
    "$ROOT/deploy/lumenlp-snapshotter.timer" \
    "$ROOT/deploy/nginx-lumenlp.conf" \
    "$SERVER:${REMOTE_DIR}/deploy/"

echo "=== Remote build + install (API + indexer) ==="
ssh "$SERVER" bash -s <<EOF
set -euo pipefail
if [ -f /root/.cargo/env ]; then
  source /root/.cargo/env
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found on remote host; install Rust toolchain first" >&2
  exit 1
fi
cd ${REMOTE_DIR}

echo "--- cargo release build ---"
cargo build -p api-server -p snapshotter -p pool-indexer --release

install -m 644 deploy/lumenlp-api.service /etc/systemd/system/lumenlp-api.service
install -m 644 deploy/lumenlp-indexer.service /etc/systemd/system/lumenlp-indexer.service
install -m 644 deploy/lumenlp-snapshotter.service /etc/systemd/system/lumenlp-snapshotter.service
install -m 644 deploy/lumenlp-snapshotter.timer /etc/systemd/system/lumenlp-snapshotter.timer
install -m 644 deploy/nginx-lumenlp.conf /etc/nginx/sites-available/lumenlp
ln -sfn /etc/nginx/sites-available/lumenlp /etc/nginx/sites-enabled/lumenlp
# The old lpagent site can still match api.lumenlp.xyz and override this vhost.
# Keep its available config for rollback, but remove only the enabled symlink.
rm -f /etc/nginx/sites-enabled/lpagent

# Stop VPS-hosted web if previously enabled
systemctl disable --now lpagent-web.service 2>/dev/null || true
rm -f /etc/systemd/system/lpagent-web.service
ufw delete allow 3300/tcp 2>/dev/null || true

ufw allow ${API_PORT}/tcp comment 'lumenlp-api' || true

systemctl stop lpagent-api.service 2>/dev/null || true
systemctl stop lpagent-indexer.service 2>/dev/null || true
systemctl stop lpagent-snapshotter.timer 2>/dev/null || true
systemctl stop lpagent-snapshotter.service 2>/dev/null || true

python3 - <<'PY'
import os
import sqlite3

src = "/opt/lumenlp/data/lumenlp.db"
dst = "/opt/lumenlp/data/pool-indexer.db"
# One-time migrate from the primary state database only when the indexer DB is absent.
# Never wipe an existing pool-indexer.db (destroys events / copy sessions).
if os.path.exists(src) and not os.path.exists(dst):
    src_con = sqlite3.connect(src)
    dst_con = sqlite3.connect(dst)
    src_con.backup(dst_con)
    dst_con.close()
    src_con.execute("PRAGMA wal_checkpoint(TRUNCATE);")
    src_con.close()
    print("migrated lumenlp.db -> pool-indexer.db")
elif os.path.exists(dst):
    print("keeping existing pool-indexer.db")
else:
    print("no sqlite DB present yet; services will create as needed")
PY

systemctl daemon-reload
systemctl enable --now lumenlp-api.service lumenlp-indexer.service lumenlp-snapshotter.timer
systemctl restart lumenlp-api.service
systemctl restart lumenlp-indexer.service
nginx -t && systemctl reload nginx

systemctl start lumenlp-snapshotter.service || true

sleep 2
systemctl --no-pager --full status lumenlp-api.service | head -12
systemctl --no-pager --full status lumenlp-indexer.service | head -12
curl -sf http://127.0.0.1:${API_PORT}/health && echo " api health ok"
EOF

echo ""
echo "=== API deployed ==="
echo "  API:  http://88.198.16.144:${API_PORT}/health"
echo "  Web:  deploy separately with ./deploy/deploy_site.sh (Cloudflare Pages)"
