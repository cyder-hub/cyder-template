#!/usr/bin/env bash
set -euo pipefail

image="${1:-}"
if [[ -z "$image" ]]; then
  echo "usage: $0 <image>" >&2
  exit 2
fi

container_name="cyder-template-shutdown-${GITHUB_RUN_ID:-local}-$$"

cleanup() {
  docker rm -f "$container_name" >/dev/null 2>&1 || true
}

fail() {
  echo "$1" >&2
  docker logs "$container_name" 2>&1 || true
  exit 1
}

trap cleanup EXIT

docker image inspect "$image" >/dev/null
docker run --detach --name "$container_name" "$image" >/dev/null

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

if ! docker exec "$container_name" test -f /data/db/cyder-template.sqlite; then
  fail "default SQLite database was not created under /data/db"
fi

process_uid="$(docker exec "$container_name" /bin/sh -c "awk '/^Uid:/{print \$2}' /proc/1/status")"
if [[ "$process_uid" != "10001" ]]; then
  fail "service process must run as UID 10001; found ${process_uid}"
fi

SECONDS=0
docker stop --time 10 "$container_name" >/dev/null
elapsed_seconds="$SECONDS"

exit_code="$(docker inspect --format '{{.State.ExitCode}}' "$container_name")"
if [[ "$exit_code" != "0" ]]; then
  fail "container exited with status ${exit_code} after SIGTERM"
fi

if (( elapsed_seconds >= 10 )); then
  fail "container did not stop before Docker's 10-second deadline"
fi

logs="$(docker logs "$container_name" 2>&1)"
for event in \
  shutdown_signal_received \
  shutdown_readiness_disabled \
  shutdown_drain_started \
  shutdown_completed; do
  if ! grep -Fq "$event" <<<"$logs"; then
    fail "container logs are missing ${event}"
  fi
done

echo "container graceful shutdown passed in ${elapsed_seconds}s"
