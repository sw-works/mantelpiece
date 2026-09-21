import { act, render, screen } from "@testing-library/react-native";
import { Text } from "react-native";
import { KenBurns } from "../src/components/KenBurns";

const scaleOf = () => {
  const style = screen.getByTestId("ken-burns").props.style as { transform: Array<Record<string, number>> };
  return Number(JSON.parse(JSON.stringify(style.transform[0]!.scale)));
};

describe("KenBurns", () => {
  beforeEach(() => jest.useFakeTimers());
  afterEach(() => jest.useRealTimers());

  it("holds the frame on pause, carries on from it, and restarts on a new slide", async () => {
    const view = await render(
      <KenBurns slideKey="m1" paused={false} durationMs={10_000}>
        <Text>photo</Text>
      </KenBurns>,
    );
    await act(async () => void jest.advanceTimersByTime(5_000));
    const moving = scaleOf();
    expect(moving).toBeGreaterThan(1.02);

    // Pause: nothing jumps back to the first frame, and nothing moves on.
    await view.rerender(
      <KenBurns slideKey="m1" paused durationMs={10_000}>
        <Text>photo</Text>
      </KenBurns>,
    );
    expect(scaleOf()).toBeCloseTo(moving, 3);
    await act(async () => void jest.advanceTimersByTime(3_000));
    expect(scaleOf()).toBeCloseTo(moving, 3);

    // Play: on from the held frame.
    await view.rerender(
      <KenBurns slideKey="m1" paused={false} durationMs={10_000}>
        <Text>photo</Text>
      </KenBurns>,
    );
    expect(scaleOf()).toBeCloseTo(moving, 3);
    await act(async () => void jest.advanceTimersByTime(2_000));
    expect(scaleOf()).toBeGreaterThan(moving);

    // A new slide starts from the first frame.
    await view.rerender(
      <KenBurns slideKey="m2" paused={false} durationMs={10_000}>
        <Text>photo</Text>
      </KenBurns>,
    );
    expect(scaleOf()).toBeLessThan(1.01);
  });
});
