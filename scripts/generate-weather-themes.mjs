import fs from "node:fs";
import path from "node:path";

const projectRoot = path.resolve(import.meta.dirname, "..");
const sourceRoot = path.resolve(
  process.argv[2] ?? path.join(projectRoot, "node_modules", "qweather-icons", "icons"),
);
const outputRoot = path.join(projectRoot, "assets", "weather-themes");

const entries = [
  ["100", "晴", 0, 0, false], ["101", "多云", 0, 0, false],
  ["102", "少云", 0, 0, false], ["103", "晴间多云", 0, 0, false],
  ["104", "阴", 0, 0, false], ["150", "晴（夜）", 0, 0, false],
  ["151", "多云（夜）", 0, 0, false], ["152", "少云（夜）", 0, 0, false],
  ["153", "晴间多云（夜）", 0, 0, false], ["154", "阴（夜）", 0, 0, false],
  ["300", "阵雨", 2, 0, false], ["301", "强阵雨", 3, 0, false],
  ["302", "雷阵雨", 2, 0, true], ["303", "强雷阵雨", 3, 0, true],
  ["304", "雷阵雨伴冰雹", 2, 0, true], ["305", "小雨", 1, 0, false],
  ["306", "中雨", 2, 0, false], ["307", "大雨", 3, 0, false],
  ["308", "极端降雨", 3, 0, false], ["309", "毛毛雨", 1, 0, false],
  ["310", "暴雨", 3, 0, false], ["311", "大暴雨", 3, 0, false],
  ["312", "特大暴雨", 3, 0, false], ["313", "冻雨", 2, 0, false],
  ["314", "小到中雨", 1, 0, false], ["315", "中到大雨", 2, 0, false],
  ["316", "大到暴雨", 3, 0, false], ["317", "暴雨到大暴雨", 3, 0, false],
  ["318", "大暴雨到特大暴雨", 3, 0, false], ["350", "阵雨（夜）", 2, 0, false],
  ["351", "强阵雨（夜）", 3, 0, false], ["399", "雨", 2, 0, false],
  ["400", "小雪", 0, 1, false], ["401", "中雪", 0, 2, false],
  ["402", "大雪", 0, 3, false], ["403", "暴雪", 0, 3, false],
  ["404", "雨夹雪", 1, 1, false], ["405", "雨雪天气", 1, 1, false],
  ["406", "阵雨夹雪", 2, 2, false], ["407", "阵雪", 0, 2, false],
  ["408", "小到中雪", 0, 1, false], ["409", "中到大雪", 0, 2, false],
  ["410", "大到暴雪", 0, 3, false], ["456", "阵雨夹雪（夜）", 2, 2, false],
  ["457", "阵雪（夜）", 0, 2, false], ["499", "雪", 0, 2, false],
  ["500", "薄雾", 0, 0, false], ["501", "雾", 0, 0, false],
  ["502", "霾", 0, 0, false], ["503", "扬沙", 0, 0, false],
  ["504", "浮尘", 0, 0, false], ["507", "沙尘暴", 0, 0, false],
  ["508", "强沙尘暴", 0, 0, false], ["509", "浓雾", 0, 0, false],
  ["510", "强浓雾", 0, 0, false], ["511", "中度霾", 0, 0, false],
  ["512", "重度霾", 0, 0, false], ["513", "严重霾", 0, 0, false],
  ["514", "大雾", 0, 0, false], ["515", "特强浓雾", 0, 0, false],
  ["900", "热", 0, 0, false], ["901", "冷", 0, 0, false],
  ["999", "未知", 0, 0, false],
].map(([code, name, rain, snow, lightning]) => ({ code, name, rain, snow, lightning }));

const themes = [
  { id: "crystal", sourceSuffix: "-fill", label: "晶透微光" },
  { id: "duotone", sourceSuffix: "", label: "清脆双色" },
  { id: "neon", sourceSuffix: "", label: "夜光线刻" },
];

function palette(code) {
  const number = Number(code);
  if (number === 100 || number === 150 || number === 900) return ["#ffb52e", "#ff7448"];
  if (number >= 300 && number <= 399) return ["#80e7ff", "#4f7dff"];
  if (number >= 400 && number <= 499) return ["#f4f7ff", "#a98cff"];
  if ((number >= 503 && number <= 508) || number === 900) return ["#ffc45e", "#cf7f42"];
  if (number >= 500 && number <= 515) return ["#d2dded", "#7895b8"];
  if (number === 901) return ["#b9f5ff", "#6f91ff"];
  if (number === 999) return ["#c5b8ff", "#7d6ce8"];
  return ["#ffbe3f", "#6ea7ff"];
}

