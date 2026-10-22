import { frontMetrics } from "./spectrumState";

export function startFrontMetrics(): () => void {
  if (!import.meta.env.DEV) return () => {};

  const timer = window.setInterval(() => {
    const m = frontMetrics;
    console.log(
      `[Frontend Metrics] recv=${m.received}/s fresh=${m.freshPaint}/s ` +
        `overwritten=${m.overwritten}/s rafs=${m.rafs}/s`,
    );
    m.received = 0;
    m.freshPaint = 0;
    m.overwritten = 0;
    m.rafs = 0;
  }, 1000);

  return () => clearInterval(timer);
}
