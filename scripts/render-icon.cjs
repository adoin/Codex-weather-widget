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

function svgForSize(size) {
  if (size > 64) {
    return svg;
  }

  const smallSizeOverrides = `
    <style>
      #codex-c-x-stroke, #x { stroke-width: 20px; }
      #de { stroke: #8F60F9; stroke-width: 8px; paint-order: stroke fill; }
    </style>
  `;
  return svg.replace("</svg>", `${smallSizeOverrides}</svg>`);
}

for (const size of sizes) {
  const renderer = new Resvg(svgForSize(size), {
    fitTo: { mode: "width", value: size },
  });
  const png = renderer.render().asPng();
  fs.writeFileSync(path.join(outputDir, `icon-${size}.png`), png);
}
