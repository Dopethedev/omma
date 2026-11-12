// Assert the JSON contract of a real `omma --json` run. Usage:
//   node ci/check-json.mjs FILE full|small|region|stdin
import { readFileSync } from "node:fs";

const [file, mode] = process.argv.slice(2);
if (!file || !mode) {
  console.error("usage: node ci/check-json.mjs FILE full|small|region|stdin");
  process.exit(2);
}

const fail = (message) => {
  console.error(`json contract (${mode}): ${message}`);
  process.exit(1);
};

const report = JSON.parse(readFileSync(file, "utf8"));

for (const key of [
  "schema_version",
  "tool",
  "image",
  "region",
  "ocr",
  "colors",
  "grid",
  "layout",
  "vision",
  "warnings",
  "processing",
]) {
  if (!(key in report)) fail(`missing top-level key ${key}`);
}
if (report.schema_version !== "1.0") fail(`schema_version is ${report.schema_version}`);
if (report.tool.name !== "omma") fail("tool.name");
if (!Array.isArray(report.warnings)) fail("warnings must be an array");
if (report.vision !== null) fail("vision must be null when it was not requested");
if (report.grid.width !== 64 || report.grid.height !== 36) fail("grid geometry");
if (report.grid.cells.length !== report.grid.height) fail("grid rows");
if (report.grid.cells[0].length !== report.grid.width) fail("grid columns");

if (mode === "full" || mode === "small") {
  if (!report.ocr) fail("ocr is null although tesseract should be installed");
  if (report.ocr.untrusted !== true) fail("ocr.untrusted must be true");
  if (!/omma reads this line/i.test(report.ocr.text)) fail(`ocr text: ${JSON.stringify(report.ocr.text)}`);
  if (!(report.ocr.confidence > 50)) fail(`ocr confidence: ${report.ocr.confidence}`);
  if (!report.ocr.words.length) fail("no words");
  if (!(report.ocr.psm === 6 || report.ocr.psm === 11)) fail(`psm: ${report.ocr.psm}`);
  if (!Array.isArray(report.layout.regions)) fail("layout.regions");
  if (mode === "small" && report.ocr.upscaled !== 4) fail(`upscaled: ${report.ocr.upscaled}`);
  for (const word of report.ocr.words) {
    const n = word.bbox.normalized;
    for (const k of ["x", "y", "width", "height"]) {
      if (!(n[k] >= 0 && n[k] <= 1)) fail(`normalized ${k} out of range: ${n[k]}`);
    }
  }
}

if (mode === "region") {
  if (!report.region) fail("region block missing");
  if (report.region.width !== 200 || report.region.y !== 10) fail("region values");
  if (report.image.width !== 600) fail("source width must be preserved");
  if (report.image.analyzed_width !== 200) fail("analyzed width");
}

if (mode === "stdin") {
  if (report.image.filename !== "stdin") fail(`filename: ${report.image.filename}`);
  if (report.ocr !== null) fail("ocr must be null under --no-ocr");
  if (!report.colors) fail("colours must still run");
}

console.log(
  `json contract ok (${mode}): ${report.image.width}x${report.image.height}, ` +
    `ocr=${report.ocr ? report.ocr.confidence : "off"}, regions=${report.layout.regions.length}`,
);
