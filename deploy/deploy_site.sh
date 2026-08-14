#!/usr/bin/env bash
# Deploy static Next.js export to Cloudflare Pages
# Usage:
#   NEXT_PUBLIC_API_BASE=https://api.lumenlp.xyz ./deploy/deploy_site.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/apps/web"

API_BASE="${NEXT_PUBLIC_API_BASE:?Set NEXT_PUBLIC_API_BASE, e.g. https://api.lumenlp.xyz}"
PROJECT="${CF_PAGES_PROJECT:-lumenlp}"

echo "=== Building static export (API=${API_BASE}) ==="
npm ci
NEXT_PUBLIC_API_BASE="$API_BASE" npm run build

echo "=== Deploying to Cloudflare Pages project: ${PROJECT} ==="
npx wrangler pages deploy out --project-name="$PROJECT" --commit-dirty=true

echo "=== Done ==="
echo "  Site: https://${PROJECT}.pages.dev"
echo "  Attach custom domain lumenlp.xyz in CF Pages → Custom domains"
echo "  API:  ${API_BASE}"
