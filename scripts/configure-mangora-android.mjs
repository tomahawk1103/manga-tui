#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const REQUIRED_PERMISSIONS = [
  "android.permission.INTERNET",
  "android.permission.WAKE_LOCK",
];

function ensurePermissions(manifest) {
  let updated = manifest;
  for (const perm of REQUIRED_PERMISSIONS) {
    const pattern = new RegExp(`android:name\\s*=\\s*["']${perm.replaceAll(".", "\\.")}["']`);
    if (!pattern.test(updated)) {
      const tag = `    <uses-permission android:name="${perm}" />\n`;
      if (/<application\b/.test(updated)) {
        updated = updated.replace(/(\s*<application\b)/, `\n${tag}$1`);
      } else {
        throw new Error("Could not find <application> tag in AndroidManifest.xml");
      }
    }
  }
  return updated;
}

function ensureCleartextTraffic(manifest) {
  return manifest.replace(/<application\b([^>]*)>/, (full, attrs) => {
    if (/android:usesCleartextTraffic\s*=/.test(attrs)) {
      const nextAttrs = attrs.replace(
        /android:usesCleartextTraffic\s*=\s*["'][^"']*["']/,
        'android:usesCleartextTraffic="true"',
      );
      return `<application${nextAttrs}>`;
    }
    return `<application${attrs}\n        android:usesCleartextTraffic="true">`;
  });
}

function ensureMainActivityConfig(manifest) {
  const activityRegex = /<activity\b([^>]*)>([\s\S]*?)<\/activity>/g;
  let patchedMain = false;

  const updated = manifest.replace(activityRegex, (full, attrs, body) => {
    const isMainActivity =
      /android:name\s*=\s*["'](?:\.MainActivity|[^"']*MainActivity)["']/.test(attrs) ||
      /android\.intent\.action\.MAIN/.test(body);

    if (!isMainActivity) {
      return full;
    }

    patchedMain = true;
    let nextAttrs = attrs;
    if (/android:launchMode\s*=/.test(nextAttrs)) {
      nextAttrs = nextAttrs.replace(
        /android:launchMode\s*=\s*["'][^"']*["']/,
        'android:launchMode="singleTask"',
      );
    } else {
      nextAttrs = `${nextAttrs}\n            android:launchMode="singleTask"`;
    }

    let nextBody = body;
    if (!/android:scheme\s*=\s*["']mangora["']/.test(nextBody)) {
      const oauthFilter = [
        "",
        "            <intent-filter>",
        '                <action android:name="android.intent.action.VIEW" />',
        '                <category android:name="android.intent.category.DEFAULT" />',
        '                <category android:name="android.intent.category.BROWSABLE" />',
        '                <data android:scheme="mangora" android:host="oauth" />',
        "            </intent-filter>",
      ].join("\n");
      nextBody = `${nextBody.replace(/\s*$/, "")}${oauthFilter}\n        `;
    }

    return `<activity${nextAttrs}>${nextBody}</activity>`;
  });

  if (!patchedMain) {
    throw new Error("Could not locate MainActivity in AndroidManifest.xml");
  }

  return updated;
}

export function patchAndroidManifest(manifest) {
  let next = manifest;
  next = ensurePermissions(next);
  next = ensureCleartextTraffic(next);
  next = ensureMainActivityConfig(next);
  return next;
}

export function patchBuildGradleKts(gradleContent) {
  return gradleContent.replace(
    /manifestPlaceholders\s*\[\s*["']usesCleartextTraffic["']\s*\]\s*=\s*["']false["']/g,
    'manifestPlaceholders["usesCleartextTraffic"] = "true"',
  );
}

export function configureAndroidProject(androidDir) {
  const manifestPath = path.join(androidDir, "app", "src", "main", "AndroidManifest.xml");
  if (!fs.existsSync(manifestPath)) {
    throw new Error(`AndroidManifest.xml not found at: ${manifestPath}`);
  }

  const originalManifest = fs.readFileSync(manifestPath, "utf8");
  const patchedManifest = patchAndroidManifest(originalManifest);
  if (patchedManifest !== originalManifest) {
    fs.writeFileSync(manifestPath, patchedManifest, "utf8");
  }

  const gradlePath = path.join(androidDir, "app", "build.gradle.kts");
  if (fs.existsSync(gradlePath)) {
    const originalGradle = fs.readFileSync(gradlePath, "utf8");
    const patchedGradle = patchBuildGradleKts(originalGradle);
    if (patchedGradle !== originalGradle) {
      fs.writeFileSync(gradlePath, patchedGradle, "utf8");
    }
  }

  return { manifestPath, gradlePath: fs.existsSync(gradlePath) ? gradlePath : null };
}

function runSelfTest() {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "mangora-android-patcher-"));
  try {
    const manifestDir = path.join(tempRoot, "app", "src", "main");
    fs.mkdirSync(manifestDir, { recursive: true });
    const manifestPath = path.join(manifestDir, "AndroidManifest.xml");
    const gradlePath = path.join(tempRoot, "app", "build.gradle.kts");

    const sampleManifest = `<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <uses-permission android:name="android.permission.INTERNET" />
    <application
        android:allowBackup="true"
        android:label="@string/app_name"
        android:usesCleartextTraffic="\${usesCleartextTraffic}">
        <activity
            android:name=".MainActivity"
            android:exported="true"
            android:launchMode="singleTop">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
`;

    const sampleGradle = `android {
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
    }
}
`;

    fs.writeFileSync(manifestPath, sampleManifest, "utf8");
    fs.writeFileSync(gradlePath, sampleGradle, "utf8");

    configureAndroidProject(tempRoot);
    const afterFirst = fs.readFileSync(manifestPath, "utf8");
    const gradleAfterFirst = fs.readFileSync(gradlePath, "utf8");

    configureAndroidProject(tempRoot);
    const afterSecond = fs.readFileSync(manifestPath, "utf8");
    const gradleAfterSecond = fs.readFileSync(gradlePath, "utf8");

    assert.equal(afterFirst, afterSecond, "Manifest patcher must be idempotent");
    assert.equal(gradleAfterFirst, gradleAfterSecond, "Gradle patcher must be idempotent");

    assert.match(afterFirst, /android\.permission\.INTERNET/);
    assert.match(afterFirst, /android\.permission\.WAKE_LOCK/);
    assert.match(afterFirst, /android:usesCleartextTraffic="true"/);
    assert.doesNotMatch(afterFirst, /\$\{usesCleartextTraffic\}/);
    assert.match(afterFirst, /android:launchMode="singleTask"/);
    assert.match(afterFirst, /<data android:scheme="mangora" android:host="oauth" \/>/);
    assert.match(gradleAfterFirst, /manifestPlaceholders\["usesCleartextTraffic"\] = "true"/);

    console.log("configure-mangora-android self-test passed.");
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
}

function main() {
  const args = process.argv.slice(2);
  if (args.includes("--self-test")) {
    runSelfTest();
    return;
  }

  let androidDir = path.resolve("apps/mangora/src-tauri/gen/android");
  const idx = args.indexOf("--android-dir");
  if (idx !== -1 && args[idx + 1]) {
    androidDir = path.resolve(args[idx + 1]);
  }

  const result = configureAndroidProject(androidDir);
  console.log(`Configured Android project: ${result.manifestPath}`);
}

main();
