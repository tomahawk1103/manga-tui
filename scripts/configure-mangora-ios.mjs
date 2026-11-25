#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

function findInfoPlistRecursive(dir) {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  const preferred = [];
  const fallback = [];

  for (const entry of entries) {
    if (entry.name === "build" || entry.name.endsWith(".xcarchive")) {
      continue;
    }
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      const nested = findInfoPlistRecursive(fullPath);
      if (nested) {
        if (entry.name.endsWith("_iOS")) {
          preferred.push(nested);
        } else {
          fallback.push(nested);
        }
      }
    } else if (entry.isFile() && entry.name === "Info.plist") {
      if (path.basename(dir).endsWith("_iOS")) {
        preferred.push(fullPath);
      } else {
        fallback.push(fullPath);
      }
    }
  }

  return preferred[0] ?? fallback[0] ?? null;
}

function insertBeforeRootDictClose(plistXml, block) {
  const idx = plistXml.lastIndexOf("</dict>");
  if (idx === -1) {
    throw new Error("Could not locate closing root </dict> in Info.plist");
  }
  const before = plistXml.slice(0, idx).replace(/\s*$/, "");
  const after = plistXml.slice(idx);
  return `${before}\n${block}\n${after}`;
}

function ensureAppTransportSecurity(plistXml) {
  const atsKeyRegex = /<key>\s*NSAppTransportSecurity\s*<\/key>\s*<dict>([\s\S]*?)<\/dict>/;
  const match = plistXml.match(atsKeyRegex);
  if (match) {
    const inner = match[1];
    if (/<key>\s*NSAllowsArbitraryLoads\s*<\/key>/.test(inner)) {
      const updatedInner = inner.replace(
        /(<key>\s*NSAllowsArbitraryLoads\s*<\/key>\s*)<(?:true|false)\s*\/>/,
        "$1<true/>",
      );
      return plistXml.replace(atsKeyRegex, `<key>NSAppTransportSecurity</key>\n\t<dict>${updatedInner}</dict>`);
    }
    const injectedInner = `${inner.replace(/\s*$/, "")}\n\t\t<key>NSAllowsArbitraryLoads</key>\n\t\t<true/>\n\t`;
    return plistXml.replace(atsKeyRegex, `<key>NSAppTransportSecurity</key>\n\t<dict>${injectedInner}</dict>`);
  }

  const block = [
    "\t<key>NSAppTransportSecurity</key>",
    "\t<dict>",
    "\t\t<key>NSAllowsArbitraryLoads</key>",
    "\t\t<true/>",
    "\t</dict>",
  ].join("\n");
  return insertBeforeRootDictClose(plistXml, block);
}

function ensureUrlTypes(plistXml) {
  const urlTypesRegex = /<key>\s*CFBundleURLTypes\s*<\/key>\s*<array>([\s\S]*?)<\/array>/;
  const mangoraEntry = [
    "\t\t<dict>",
    "\t\t\t<key>CFBundleTypeRole</key>",
    "\t\t\t<string>Editor</string>",
    "\t\t\t<key>CFBundleURLName</key>",
    "\t\t\t<string>io.github.tomahawk1103.mangora</string>",
    "\t\t\t<key>CFBundleURLSchemes</key>",
    "\t\t\t<array>",
    "\t\t\t\t<string>mangora</string>",
    "\t\t\t</array>",
    "\t\t</dict>",
  ].join("\n");

  const match = plistXml.match(urlTypesRegex);
  if (match) {
    const inner = match[1];
    if (/<string>\s*mangora\s*<\/string>/.test(inner)) {
      return plistXml;
    }
    const nextInner = `${inner.replace(/\s*$/, "")}\n${mangoraEntry}\n\t`;
    return plistXml.replace(urlTypesRegex, `<key>CFBundleURLTypes</key>\n\t<array>${nextInner}</array>`);
  }

  const block = ["\t<key>CFBundleURLTypes</key>", "\t<array>", mangoraEntry, "\t</array>"].join("\n");
  return insertBeforeRootDictClose(plistXml, block);
}

function ensureQuerySchemes(plistXml) {
  const requiredSchemes = ["sidestore", "altstore"];
  const schemesRegex = /<key>\s*LSApplicationQueriesSchemes\s*<\/key>\s*<array>([\s\S]*?)<\/array>/;
  const match = plistXml.match(schemesRegex);
  if (match) {
    let inner = match[1];
    const missing = requiredSchemes.filter((s) => !new RegExp(`<string>\\s*${s}\\s*</string>`).test(inner));
    if (missing.length === 0) {
      return plistXml;
    }
    const extra = missing.map((s) => `\t\t<string>${s}</string>`).join("\n");
    inner = `${inner.replace(/\s*$/, "")}\n${extra}\n\t`;
    return plistXml.replace(schemesRegex, `<key>LSApplicationQueriesSchemes</key>\n\t<array>${inner}</array>`);
  }

  const block = [
    "\t<key>LSApplicationQueriesSchemes</key>",
    "\t<array>",
    ...requiredSchemes.map((s) => `\t\t<string>${s}</string>`),
    "\t</array>",
  ].join("\n");
  return insertBeforeRootDictClose(plistXml, block);
}

