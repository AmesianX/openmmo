#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

work=$(mktemp -d)
cleanup() {
    if [[ -f "$work/nginx.pid" ]]; then
        nginx -s quit -p "$work" -c "$work/nginx.conf"
    fi
    rm -rf "$work"
}
trap cleanup EXIT
mkdir -p "$work/terrain/grass/r+00_+00" "$work/logs" "$work/cache"
printf 'terrain-fixture' > "$work/terrain/grass/r+00_+00/g_+0000_+0000.bin"
cargo run --quiet -p onlinerpg-terrain --bin terrain-snapshots -- "$work/terrain"
snapshot=$(find "$work/terrain/snapshots/full/0/0" -type f | head -1)
version=$(basename "$snapshot")

NGINX_LOCAL_RESOLVERS=127.0.0.1 envsubst '$NGINX_LOCAL_RESOLVERS' \
    < docker/nginx.conf.template > "$work/site.conf"
sed -i "s#/var/cache/nginx#$work/cache#g; s#/dev/stdout#$work/logs/access.log#g; s#listen 80;#listen unix:$work/nginx.sock;#; s#/terrain/snapshots#$work/terrain/snapshots#g" "$work/site.conf"
cat > "$work/nginx.conf" <<CONF
pid $work/nginx.pid;
error_log $work/logs/error.log;
events {}
http {
    access_log $work/logs/access.log;
    client_body_temp_path $work/body;
    proxy_temp_path $work/proxy;
    fastcgi_temp_path $work/fastcgi;
    uwsgi_temp_path $work/uwsgi;
    scgi_temp_path $work/scgi;
    include $work/site.conf;
}
CONF
nginx -t -p "$work" -c "$work/nginx.conf"
nginx -p "$work" -c "$work/nginx.conf"
url="http://localhost/api/terrain/snapshot/full/0/0/$version"
curl --unix-socket "$work/nginx.sock" -fsS -D "$work/headers" "$url" -o "$work/response"
cmp "$snapshot" "$work/response"
grep -qi 'Cache-Control: public, max-age=31536000, immutable' "$work/headers"
ground=$(find "$work/terrain/snapshots/ground/0/0" -type f | head -1)
curl --unix-socket "$work/nginx.sock" -fsS "http://localhost/api/terrain/snapshot/ground/0/0/$(basename "$ground")" -o "$work/ground-response"
cmp "$ground" "$work/ground-response"
etag=$(sed -n 's/^ETag: \(.*\)\r$/\1/p' "$work/headers")
test -n "$etag"
code=$(curl --unix-socket "$work/nginx.sock" -sS -o /dev/null -w '%{http_code}' -H "If-None-Match: $etag" "$url")
test "$code" = 304
missing=$(printf '%064d' 0)
code=$(curl --unix-socket "$work/nginx.sock" -sS -D "$work/missing-headers" -o /dev/null -w '%{http_code}' "http://localhost/api/terrain/snapshot/full/0/0/$missing")
test "$code" = 404
grep -qi 'Cache-Control: no-store' "$work/missing-headers"
printf 'updated-fixture' > "$work/terrain/grass/r+00_+00/g_+0000_+0000.bin"
cargo run --quiet -p onlinerpg-terrain --bin terrain-snapshots -- "$work/terrain"
test "$(find "$work/terrain/snapshots/full/0/0" -type f | wc -l)" = 2
curl --unix-socket "$work/nginx.sock" -fsS "$url" -o "$work/old-response"
cmp "$snapshot" "$work/old-response"
echo 'PASS nginx serves generated terrain files, revalidates them, and retains old versions without a game backend'
