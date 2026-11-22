import { test, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const css = await readFile(resolve("../../../crates/ms-console/web/styles.css"), "utf8");
function colour(name) {
  const match = css.match(new RegExp(`--${name}:\\s*(#[0-9A-Fa-f]{6})`));
  if (!match) throw new Error(`missing colour token --${name}`);
  return match[1];
}
function luminance(hex) {
  const rgb = hex.slice(1).match(/../g).map(pair => parseInt(pair, 16) / 255).map(value => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
  return rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
}
function ratio(foreground, background) {
  const [light, dark] = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (light + 0.05) / (dark + 0.05);
}

test("documented text and status pairs meet contrast and agent avatars are readable", async () => {
  const pairs = [
    ["ink", "bg"], ["ink", "bg-chat"], ["ink", "surface"], ["ink", "sunken"], ["ink", "hover"], ["ink", "you"],
    ["ink-2", "bg"], ["ink-3", "bg"], ["seal-ink", "seal-soft"], ["teal-ink", "teal-soft"], ["amber-ink", "amber-soft"],
  ];
  for (const [foreground, background] of pairs) expect(ratio(colour(foreground), colour(background)), `${foreground} on ${background}`).toBeGreaterThanOrEqual(4.5);
  for (let index = 0; index <= 6; index++) {
    expect(ratio(colour(`agent-${index}-text`), colour(`agent-${index}-bg`)), `agent ${index}`).toBeGreaterThanOrEqual(4.5);
  }
});
