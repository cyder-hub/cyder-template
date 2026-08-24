#!/usr/bin/env bash
set -euo pipefail

image="${1:-}"
if [[ -z "$image" ]]; then
  echo "usage: $0 <image>" >&2
  exit 2
fi

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
container_name="cyder-template-e2e-${GITHUB_RUN_ID:-local}-$$"

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
docker run --detach \
  --name "$container_name" \
  --publish 127.0.0.1::8000 \
  "$image" >/dev/null

host_port="$(docker inspect \
  --format '{{(index (index .NetworkSettings.Ports "8000/tcp") 0).HostPort}}' \
  "$container_name")"
if [[ ! "$host_port" =~ ^[0-9]+$ ]]; then
  fail "container did not publish a valid HTTP port: ${host_port}"
fi

ready=false
for _ in {1..30}; do
  if curl --fail --silent --show-error \
    "http://127.0.0.1:${host_port}/readyz" >/dev/null 2>&1; then
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

if ! E2E_BASE_URL="http://127.0.0.1:${host_port}" \
  npm --prefix "$project_root/front" run test:e2e; then
  fail "container browser E2E contract failed"
fi

echo "container browser E2E contract passed"
