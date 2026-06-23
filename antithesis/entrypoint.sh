#!/bin/sh

set -eu

index_url="${UV_ANTITHESIS_INDEX_URL:-http://index:8000/simple}"

until python -c "import urllib.request; urllib.request.urlopen('${index_url}/antithesis-root/', timeout=1)"; do
  sleep 0.1
done

touch /tmp/uv-antithesis-ready

if [ "${UV_ANTITHESIS_EMIT_SETUP_COMPLETE:-1}" = "1" ]; then
  python - <<'PY'
from antithesis.lifecycle import setup_complete

setup_complete({"message": "uv and the package index are ready for testing"})
PY
fi

exec sleep infinity
