import { describe, expect, it } from "vitest";
import { formatClock, formatDuration } from "./format";

describe("formatDuration", () => {
  it("formats minutes only", () => {
    expect(formatDuration(0)).toBe("0m");
    expect(formatDuration(59_999)).toBe("<1m");
    expect(formatDuration(12 * 60_000)).toBe("12m");
  });

  it("formats hours with padded minutes", () => {
    expect(formatDuration((2 * 60 + 5) * 60_000)).toBe("2h 05m");
    expect(formatDuration(1234 * 3_600_000)).toBe("1,234h 00m");
  });
});

describe("formatClock", () => {
  it("formats minutes and seconds", () => {
    expect(formatClock(0)).toBe("0:00");
    expect(formatClock(187_400)).toBe("3:07");
  });

  it("formats hours", () => {
    expect(formatClock(3_729_000)).toBe("1:02:09");
  });
});
