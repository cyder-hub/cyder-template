#!/usr/bin/env bash
set -euo pipefail

image="${1:-}"
if [[ -z "$image" ]]; then
  echo "usage: $0 <image>" >&2
  exit 2
fi

temporary_directory="$(mktemp -d)"
health_container="cyder-music-config-health-${GITHUB_RUN_ID:-local}-$$"

cleanup() {
  docker rm -f "$health_container" >/dev/null 2>&1 || true
  rm -rf "$temporary_directory"
}

fail() {
  echo "$1" >&2
  exit 1
}

trap cleanup EXIT

docker image inspect "$image" >/dev/null

layout="$(docker run --rm "$image" /bin/sh -c '
  test -x /app/bin/cyder-music
  test -f /app/front/dist/index.html
  test -d /data/config
  test -d /data/db
  test -d /data/storage
  test -d /data/logs
  test -d /tmp/cyder-music
  id -u
')"
if [[ "$layout" != "10001" ]]; then
  fail "container commands must run as UID 10001; found: $layout"
fi

default_summary="$(docker run --rm "$image" config check --format json)"
if [[ "$default_summary" != *'"data_dir":"/data"'* \
  || "$default_summary" != *'"database_kind":"sqlite"'* \
  || "$default_summary" != *'"database_pool_size":1'* \
  || "$default_summary" != *'"http_request_timeout_ms":30000'* \
  || "$default_summary" != *'"http_max_concurrent_requests":64'* \
  || "$default_summary" != *'"http_max_request_body_bytes":1048576'* ]]; then
  fail "default container configuration summary is invalid: $default_summary"
fi
if [[ "$default_summary" == *"database_url"* ]]; then
  fail "safe configuration summary exposed database_url"
fi

validation_data="$temporary_directory/validation-data"
mkdir -p "$validation_data"
chmod 0755 "$validation_data"
readonly_summary="$(docker run --rm \
  --volume "$validation_data:/data:ro" \
  "$image" config check --format json)"
if [[ "$readonly_summary" != *'"data_dir":"/data"'* ]]; then
  fail "read-only volume configuration check returned an invalid summary: $readonly_summary"
fi
if find "$validation_data" -mindepth 1 -print -quit | grep -q .; then
  fail "configuration check mutated its mounted data directory"
fi

secret="container-postgres-secret-marker"
postgres_summary="$(docker run --rm \
  --env "APP_DATABASE_URL=postgres://app:${secret}@database/app" \
  "$image" config check --format json)"
if [[ "$postgres_summary" != *'"database_kind":"postgres"'* \
  || "$postgres_summary" != *'"database_pool_size":5'* ]]; then
  fail "PostgreSQL container configuration summary is invalid: $postgres_summary"
fi
if [[ "$postgres_summary" == *"$secret"* ]]; then
  fail "safe PostgreSQL summary exposed a password"
fi

cat >"$temporary_directory/config.yaml" <<'YAML'
host: 0.0.0.0
port: 9010
database_acquire_timeout_ms: 12000
http_request_timeout_ms: 45000
http_max_concurrent_requests: 128
http_max_request_body_bytes: 2097152
YAML
mounted_summary="$(docker run --rm \
  --volume "$temporary_directory/config.yaml:/data/config/config.yaml:ro" \
  "$image" config check --format json)"
if [[ "$mounted_summary" != *'"config_file":{"kind":"default","path":"/data/config/config.yaml"}'* \
  || "$mounted_summary" != *'"host":"0.0.0.0"'* \
  || "$mounted_summary" != *'"port":9010'* \
  || "$mounted_summary" != *'"database_acquire_timeout_ms":12000'* \
  || "$mounted_summary" != *'"http_request_timeout_ms":45000'* \
  || "$mounted_summary" != *'"http_max_concurrent_requests":128'* \
  || "$mounted_summary" != *'"http_max_request_body_bytes":2097152'* ]]; then
  fail "mounted default configuration was not resolved: $mounted_summary"
fi

docker run --detach \
  --name "$health_container" \
  --health-interval=1s \
  --health-timeout=5s \
  --health-start-period=0s \
  --health-retries=10 \
  --volume "$temporary_directory/config.yaml:/data/config/config.yaml:ro" \
  "$image" >/dev/null

healthy=false
for _ in {1..30}; do
  health_status="$(docker inspect --format '{{.State.Health.Status}}' "$health_container")"
  if [[ "$health_status" = "healthy" ]]; then
    healthy=true
    break
  fi
  if [[ "$(docker inspect --format '{{.State.Running}}' "$health_container")" != "true" ]]; then
    break
  fi
  sleep 1
done
if [[ "$healthy" != "true" ]]; then
  docker inspect --format '{{json .State.Health}}' "$health_container" >&2 || true
  docker logs "$health_container" >&2 || true
  fail "container healthcheck did not use the YAML-resolved port"
fi

missing_stderr="$temporary_directory/missing.stderr"
if docker run --rm \
  --env APP_CONFIG_PATH=/data/config/missing.yaml \
  "$image" config check >"$temporary_directory/missing.stdout" 2>"$missing_stderr"; then
  fail "explicit missing container configuration unexpectedly passed"
fi
if ! grep -Fq "does not exist" "$missing_stderr"; then
  fail "explicit missing container configuration did not report a useful error"
fi

invalid_secret="invalid-url-secret-marker"
if docker run --rm \
  --env "APP_DATABASE_URL=mysql://${invalid_secret}@database/app" \
  "$image" config check >"$temporary_directory/invalid.stdout" 2>"$temporary_directory/invalid.stderr"; then
  fail "unsupported database URL unexpectedly passed"
fi
if grep -Fq "$invalid_secret" "$temporary_directory/invalid.stdout" \
  || grep -Fq "$invalid_secret" "$temporary_directory/invalid.stderr"; then
  fail "invalid database diagnostics exposed the original URL"
fi

unsupported_value="unsupported-environment-value-marker"
if docker run --rm \
  --env "APP_UNSUPPORTED_SETTING=${unsupported_value}" \
  "$image" config check >"$temporary_directory/unsupported.stdout" 2>"$temporary_directory/unsupported.stderr"; then
  fail "unsupported advanced APP_* environment variable unexpectedly passed config check"
fi
if ! grep -Fq "APP_UNSUPPORTED_SETTING" "$temporary_directory/unsupported.stderr"; then
  fail "unsupported advanced APP_* environment variable was not identified"
fi
if grep -Fq "$unsupported_value" "$temporary_directory/unsupported.stdout" \
  || grep -Fq "$unsupported_value" "$temporary_directory/unsupported.stderr"; then
  fail "unsupported environment diagnostics exposed the variable value"
fi

echo "container configuration contract passed"