function readSource(code, suffix) {
  // 154 (night overcast) existed in the legacy API table but was removed from
  // the current icon package. Keep the API-compatible slot by reusing the
  // identical overcast geometry from 104.
  const sourceCode = code === "154" ? "104" : code;
  const sourcePath = path.join(sourceRoot, `${sourceCode}${suffix}.svg`);
  if (!fs.existsSync(sourcePath)) throw new Error(`Missing QWeather icon: ${sourcePath}`);
  const raw = fs.readFileSync(sourcePath, "utf8");
  const body = raw.match(/<svg[^>]*>([\s\S]*?)<\/svg>/)?.[1]?.trim();
  if (!body) throw new Error(`Invalid SVG: ${sourcePath}`);
  return body;
}

function themedBody(theme, code) {
  const source = readSource(code, theme.sourceSuffix);
  const [start, end] = palette(code);
  const paint = `paint-${theme.id}-${code}`;
  const glow = `glow-${theme.id}-${code}`;
  if (theme.id === "duotone") {
    return `<defs><linearGradient id="${paint}" x1="2" y1="2" x2="14" y2="14" gradientUnits="userSpaceOnUse"><stop stop-color="${start}"/><stop offset="1" stop-color="${end}"/></linearGradient></defs><g fill="url(#${paint})">${source}</g>`;
  }
  const blur = theme.id === "crystal" ? "0.72" : "0.52";
  const highlight = theme.id === "crystal" ? "0.34" : "0.24";
  return `<defs><linearGradient id="${paint}" x1="2" y1="1" x2="14" y2="15" gradientUnits="userSpaceOnUse"><stop stop-color="${start}"/><stop offset="0.52" stop-color="${theme.id === "neon" ? "#a873ff" : "#cde9ff"}"/><stop offset="1" stop-color="${end}"/></linearGradient><filter id="${glow}" x="-45%" y="-45%" width="190%" height="190%"><feGaussianBlur stdDeviation="${blur}"/></filter></defs><g fill="url(#${paint})"><g opacity="${highlight}" filter="url(#${glow})">${source}</g><g stroke="#ffffff" stroke-opacity="${theme.id === "crystal" ? "0.32" : "0.16"}" stroke-width="0.12" paint-order="stroke">${source}</g></g>`;
}

function standaloneSvg(theme, code) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 16 16">${themedBody(theme, code)}</svg>\n`;
}

function rustModule() {
  const arms = [];
  for (const theme of themes) {
    const rustTheme = { crystal: "Crystal", duotone: "Duotone", neon: "Neon" }[theme.id];
    for (const { code } of entries) {
      arms.push(`        (WeatherIconTheme::${rustTheme}, "${code}") => egui::include_image!("../assets/weather-themes/${theme.id}/${code}.svg"),`);
    }
  }
  return `// @generated by scripts/generate-weather-themes.mjs\nuse crate::config::WeatherIconTheme;\n\npub fn weather_icon_source(theme: WeatherIconTheme, qweather_code: Option<&str>, wmo_code: Option<u16>) -> egui::ImageSource<'static> {\n    let code = qweather_code.unwrap_or_else(|| qweather_code_for_wmo(wmo_code));\n    match (theme, code) {\n${arms.join("\n")}\n        (WeatherIconTheme::Crystal, _) => egui::include_image!("../assets/weather-themes/crystal/999.svg"),\n        (WeatherIconTheme::Duotone, _) => egui::include_image!("../assets/weather-themes/duotone/999.svg"),\n        (WeatherIconTheme::Neon, _) => egui::include_image!("../assets/weather-themes/neon/999.svg"),\n    }\n}\n\nfn qweather_code_for_wmo(code: Option<u16>) -> &'static str {\n    match code {\n        Some(0) => "100",\n        Some(1) => "103",\n        Some(2) => "101",\n        Some(3) => "104",\n        Some(45 | 48) => "501",\n        Some(51 | 53 | 55) => "309",\n        Some(56 | 57 | 66 | 67) => "313",\n        Some(61) => "305",\n        Some(63) => "306",\n        Some(65) => "307",\n        Some(71) => "400",\n        Some(73) => "401",\n        Some(75) => "402",\n        Some(77 | 85) => "407",\n        Some(80 | 81) => "300",\n        Some(82) => "301",\n        Some(86) => "402",\n        Some(95) => "302",\n        Some(96 | 99) => "304",\n        _ => "999",\n    }\n}\n`;
}

if (!fs.existsSync(sourceRoot)) throw new Error(`QWeather icon source directory not found: ${sourceRoot}`);
fs.mkdirSync(outputRoot, { recursive: true });
for (const theme of themes) {
  const themeRoot = path.join(outputRoot, theme.id);
  fs.mkdirSync(themeRoot, { recursive: true });
  for (const { code } of entries) {
    fs.writeFileSync(path.join(themeRoot, `${code}.svg`), standaloneSvg(theme, code));
  }
}
const licenseSource = path.resolve(sourceRoot, "..", "LICENSE");
if (fs.existsSync(licenseSource)) fs.copyFileSync(licenseSource, path.join(outputRoot, "LICENSE-QWEATHER-ICONS"));
fs.writeFileSync(path.join(projectRoot, "src", "generated_weather_icons.rs"), rustModule());
console.log(`Generated ${entries.length} weather icons across ${themes.length} themes.`);
