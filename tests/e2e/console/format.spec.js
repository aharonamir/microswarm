import { test, expect } from "@playwright/test";
import { duration, relativeTime, shortId } from "../../../crates/ms-console/web/js/format.js";

test("formats durations and short identifiers", async () => {
  expect(duration(400)).toBe("0.4 s");
  expect(duration(220000)).toBe("3 min 40 s");
  expect(duration(7500000)).toBe("2 h 5 min");
  expect(relativeTime(11 * 60_000, 0)).toBe("11 min left");
  expect(shortId("01ARZ3NDEKTSV4RRFFQ69G5FAV")).toBe("01ARZ3…FAV");
});
