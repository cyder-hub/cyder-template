#!/usr/bin/env bash
set -euo pipefail

image="${1:-}"
if [[ -z "$image" ]]; then
  echo "usage: $0 <image>" >&2
  exit 2
fi

temporary_directory="$(mktemp -d)"
container_name="cyder-template-http-${GITHUB_RUN_ID:-local}-$$"
runtime_warning_value="runtime-warning-value-marker"

cleanup() {
  docker rm -f "$container_name" >/dev/null 2>&1 || true
  rm -rf "$temporary_directory"
}

fail() {
  echo "$1" >&2
  docker logs "$container_name" 2>&1 || true
  exit 1
}

trap cleanup EXIT

header_value() {
  local headers="$1"
  local wanted="$2"
  awk -v wanted="$wanted" '
    BEGIN { IGNORECASE = 1 }
    index($0, ":") > 0 {
      name = substr($0, 1, index($0, ":") - 1)
      if (tolower(name) == tolower(wanted)) {
        value = substr($0, index($0, ":") + 1)
        sub(/^[[:space:]]+/, "", value)
      }
    }
    END { print value }
  ' <<<"$headers"
}

status_code() {
  awk 'toupper($1) ~ /^HTTP\// { status = $2 } END { print status }' <<<"$1"
}

request_headers() {
  docker exec "$container_name" curl \
    --silent \
    --show-error \
    --dump-header - \
    --output /dev/null \
    "$@" | tr -d '\r'
}

request_body() {
  docker exec "$container_name" curl --silent --show-error "$@"
}

docker image inspect "$image" >/dev/null

compressed_asset="$(docker run --rm "$image" /bin/sh -c \
  'find /app/front/dist/assets -type f -name "*.js.br" -print -quit')"
if [[ -z "$compressed_asset" ]]; then
  fail "image does not contain a precompressed JavaScript asset"
fi
plain_asset="${compressed_asset%.br}"
docker run --rm "$image" test -f "$plain_asset"
docker run --rm "$image" test -f "${plain_asset}.gz"
asset_path="/${plain_asset#/app/front/dist/}"

cat >"$temporary_directory/config.yaml" <<'YAML'
host: 0.0.0.0
port: 8000
http_request_timeout_ms: 30000
http_max_concurrent_requests: 64
http_max_request_body_bytes: 32
YAML

docker run --detach \
  --name "$container_name" \
  --env "APP_UNSUPPORTED_SETTING=${runtime_warning_value}" \
  --volume "$temporary_directory/config.yaml:/data/config/config.yaml:ro" \
  "$image" >/dev/null

ready=false
for _ in {1..30}; do
  if docker exec "$container_name" \
    curl -fsS http://127.0.0.1:8000/readyz >/dev/null 2>&1; then
    ready=true
    break
  fi
  if [[ "$(docker inspect --format '{{.State.Running}}' "$container_name")" != "true" ]]; then
    fail "container exited before becoming ready"
  fi
  sleep 1
done
if [[ "$ready" != "true" ]]; then
  fail "container did not become ready within 30 seconds"
fi

health_headers="$(request_headers \
  --header 'X-Request-ID: proxy-request_123' \
  http://127.0.0.1:8000/healthz)"
if [[ "$(status_code "$health_headers")" != "200" ]]; then
  fail "health endpoint did not return 200"
fi
if [[ "$(header_value "$health_headers" x-request-id)" != "proxy-request_123" ]]; then
  fail "valid inbound request ID was not preserved"
fi
if [[ "$(header_value "$health_headers" cache-control)" != "no-store" ]]; then
  fail "health endpoint does not use no-store"
fi
ready_headers="$(request_headers http://127.0.0.1:8000/readyz)"
if [[ "$(status_code "$ready_headers")" != "200" \
  || "$(header_value "$ready_headers" cache-control)" != "no-store" ]]; then
  fail "readiness endpoint does not use no-store"
fi
if [[ "$(header_value "$health_headers" content-security-policy)" != "default-src 'self'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'; form-action 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self' data:; connect-src 'self'" ]]; then
  fail "global content security policy is invalid"
fi
for expected_header in \
  'x-content-type-options:nosniff' \
  'x-frame-options:DENY' \
  'referrer-policy:strict-origin-when-cross-origin' \
  'permissions-policy:camera=(), microphone=(), geolocation=(), payment=(), usb=()' \
  'cross-origin-opener-policy:same-origin' \
  'cross-origin-resource-policy:same-origin'; do
  name="${expected_header%%:*}"
  expected="${expected_header#*:}"
  if [[ "$(header_value "$health_headers" "$name")" != "$expected" ]]; then
    fail "global response header ${name} is invalid"
  fi
done
if [[ -n "$(header_value "$health_headers" strict-transport-security)" ]]; then
  fail "application must not emit HSTS without owning TLS termination"
fi
if [[ -n "$(header_value "$health_headers" access-control-allow-origin)" ]]; then
  fail "same-origin application unexpectedly enabled CORS"
fi