function ensureBooleanKey(plistXml, key, value) {
  const boolTag = value ? "<true/>" : "<false/>";
  const regex = new RegExp(`(<key>\\s*${key}\\s*</key>\\s*)<(?:true|false)\\s*/>`);
  if (regex.test(plistXml)) {
    return plistXml.replace(regex, `$1${boolTag}`);
  }
  const block = `\t<key>${key}</key>\n\t${boolTag}`;
  return insertBeforeRootDictClose(plistXml, block);
}

export function patchInfoPlist(plistXml) {
  let next = plistXml;
  next = ensureAppTransportSecurity(next);
  next = ensureUrlTypes(next);
  next = ensureQuerySchemes(next);
  next = ensureBooleanKey(next, "UIFileSharingEnabled", true);
  next = ensureBooleanKey(next, "LSSupportsOpeningDocumentsInPlace", true);
  next = ensureBooleanKey(next, "ITSAppUsesNonExemptEncryption", false);
  return next;
}

export function configureIosProject(appleDir) {
  if (!fs.existsSync(appleDir)) {
    throw new Error(`Apple project directory not found at: ${appleDir}`);
  }
  const plistPath = findInfoPlistRecursive(appleDir);
  if (!plistPath) {
    throw new Error(`Could not find iOS Info.plist under: ${appleDir}`);
  }

  const original = fs.readFileSync(plistPath, "utf8");
  const patched = patchInfoPlist(original);
  if (patched !== original) {
    fs.writeFileSync(plistPath, patched, "utf8");
  }

  return { plistPath };
}

function runSelfTest() {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "mangora-ios-patcher-"));
  try {
    const iosDir = path.join(tempRoot, "Mangora_iOS");
    fs.mkdirSync(iosDir, { recursive: true });
    const plistPath = path.join(iosDir, "Info.plist");

    const samplePlist = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
\t<key>CFBundleDevelopmentRegion</key>
\t<string>$(DEVELOPMENT_LANGUAGE)</string>
\t<key>CFBundleDisplayName</key>
\t<string>Mangora</string>
\t<key>CFBundleExecutable</key>
\t<string>$(EXECUTABLE_NAME)</string>
\t<key>CFBundleIdentifier</key>
\t<string>$(PRODUCT_BUNDLE_IDENTIFIER)</string>
\t<key>CFBundleShortVersionString</key>
\t<string>0.1.0</string>
\t<key>CFBundleVersion</key>
\t<string>1</string>
</dict>
</plist>
`;

    fs.writeFileSync(plistPath, samplePlist, "utf8");

    configureIosProject(tempRoot);
    const afterFirst = fs.readFileSync(plistPath, "utf8");

    configureIosProject(tempRoot);
    const afterSecond = fs.readFileSync(plistPath, "utf8");

    assert.equal(afterFirst, afterSecond, "Info.plist patcher must be idempotent");
    assert.match(afterFirst, /<key>NSAppTransportSecurity<\/key>[\s\S]*?<key>NSAllowsArbitraryLoads<\/key>\s*<true\/>/);
    assert.match(afterFirst, /<key>CFBundleURLTypes<\/key>[\s\S]*?<string>mangora<\/string>/);
    assert.match(afterFirst, /<key>CFBundleURLName<\/key>\s*<string>io\.github\.tomahawk1103\.mangora<\/string>/);
    assert.match(afterFirst, /<key>LSApplicationQueriesSchemes<\/key>[\s\S]*?<string>sidestore<\/string>[\s\S]*?<string>altstore<\/string>/);
    assert.match(afterFirst, /<key>UIFileSharingEnabled<\/key>\s*<true\/>/);
    assert.match(afterFirst, /<key>LSSupportsOpeningDocumentsInPlace<\/key>\s*<true\/>/);
    assert.match(afterFirst, /<key>ITSAppUsesNonExemptEncryption<\/key>\s*<false\/>/);

    console.log("configure-mangora-ios self-test passed.");
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

  let appleDir = path.resolve("apps/mangora/src-tauri/gen/apple");
  const idx = args.indexOf("--apple-dir");
  if (idx !== -1 && args[idx + 1]) {
    appleDir = path.resolve(args[idx + 1]);
  }

  const result = configureIosProject(appleDir);
  console.log(`Configured iOS project: ${result.plistPath}`);
}

main();
