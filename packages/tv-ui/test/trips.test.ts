import { groupByYear } from "../src/trips";

describe("groupByYear", () => {
  it("groups by start year, newest year first, keeping the order inside a year", () => {
    const groups = groupByYear([
      { id: "a", start: "2026-04-03T00:00:00Z" },
      { id: "b", start: "2025-09-01T00:00:00Z" },
      { id: "c", start: "2025-04-04T00:00:00Z" },
      { id: "d", start: "" },
    ]);
    expect(groups.map((g) => [g.year, g.trips.map((t) => t.id)])).toEqual([
      ["Undated", ["d"]],
      ["2026", ["a"]],
      ["2025", ["b", "c"]],
    ]);
  });

  it("is empty for no trips", () => {
    expect(groupByYear([])).toEqual([]);
  });
});
