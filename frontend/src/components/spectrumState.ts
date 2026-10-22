const NUM_BANDS = 33;

let latest: number[] = Array(NUM_BANDS).fill(0);

export function setLatestBands(bands: number[]) {
  latest = bands;
}

export function getLatestBands(): number[] {
  return latest;
}
