import { useEffect, useState } from 'react';

/**
 * A ticking "now" for timer displays.
 *
 * The interval only drives RE-RENDERS — every consumer derives its value from
 * wall-clock timestamps (`features/timer/clock`), so a missed tick (background,
 * dropped frame) never loses time; the next render simply tells the truth.
 */
export function useNow(intervalMs: number, active: boolean): number {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!active) return undefined;
    setNow(Date.now()); // Snap immediately on activation; no stale first frame.
    const timer = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(timer);
  }, [intervalMs, active]);

  return now;
}
