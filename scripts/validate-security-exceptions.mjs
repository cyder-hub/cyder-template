import { readFileSync } from "node:fs";

const MAX_EXCEPTION_DAYS = 90;
const DAY_MS = 24 * 60 * 60 * 1000;
const configPath = process.argv[2] ?? new URL("../deny.toml", import.meta.url);
const config = readFileSync(configPath, "utf8");
const lines = config.split("\n");

const sectionStart = lines.findIndex((line) => line.trim() === "[advisories]");
if (sectionStart === -1) {
  throw new Error("deny.toml is missing an [advisories] section");
}
const nextSectionOffset = lines
  .slice(sectionStart + 1)
  .findIndex((line) => /^\[[^\]]+\]$/.test(line.trim()));
const sectionEnd =
  nextSectionOffset === -1 ? lines.length : sectionStart + 1 + nextSectionOffset;
const advisories = lines.slice(sectionStart + 1, sectionEnd);

const ignoreStart = advisories.findIndex((line) => /^ignore\s*=\s*\[\s*$/.test(line.trim()));
const ignoreEnd = advisories
  .slice(ignoreStart + 1)
  .findIndex((line) => line.trim() === "]");
if (ignoreStart === -1 || ignoreEnd === -1) {
  throw new Error("deny.toml must define [advisories].ignore as a multiline array");
}
const ignoreLines = advisories.slice(ignoreStart + 1, ignoreStart + 1 + ignoreEnd);

const entryPattern = /^\{\s*(id|crate)\s*=\s*"([^"]+)",\s*reason\s*=\s*"owner=(@[A-Za-z0-9][A-Za-z0-9-]*(?:\/[A-Za-z0-9][A-Za-z0-9_.-]*)?); expires=(\d{4}-\d{2}-\d{2}); justification=([^";][^"]*)"\s*\},$/;
const today = new Date();
const todayUtc = Date.UTC(
  today.getUTCFullYear(),
  today.getUTCMonth(),
  today.getUTCDate(),
);
const latestAllowed = todayUtc + MAX_EXCEPTION_DAYS * DAY_MS;
const seen = new Set();
let activeExceptions = 0;

for (const rawLine of ignoreLines) {
  const line = rawLine.trim();
  if (line === "" || line.startsWith("#")) {
    continue;
  }

  const match = line.match(entryPattern);
  if (!match) {
    throw new Error(
      "Every advisory exception must be a one-line inline table with " +
        'owner=@user-or-team; expires=YYYY-MM-DD; justification=... and a trailing comma: ' +
        line,
    );
  }

  const [, kind, advisory, owner, expires, justification] = match;
  if (kind === "id" && !/^RUSTSEC-\d{4}-\d{4}$/.test(advisory)) {
    throw new Error(`Invalid RustSec advisory id: ${advisory}`);
  }
  if (seen.has(`${kind}:${advisory}`)) {
    throw new Error(`Duplicate security exception: ${advisory}`);
  }
  seen.add(`${kind}:${advisory}`);

  const expiresUtc = Date.parse(`${expires}T00:00:00Z`);
  if (!Number.isFinite(expiresUtc) || new Date(expiresUtc).toISOString().slice(0, 10) !== expires) {
    throw new Error(`Invalid expiry date for ${advisory}: ${expires}`);
  }
  if (expiresUtc <= todayUtc) {
    throw new Error(`Security exception ${advisory} owned by ${owner} expired on ${expires}`);
  }
  if (expiresUtc > latestAllowed) {
    throw new Error(
      `Security exception ${advisory} expires more than ${MAX_EXCEPTION_DAYS} days from today: ${expires}`,
    );
  }
  if (justification.trim().length < 10) {
    throw new Error(`Security exception ${advisory} needs a meaningful justification`);
  }

  activeExceptions += 1;
}

console.log(`Security exceptions valid: ${activeExceptions} active.`);
