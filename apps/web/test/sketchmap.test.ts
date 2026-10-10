// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { sketchMapView } from "../src/sketchmap.js";

describe("sketch map on the phone (F-712)", () => {
  it("shows the map with its credit once it arrives, with Share for the owner", async () => {
    const view = sketchMapView({ map: Promise.resolve({ url: "https://cdn.test/trip.png", credit: "© OpenStreetMap contributors" }), share: true });
    await Promise.resolve();
    await Promise.resolve();
    expect(view.hidden).toBe(false);
    expect(view.querySelector("img")!.getAttribute("src")).toBe("https://cdn.test/trip.png");
    expect(view.textContent).toContain("© OpenStreetMap contributors");
    expect(view.textContent).toContain("Share map");
  });

  it("is view only without share, and goes away when there is no map yet", async () => {
    const view = sketchMapView({ map: Promise.resolve({ url: "u", credit: "c" }) });
    await Promise.resolve();
    await Promise.resolve();
    expect(view.textContent).not.toContain("Share");
    const parent = document.createElement("div");
    const missing = sketchMapView({ map: Promise.reject(new Error("404")) });
    parent.append(missing);
    await Promise.resolve();
    await Promise.resolve();
    expect(parent.children.length).toBe(0);
  });
});
