"use strict";

const fs = require("node:fs");
const path = require("node:path");

const [svgPath, outputDir, modulePath, sizesValue] = process.argv.slice(2);
if (!svgPath || !outputDir || !modulePath || !sizesValue) {
  throw new Error("Usage: node render-icon.cjs <svg> <output-dir> <resvg-module> <sizes>");
}

const { Resvg } = require(modulePath);
const svg = fs.readFileSync(svgPath, "utf8");
const sizes = sizesValue.split(",").map(Number);

for (const size of sizes) {
  const renderer = new Resvg(svg, {
    fitTo: { mode: "width", value: size },
  });
  const png = renderer.render().asPng();
  fs.writeFileSync(path.join(outputDir, `icon-${size}.png`), png);
}
