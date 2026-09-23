import init, { Photo, Session } from './wasm/squares_core.js';
import type { FromWorker, ToWorker } from './protocol';

const SLICE_MS = 14;
const POST_EVERY_MS = 60;

let photo: Photo | null = null;
let session: Session | null = null;
let generation = 0;
let refineDeadline = 0;

const post = (m: FromWorker, transfer: Transferable[] = []) => postMessage(m, transfer);

function snapshot(s: Session): FromWorker {
  const squares = s.squares();
  const f = s.frame();
  return {
    type: 'state',
    squares,
    width: s.width(),
    stage: s.stage(),
    count: s.count(),
    frame: [f[0] ?? 1, f[1] ?? 0, f[2] ?? 0, f[3] ?? 0],
  };
}

function loop(gen: number, lastPost: number, lastStage: number) {
  if (gen !== generation || !session) return;
  const s = session;
  const end = performance.now() + SLICE_MS;
  let changed = false;
  while (performance.now() < end) {
    changed = s.run(4) || changed;
    const stage = s.stage();
    if (stage === 3) break;
    if (stage === 2 && refineDeadline === 0) refineDeadline = performance.now() + refineMs;
    if (stage === 2 && performance.now() > refineDeadline) break;
  }
  const now = performance.now();
  const stage = s.stage();
  const finished = stage === 3 || (stage === 2 && now > refineDeadline);
  if ((changed && now - lastPost > POST_EVERY_MS) || stage !== lastStage || finished) {
    const m = snapshot(s);
    post(m, m.type === 'state' ? [m.squares.buffer] : []);
    lastPost = now;
  }
  if (finished) {
    post({ type: 'done' });
    return;
  }
  setTimeout(() => loop(gen, lastPost, stage), 0);
}

let refineMs = 4000;

const ready = init();

onmessage = async (ev: MessageEvent<ToWorker>) => {
  await ready;
  const m = ev.data;
  switch (m.type) {
    case 'photo':
      generation += 1;
      session = null;
      photo = new Photo(new Uint8Array(m.rgba.buffer), m.width, m.height);
      post({ type: 'ready' });
      break;
    case 'start':
      if (!photo) return;
      generation += 1;
      refineMs = m.refineMs;
      refineDeadline = 0;
      session = new Session(photo, m.squares, m.seed);
      loop(generation, 0, -1);
      break;
    case 'stop':
      generation += 1;
      break;
    case 'svg':
      if (session) post({ type: 'svg', svg: session.svg(m.gap, m.background ?? undefined) });
      break;
  }
};
