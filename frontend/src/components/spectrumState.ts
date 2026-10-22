const NUM_BANDS = 28;

let latest: number[] = Array(NUM_BANDS).fill(0);
let unread = false;

export const frontMetrics = {
  received: 0, // mensagens recebidas do Rust
  freshPaint: 0, // pinturas que usaram dado novo
  overwritten: 0, // dados substituídos antes de pintar
  rafs: 0, // quadros de animação executados
};

export function setLatestBands(bands: number[]) {
  latest = bands;
  frontMetrics.received++;
  if (unread) frontMetrics.overwritten++;
  unread = true;
}

export function getLatestBands(): number[] {
  frontMetrics.rafs++;
  if (unread) {
    frontMetrics.freshPaint++;
    unread = false;
  }
  return latest;
}
