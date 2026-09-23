export type ToWorker =
  | { type: 'photo'; rgba: Uint8ClampedArray; width: number; height: number }
  | { type: 'start'; squares: number; seed: number; refineMs: number }
  | { type: 'stop' }
  | { type: 'svg'; gap: number; background: string | null };

export type FromWorker =
  | { type: 'ready' }
  | {
      type: 'state';
      squares: Float32Array;
      width: number;
      stage: number;
      count: number;
      frame: [number, number, number, number];
    }
  | { type: 'done' }
  | { type: 'svg'; svg: string };

export const STRIDE = 7;
