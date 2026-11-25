#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const EXPECTED_BUNDLE_ID = "io.github.tomahawk1103.mangora";
const DEFAULT_RELEASE_BASE = "https://github.com/tomahawk1103/manga-tui/releases/download";

function normalizeVersion(raw) {
  return String(raw ?? "")
    .trim()
    .replace(/^mangora-/i, "")
    .replace(/^v/i, "");
}

function nextBuildVersion(existingVersions, explicitBuildVersion) {
  if (explicitBuildVersion) {
    return String(explicitBuildVersion);
  }
  const numericBuilds = (existingVersions ?? [])
    .map((v) => Number.parseInt(String(v.buildVersion ?? ""), 10))
    .filter((n) => Number.isFinite(n) && n > 0);
  if (numericBuilds.length === 0) {
    return "1";
  }
  return String(Math.max(...numericBuilds) + 1);
}

export function updateSideStoreSource(sourceObj, entry) {
  if (!sourceObj || !Array.isArray(sourceObj.apps) || sourceObj.apps.length === 0) {
    throw new Error("SideStore source JSON must contain a non-empty apps array.");
  }

  const app = sourceObj.apps[0];
  if (app.bundleIdentifier !== EXPECTED_BUNDLE_ID) {
    throw new Error(
      `Expected bundleIdentifier "${EXPECTED_BUNDLE_ID}", found "${app.bundleIdentifier ?? ""}".`,
    );
  }

  const version = normalizeVersion(entry.version);
  if (!version) {
    throw new Error("Version is required.");
  }
  if (!entry.downloadURL || !String(entry.downloadURL).startsWith("https://")) {
    throw new Error(`downloadURL must start with https://, got "${entry.downloadURL ?? ""}".`);
  }
  if (!Number.isInteger(entry.size) || entry.size <= 0) {
    throw new Error(`size must be a positive integer (bytes), got "${entry.size}".`);
  }

  const existing = Array.isArray(app.versions) ? app.versions : [];
  const previousSame = existing.find((v) => normalizeVersion(v.version) === version);
  const remaining = existing.filter((v) => normalizeVersion(v.version) !== version);

  const buildVersion =
    entry.buildVersion ??
    previousSame?.buildVersion ??
    nextBuildVersion(remaining, undefined);

  const newEntry = {
    version,
    buildVersion: String(buildVersion),
    date: entry.date || new Date().toISOString(),
    downloadURL: entry.downloadURL,
    size: entry.size,
    minOSVersion: entry.minOSVersion || "15.0",
    localizedDescription:
      entry.localizedDescription ||
      previousSame?.localizedDescription ||
      `Mangora ${version} para iOS (SideStore / AltStore).`,
  };

  return {
    ...sourceObj,
    apps: [
      {
        ...app,
        versions: [newEntry, ...remaining],
      },
      ...sourceObj.apps.slice(1),
    ],
  };
}

export function updateSideStoreSourceFile(options) {
  const sourcePath = path.resolve(options.source || "docs/sidestore.json");
  const raw = fs.readFileSync(sourcePath, "utf8");
  const sourceObj = JSON.parse(raw);

  const version = normalizeVersion(options.version);
  if (!version) {
    throw new Error("--version is required.");
  }

  let size = options.size !== undefined ? Number(options.size) : undefined;
  let ipaFileName = `Mangora-${version}-ios-unsigned.ipa`;

  if (options.ipa) {
    const ipaPath = path.resolve(options.ipa);
    const stat = fs.statSync(ipaPath);
    size = stat.size;
    ipaFileName = path.basename(ipaPath);
  }

  const tag = options.tag || `mangora-v${version}`;
  const downloadURL =
    options.downloadUrl || `${DEFAULT_RELEASE_BASE}/${tag}/${encodeURIComponent(ipaFileName)}`;

  const updated = updateSideStoreSource(sourceObj, {
    version,
    buildVersion: options.buildVersion,
    date: options.date || new Date().toISOString(),
    downloadURL,
    size,
    minOSVersion: options.minOs || "15.0",
    localizedDescription: options.notes,
  });

  fs.writeFileSync(sourcePath, `${JSON.stringify(updated, null, 2)}\n`, "utf8");
  return { sourcePath, updated };
}

