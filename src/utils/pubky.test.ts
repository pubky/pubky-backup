import { describe, it, expect } from "vitest";
import { displayPubky, stripPubkyPrefix } from "./pubky";

const PUBKY = "gabri5dk51q1c1kzi53hjpqw5yhmw1nqsryzsfx5kubd7jpm63jy";
/** A bare key that itself starts with "pubky" */
const PUBKY_STARTING_WITH_PUBKY =
  "pubkyydk51q1c1kzi53hjpqw5yhmw1nqsryzsfx5kubd7jpm63jy";

describe("stripPubkyPrefix", () => {
  it("should strip 'pubky' prefix from a prefixed key", () => {
    expect(stripPubkyPrefix(`pubky${PUBKY}`)).toBe(PUBKY);
  });

  it("should return a bare key unchanged", () => {
    expect(stripPubkyPrefix(PUBKY)).toBe(PUBKY);
  });

  it("should keep a bare key that starts with 'pubky' whole", () => {
    expect(stripPubkyPrefix(PUBKY_STARTING_WITH_PUBKY)).toBe(
      PUBKY_STARTING_WITH_PUBKY,
    );
  });

  it("should leave 'pubky' alone when no full key follows it", () => {
    expect(stripPubkyPrefix("pubky")).toBe("pubky");
    expect(stripPubkyPrefix("pubkyabc123")).toBe("pubkyabc123");
  });

  it("should handle empty string", () => {
    expect(stripPubkyPrefix("")).toBe("");
  });

  it("should be case-sensitive (not strip PUBKY)", () => {
    expect(stripPubkyPrefix(`PUBKY${PUBKY}`)).toBe(`PUBKY${PUBKY}`);
  });
});

describe("displayPubky", () => {
  it("should return ... for null input", () => {
    expect(displayPubky(null)).toBe("...");
  });

  it("should return full string if short enough", () => {
    // String with length <= prefix(5) + suffix(5) + ellipsis(3) = 13 chars
    expect(displayPubky("short")).toBe("short");
    expect(displayPubky("1234567890")).toBe("1234567890");
    expect(displayPubky("1234567890123")).toBe("1234567890123");
  });

  it("should truncate long pubkeys", () => {
    // String with length > 13 chars
    const longPubky = "abcdefghijklmnopqrstuvwxyz";
    const result = displayPubky(longPubky);
    expect(result).toBe("abcde...vwxyz");
  });

  it("should respect default prefix length of 5", () => {
    const longPubky = "0123456789abcdefghij";
    const result = displayPubky(longPubky);
    expect(result.startsWith("01234")).toBe(true);
    expect(result.includes("...")).toBe(true);
    expect(result.endsWith("fghij")).toBe(true);
  });

  it("should respect custom prefix length", () => {
    const longPubky = "0123456789abcdefghij";
    const result = displayPubky(longPubky, 8);
    expect(result.startsWith("01234567")).toBe(true);
    expect(result.includes("...")).toBe(true);
    expect(result.endsWith("fghij")).toBe(true);
  });

  it("should respect custom suffix length", () => {
    const longPubky = "0123456789abcdefghij";
    const result = displayPubky(longPubky, 5, 8);
    expect(result.startsWith("01234")).toBe(true);
    expect(result.includes("...")).toBe(true);
    expect(result.endsWith("cdefghij")).toBe(true);
  });

  it("should respect both custom prefix and suffix lengths", () => {
    const longPubky = "0123456789abcdefghij";
    const result = displayPubky(longPubky, 3, 3);
    expect(result).toBe("012...hij");
  });

  it("should handle edge case at boundary length", () => {
    // Exactly 14 chars - should be truncated
    const boundaryPubky = "12345678901234";
    const result = displayPubky(boundaryPubky);
    expect(result).toBe("12345...01234");
  });

  it("should strip 'pubky' prefix before truncating", () => {
    expect(displayPubky(`pubky${PUBKY}`)).toBe("gabri...m63jy");
  });

  it("should keep a bare key that starts with 'pubky' whole", () => {
    expect(displayPubky(PUBKY_STARTING_WITH_PUBKY)).toBe("pubky...m63jy");
  });
});
