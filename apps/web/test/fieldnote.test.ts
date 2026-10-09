import { describe, expect, it } from "vitest";
import { shareMethod, withNotes } from "../src/fieldnote.js";

const note = { placement: { left: 0.5, top: 0.05, width: 0.4, height: 0.35 }, mode: "town", certainty: "precise" as const, credit: "© OpenStreetMap contributors" };

describe("field notes on the phone", () => {
  it("lists the highlights that have a note, in highlight order", () => {
    expect(withNotes(["a", "b", "c"], { c: note, a: note })).toEqual(["a", "c"]);
    expect(withNotes(["a"], undefined)).toEqual([]);
  });

  it("uses the share sheet only where it takes files", () => {
    const file = new File([new Uint8Array(3)], "x.jpg", { type: "image/jpeg" });
    expect(shareMethod({ canShare: () => true }, file)).toBe("share");
    expect(shareMethod({ canShare: () => false }, file)).toBe("download");
    expect(shareMethod({}, file)).toBe("download");
    expect(shareMethod(undefined, file)).toBe("download");
    expect(
      shareMethod(
        {
          canShare: () => {
            throw new Error("no");
          },
        },
        file,
      ),
    ).toBe("download");
  });
});
