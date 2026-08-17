import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve, relative, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const EXCLUDED_ENTRY_PATTERNS = [
  ["redis cloud", /\bredis\s+cloud\b/gi],
  ["redis-cloud", /\bredis-cloud\b/gi],
  ["cloud-capi", /\bcloud-capi\b/gi],
  ["azure managed redis", /\bazure\s+managed\s+redis\b/gi],
  ["cloud login", /\bcloud\s+login\b/gi],
  ["cloud sdk", /\bcloud\s+sdk\b/gi],
  ["cloud api", /\bcloud\s+api\b/gi],
  ["cloud endpoint", /\bcloud\s+endpoint\b/gi],
  ["cloud account", /\bcloud\s+account\b/gi],
  [
    "cloud database discovery",
    /\bcloud\s+database\s+discovery\b/gi,
  ],
];

const PRODUCT_PATHS = ["src", "src-tauri/src", "package.json"];

function findHits(content) {
  return EXCLUDED_ENTRY_PATTERNS.flatMap(([term, pattern], patternOrder) => {
    pattern.lastIndex = 0;
    const matches = [];
    let match = pattern.exec(content);

    while (match) {
      matches.push({ index: match.index, patternOrder, term });
      match = pattern.exec(content);
    }

    return matches;
  }).sort((left, right) => left.index - right.index || left.patternOrder - right.patternOrder);
}

export function scanFiles(contents) {
  const hits = [];
  const seen = new Set();

  for (const content of contents) {
    for (const { term } of findHits(content)) {
      if (!seen.has(term)) {
        seen.add(term);
        hits.push(term);
      }
    }
  }

  return hits;
}

function listFiles(path) {
  const stats = statSync(path);
  if (stats.isFile()) {
    return [path];
  }

  return readdirSync(path, { withFileTypes: true })
    .sort((left, right) => left.name.localeCompare(right.name))
    .flatMap((entry) => listFiles(resolve(path, entry.name)));
}

function getProductFiles(projectRoot) {
  return PRODUCT_PATHS.flatMap((productPath) =>
    listFiles(resolve(projectRoot, productPath)),
  );
}

export function main() {
  const projectRoot = fileURLToPath(new URL("..", import.meta.url));
  const files = getProductFiles(projectRoot);
  const findings = [];

  for (const file of files) {
    const content = readFileSync(file, "utf8");
    for (const { term } of findHits(content)) {
      findings.push({ file: relative(projectRoot, file).split(sep).join("/"), term });
    }
  }

  if (findings.length > 0) {
    for (const finding of findings) {
      console.error(`非 Cloud 范围命中: ${finding.file} -> ${finding.term}`);
    }
    return 1;
  }

  console.log("非 Cloud 范围检查通过：仅扫描 src/、src-tauri/src/ 和 package.json");
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  process.exitCode = main();
}
