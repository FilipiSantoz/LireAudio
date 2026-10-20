const NUM_BANDS = 28;

let latest: number[] = Array(NUM_BANDS).fill(0);

export function setLatestBands(bands: number[]) {
  latest = bands;
}

export function getLatestBands(): number[] {
  return latest;
}