function parseArgs(argv) {
  const opts = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--self-test") {
      opts.selfTest = true;
      continue;
    }
    const next = argv[i + 1];
    switch (arg) {
      case "--source":
        opts.source = next;
        i += 1;
        break;
      case "--version":
        opts.version = next;
        i += 1;
        break;
      case "--tag":
        opts.tag = next;
        i += 1;
        break;
      case "--ipa":
        opts.ipa = next;
        i += 1;
        break;
      case "--size":
        opts.size = next;
        i += 1;
        break;
      case "--download-url":
        opts.downloadUrl = next;
        i += 1;
        break;
      case "--date":
        opts.date = next;
        i += 1;
        break;
      case "--build-version":
        opts.buildVersion = next;
        i += 1;
        break;
      case "--min-os":
        opts.minOs = next;
        i += 1;
        break;
      case "--notes":
        opts.notes = next;
        i += 1;
        break;
      default:
        throw new Error(`Unknown argument: ${arg}`);
    }
  }
  return opts;
}

function runSelfTest() {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "mangora-sidestore-test-"));
  try {
    const sourcePath = path.join(tempRoot, "sidestore.json");
    const ipaPath = path.join(tempRoot, "Mangora-0.2.0-ios-unsigned.ipa");
    const ipaBytes = Buffer.alloc(12345, 0x41);
    fs.writeFileSync(ipaPath, ipaBytes);

    const initialSource = {
      name: "Mangora",
      identifier: "io.github.tomahawk1103.mangora.source",
      apps: [
        {
          name: "Mangora",
          bundleIdentifier: EXPECTED_BUNDLE_ID,
          developerName: "tomahawk1103",
          iconURL: "https://tomahawk1103.github.io/manga-tui/images/mangora-icon.png",
          versions: [
            {
              version: "0.1.0",
              buildVersion: "1",
              date: "2026-11-25T00:00:00Z",
              downloadURL:
                "https://github.com/tomahawk1103/manga-tui/releases/download/mangora-v0.1.0/Mangora-0.1.0-ios-unsigned.ipa",
              size: 10000,
              minOSVersion: "15.0",
            },
          ],
        },
      ],
    };
    fs.writeFileSync(sourcePath, `${JSON.stringify(initialSource, null, 2)}\n`, "utf8");

    updateSideStoreSourceFile({
      source: sourcePath,
      version: "mangora-v0.2.0",
      tag: "mangora-v0.2.0",
      ipa: ipaPath,
      date: "2026-11-26T12:00:00Z",
      notes: "Release 0.2.0",
    });

    // Re-running the same version must be idempotent
    const { updated } = updateSideStoreSourceFile({
      source: sourcePath,
      version: "0.2.0",
      tag: "mangora-v0.2.0",
      ipa: ipaPath,
      date: "2026-11-26T12:00:00Z",
      notes: "Release 0.2.0",
    });

    const versions = updated.apps[0].versions;
    assert.equal(versions.length, 2);
    assert.equal(versions[0].version, "0.2.0");
    assert.equal(versions[0].buildVersion, "2");
    assert.equal(versions[0].size, 12345);
    assert.equal(
      versions[0].downloadURL,
      "https://github.com/tomahawk1103/manga-tui/releases/download/mangora-v0.2.0/Mangora-0.2.0-ios-unsigned.ipa",
    );
    assert.equal(versions[1].version, "0.1.0");

    assert.throws(() =>
      updateSideStoreSource(initialSource, {
        version: "0.3.0",
        downloadURL: "http://insecure.example/app.ipa",
        size: 100,
      }),
    );

    console.log("update-sidestore-source self-test passed.");
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.selfTest) {
    runSelfTest();
    return;
  }

  const { sourcePath, updated } = updateSideStoreSourceFile(opts);
  const latest = updated.apps[0].versions[0];
  console.log(
    `Updated ${sourcePath}: version=${latest.version} build=${latest.buildVersion} size=${latest.size} url=${latest.downloadURL}`,
  );
}

main();
