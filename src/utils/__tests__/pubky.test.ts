import { describe, it, expect } from "vitest";
import { displayPubky, stripPubkyPrefix } from "../pubky";

describe("stripPubkyPrefix", () => {
  it("should strip 'pubky' prefix from string", () => {
    expect(stripPubkyPrefix("pubkyabc123")).toBe("abc123");
  });

  it("should return unchanged string if no 'pubky' prefix", () => {
    expect(stripPubkyPrefix("abc123")).toBe("abc123");
  });

  it("should handle empty string", () => {
    expect(stripPubkyPrefix("")).toBe("");
  });

  it("should handle string that is exactly 'pubky'", () => {
    expect(stripPubkyPrefix("pubky")).toBe("");
  });

  it("should only strip prefix, not occurrences elsewhere", () => {
    expect(stripPubkyPrefix("pubkycontainspubky")).toBe("containspubky");
  });

  it("should be case-sensitive (not strip PUBKY)", () => {
    expect(stripPubkyPrefix("PUBKYabc")).toBe("PUBKYabc");
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
    // With prefix: "pubkyabcdefghijklmnopqrstuvwxyz" (31 chars)
    // After stripping: "abcdefghijklmnopqrstuvwxyz" (26 chars)
    const pubkyPrefixed = "pubkyabcdefghijklmnopqrstuvwxyz";
    const result = displayPubky(pubkyPrefixed);
    expect(result).toBe("abcde...vwxyz");
  });

  it("should strip 'pubky' prefix for short strings too", () => {
    // With prefix: "pubkyshort" -> "short" (5 chars, under threshold)
    expect(displayPubky("pubkyshort")).toBe("short");
  });
});
