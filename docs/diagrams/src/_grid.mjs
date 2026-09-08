// Shared grid for the workflow diagrams: six columns, rows on a 120px pitch.
export const W = 190, GAP = 28, H = 64;
export const X = (i) => 40 + i * (W + GAP);
export const R = (i) => 100 + i * 120;
export const mid = (i) => X(i) + W / 2;
export const WIDTH = X(6) + 12;