invalid_id_headers="$(request_headers \
  --header 'X-Request-ID: contains spaces' \
  http://127.0.0.1:8000/healthz)"
replacement_id="$(header_value "$invalid_id_headers" x-request-id)"
if [[ ! "$replacement_id" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$ ]]; then
  fail "invalid inbound request ID was not replaced with a UUID v4: ${replacement_id}"
fi

api_headers="$(request_headers \
  --header 'X-Request-ID: api-contract-id' \
  http://127.0.0.1:8000/api/missing)"
if [[ "$(status_code "$api_headers")" != "404" \
  || "$(header_value "$api_headers" content-type)" != "application/json" \
  || "$(header_value "$api_headers" cache-control)" != "no-store" ]]; then
  fail "API 404 did not use the JSON/no-store boundary"
fi
api_body="$(request_body \
  --header 'X-Request-ID: api-contract-id' \
  http://127.0.0.1:8000/api/missing)"
for fragment in \
  '"error":"not_found"' \
  '"message":"API route was not found"' \
  '"request_id":"api-contract-id"'; do
  if [[ "$api_body" != *"$fragment"* ]]; then
    fail "API error body is missing ${fragment}: ${api_body}"
  fi
done

small_post_headers="$(request_headers \
  --request POST \
  --header 'Content-Type: application/json' \
  --data-binary '{}' \
  http://127.0.0.1:8000/api/missing)"
if [[ "$(status_code "$small_post_headers")" != "404" ]]; then
  fail "unsupported APP_* variable unexpectedly overrode the YAML body limit"
fi

large_post_headers="$(request_headers \
  --request POST \
  --header 'Content-Type: application/json' \
  --data-binary 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' \
  http://127.0.0.1:8000/api/missing)"
if [[ "$(status_code "$large_post_headers")" != "413" \
  || "$(header_value "$large_post_headers" content-type)" != "application/json" \
  || -n "$(header_value "$large_post_headers" retry-after)" ]]; then
  fail "request body boundary did not return JSON 413 without Retry-After"
fi

br_headers="$(request_headers \
  --header 'Accept-Encoding: br, gzip' \
  "http://127.0.0.1:8000${asset_path}")"
if [[ "$(status_code "$br_headers")" != "200" \
  || "$(header_value "$br_headers" content-encoding)" != "br" \
  || "$(header_value "$br_headers" vary)" != "accept-encoding" \
  || "$(header_value "$br_headers" cache-control)" != "public, max-age=31536000, immutable" ]]; then
  fail "Brotli static asset contract is invalid"
fi
etag="$(header_value "$br_headers" etag)"
if [[ -z "$etag" ]]; then
  fail "compressed static asset does not carry an ETag"
fi
if [[ "$etag" == W/* ]]; then
  fail "compressed static asset ETag must be strong"
fi

gzip_headers="$(request_headers \
  --header 'Accept-Encoding: gzip' \
  "http://127.0.0.1:8000${asset_path}")"
if [[ "$(status_code "$gzip_headers")" != "200" \
  || "$(header_value "$gzip_headers" content-encoding)" != "gzip" ]]; then
  fail "gzip static asset contract is invalid"
fi

conditional_headers="$(request_headers \
  --header 'Accept-Encoding: br' \
  --header "If-None-Match: ${etag}" \
  "http://127.0.0.1:8000${asset_path}")"
if [[ "$(status_code "$conditional_headers")" != "304" \
  || "$(header_value "$conditional_headers" cache-control)" != "public, max-age=31536000, immutable" ]]; then
  fail "conditional static asset request did not return immutable 304"
fi

history_headers="$(request_headers \
  --header 'Accept: text/html' \
  http://127.0.0.1:8000/dashboard)"
if [[ "$(status_code "$history_headers")" != "200" \
  || "$(header_value "$history_headers" cache-control)" != "no-cache" \
  || -z "$(header_value "$history_headers" etag)" ]]; then
  fail "SPA history fallback cache contract is invalid"
fi

static_missing_headers="$(request_headers http://127.0.0.1:8000/assets/missing.js)"
if [[ "$(status_code "$static_missing_headers")" != "404" \
  || "$(header_value "$static_missing_headers" content-type)" == "application/json" ]]; then
  fail "static 404 did not retain non-API semantics"
fi

request_body \
  --header 'X-Request-ID: access-log-contract' \
  'http://127.0.0.1:8000/healthz?query-secret-marker' >/dev/null
logs="$(docker logs "$container_name" 2>&1)"
for field in \
  'event="http_request_completed"' \
  'request_id=access-log-contract' \
  'method=GET' \
  'status=200' \
  'latency_ms=' \
  'response_bytes=' \
  'APP_UNSUPPORTED_SETTING'; do
  if [[ "$logs" != *"$field"* ]]; then
    fail "container logs are missing ${field}"
  fi
done
if [[ "$logs" == *"query-secret-marker"* || "$logs" == *"$runtime_warning_value"* ]]; then
  fail "container logs exposed a query string or ignored environment value"
fi

echo "container HTTP and static-asset contract passed"
