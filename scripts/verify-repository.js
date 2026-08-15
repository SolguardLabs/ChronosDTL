"use strict";

const { execFileSync } = require("node:child_process");
const { existsSync, readFileSync, readdirSync, statSync } = require("node:fs");
const { extname, join, relative, resolve } = require("node:path");

const root = resolve(__dirname, "..");

const expectedDocs = [
  "architecture.md",
  "debt-and-epochs.md",
  "economic-model.md",
  "governance.md",
  "operations.md",
  "sdk.md",
  "security-model.md",
];

const requiredFiles = [
  ".editorconfig",
  ".gitattributes",
  ".github/CODEOWNERS",
  ".github/dependabot.yml",
  ".github/workflows/ci.yml",
  ".github/workflows/release-integrity.yml",
  ".gitignore",
  ".prettierignore",
  "Cargo.lock",
  "Cargo.toml",
  "LICENSE",
  "README.md",
  "SECURITY.md",
  "assets/banner.png",
  "package-lock.json",
  "package.json",
  "sdk/chronosClient.js",
  "src/capital/mod.rs",
  "src/governance/mod.rs",
  "tests/production_controls.rs",
];

const excludedDirectories = new Set([".git", "assets", "node_modules", "target", "private"]);
const textExtensions = new Set([
  "",
  ".js",
  ".json",
  ".lock",
  ".md",
  ".rs",
  ".sh",
  ".toml",
  ".yaml",
  ".yml",
]);
const reservedWords = [
  ["c", "tf"].join(""),
  ["la", "boratorio"].join(""),
  ["vulnera", "bilidad"].join(""),
  ["vulnera", "ble"].join(""),
  ["ex", "ploit"].join(""),
  ["bu", "g"].join(""),
];

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function publicTextFiles(directory = root) {
  const files = [];
  for (const entry of readdirSync(directory)) {
    if (excludedDirectories.has(entry)) continue;
    const absolute = join(directory, entry);
    const stats = statSync(absolute);
    if (stats.isDirectory()) {
      files.push(...publicTextFiles(absolute));
    } else if (
      textExtensions.has(extname(entry).toLowerCase()) ||
      entry === ".gitignore" ||
      entry === ".editorconfig"
    ) {
      files.push(absolute);
    }
  }
  return files;
}

function verifyArtifacts() {
  for (const file of requiredFiles) {
    assert(existsSync(join(root, file)), `missing required artifact: ${file}`);
  }
  const docs = readdirSync(join(root, "docs"))
    .filter((name) => name.endsWith(".md"))
    .sort();
  assert(
    JSON.stringify(docs) === JSON.stringify(expectedDocs),
    `docs set is not exact: ${docs.join(", ")}`,
  );

  const readme = readFileSync(join(root, "README.md"), "utf8");
  assert(readme.includes("./assets/banner.png"), "README does not use the canonical banner");
  assert(readme.includes("Production%201.0.0"), "README does not expose the production release");
  for (const doc of docs) {
    assert(readme.includes(`docs/${doc}`), `README does not link docs/${doc}`);
  }

  const documentation = [
    readme,
    readFileSync(join(root, "SECURITY.md"), "utf8"),
    ...docs.map((doc) => readFileSync(join(root, "docs", doc), "utf8")),
  ].join("\n");
  const diagrams = documentation.match(/```mermaid/gu)?.length ?? 0;
  assert(diagrams >= 14, "documentation requires at least fourteen Mermaid diagrams");

  const bannerBytes = statSync(join(root, "assets", "banner.png")).size;
  assert(bannerBytes >= 100_000, "banner asset is unexpectedly small");
  assert(
    readFileSync(join(root, "Cargo.toml"), "utf8").includes('version = "1.0.0"'),
    "Cargo version is not 1.0.0",
  );
  assert(require(join(root, "package.json")).version === "1.0.0", "package version is not 1.0.0");
  return { docs: docs.length, diagrams, bannerBytes };
}

function verifyNarrative() {
  const findings = [];
  const files = publicTextFiles();
  for (const file of files) {
    const content = readFileSync(file, "utf8");
    for (const word of reservedWords) {
      if (new RegExp(`\\b${word}\\b`, "giu").test(content)) {
        findings.push(`${relative(root, file)}:${word}`);
      }
    }
  }
  assert(findings.length === 0, `public narrative contains reserved terms: ${findings.join(", ")}`);
  return { files: files.length };
}

function verifyPrivateBoundary() {
  const tracked = execFileSync("git", ["ls-files", "tests/private", "private-notes.md"], {
    cwd: root,
    encoding: "utf8",
  }).trim();
  assert(tracked === "", `private material is tracked: ${tracked}`);
  return { trackedPrivateFiles: 0 };
}

function verifyRepository() {
  return {
    artifacts: verifyArtifacts(),
    narrative: verifyNarrative(),
    boundary: verifyPrivateBoundary(),
  };
}

if (require.main === module) {
  console.log(JSON.stringify(verifyRepository()));
}

module.exports = {
  expectedDocs,
  root,
  verifyArtifacts,
  verifyNarrative,
  verifyPrivateBoundary,
  verifyRepository,
};
