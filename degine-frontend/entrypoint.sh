#!/bin/sh
set -eu

export BACKEND_URL="${BACKEND_URL:-http://degine-backend:3000}"
envsubst '${BACKEND_URL}' < /etc/nginx/nginx.conf.template > /etc/nginx/nginx.conf

cat > /usr/share/nginx/html/config.js << EOF
window.degine = {
  apiUrl: "${API_URL:-}",
  wsUrl: "${WS_URL:-}",
  supabaseUrl: "${SUPABASE_URL:-}",
  supabaseKey: "${SUPABASE_KEY:-}"
};
EOF

exec nginx -g 'daemon off;'
