import { qrMatrix } from "../src/qr";

describe("qrMatrix", () => {
  it("produces a square matrix with finder patterns", () => {
    const m = qrMatrix("https://upload.mantelpiece.test/?t=AB2C3D");
    expect(m.length).toBeGreaterThanOrEqual(21);
    expect(m.every((row) => row.length === m.length)).toBe(true);
    // Top-left finder pattern: 7x7 with a dark border.
    expect(m[0]!.slice(0, 7).every(Boolean)).toBe(true);
    expect(m[1]![1]).toBe(false);
    expect(m[3]![3]).toBe(true);
  });
});
