// HAVE TO BE EQUAL
// NUM_BANDS FROM pipeline.rs
export const NUM_BANDS = 35;

let latest: number[] = Array(NUM_BANDS).fill(0);

export function setLatestBands(bands: number[]) {
  latest = bands;
}

export function getLatestBands(): number[] {
  return latest;
}
