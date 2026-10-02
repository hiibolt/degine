#!/bin/sh
set -eu

: "${DATABASE_URL:?DATABASE_URL is required}"
: "${SUPABASE_URL:?SUPABASE_URL is required}"

cat > /tmp/degine-config.toml << EOF
[server]
bind = "${BIND:-0.0.0.0:3000}"
database_url = """${DATABASE_URL}"""
database_ca = "${DATABASE_CA:-}"

[lean]
project_dir = "${LEAN_PROJECT:-/app/lean-project}"
helper_path = "${LEAN_HELPER:-/app/lean-project/.lake/build/bin/extractor}"
timeout_secs = ${LEAN_TIMEOUT:-180}

[auth]
supabase_url = "${SUPABASE_URL}"
EOF

exec /app/degine-backend /tmp/degine-config.toml
