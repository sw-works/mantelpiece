import { describe, expect, it } from "vitest";
import { type QueueItem } from "../src/upload/queue.js";
import { doneTitle, failures, inFlight, kept, latestSent, sendingTitle, shareHint, summaryLine, tally } from "../src/upload/view.js";

const file = (name: string, size = 100) => new File([new Uint8Array(size)], name, { type: "image/jpeg" });
const item = (name: string, phase: QueueItem["progress"]["phase"], fraction = 1, mediaId?: string): QueueItem => ({
  file: file(name),
  batchId: "b",
  progress: { phase, fraction },
  ...(mediaId ? { mediaId } : {}),
});

describe("upload page view", () => {
  const batch = [
    item("a.jpg", "done", 1, "m1"),
    item("b.jpg", "done", 1, "m2"),
    item("c.jpg", "duplicate"),
    item("d.jpg", "uploading", 0.5),
    item("e.jpg", "hashing", 0.3),
    item("f.jpg", "hashing", 0),
    item("g.jpg", "failed"),
  ];

  it("lists only files in flight and failures, however big the batch", () => {
    expect(inFlight(batch).map((it) => it.file.name)).toEqual(["d.jpg", "e.jpg"]);
    expect(failures(batch).map((it) => it.file.name)).toEqual(["g.jpg"]);
    const huge = Array.from({ length: 1000 }, (_, i) => item(`${i}.jpg`, i < 3 ? "uploading" : "hashing", i < 3 ? 0.2 : 0));
    expect(inFlight(huge)).toHaveLength(3);
  });

  it("counts without saying zero", () => {
    const t = tally(batch, new Set());
    expect(sendingTitle(t)).toBe("3 of 7 sent");
    expect(summaryLine(t)).toBe("2 photos added · 1 already on the TV · 1 did not send");
    expect(summaryLine(tally([item("a.jpg", "done", 1, "m1")], new Set()))).toBe("1 photo added");
    expect(summaryLine(tally([item("f.jpg", "hashing", 0)], new Set()))).toBe("");
  });

  it("follows removals in the title, the line and the review grid", () => {
    const removed = new Set(["m1"]);
    const finished = batch.filter((it) => it.progress.phase === "done" || it.progress.phase === "duplicate");
    expect(doneTitle(tally(finished, new Set()))).toBe("All 3 on the TV");
    expect(doneTitle(tally(finished, removed))).toBe("2 on the TV");
    expect(summaryLine(tally(finished, removed))).toBe("1 photo added · 1 already on the TV · 1 removed");
    expect(kept(batch, removed).map((it) => it.file.name)).toEqual(["b.jpg"]);
    expect(doneTitle(tally(batch, new Set()))).toBe("3 of 7 on the TV");
  });

  it("shows the newest sent first", () => {
    expect(latestSent(batch, 4).map((it) => it.file.name)).toEqual(["b.jpg", "a.jpg"]);
  });

  it("offers the share sheet only on Android", () => {
    expect(shareHint("Mozilla/5.0 (Linux; Android 14; Pixel 8)")).toMatch(/home screen/);
    expect(shareHint("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)")).toBeNull();
  });
});
