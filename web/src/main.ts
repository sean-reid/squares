import { download, renderPng } from './export';
import type { FromWorker, ToWorker } from './protocol';
import { Renderer } from './tiles';

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const stage = $<HTMLDivElement>('stage');
const canvas = $<HTMLCanvasElement>('canvas');
const hint = $<HTMLParagraphElement>('hint');
const file = $<HTMLInputElement>('file');
const note = $<HTMLParagraphElement>('note');
const controls = $<HTMLDivElement>('controls');
const density = $<HTMLInputElement>('density');
const count = $<HTMLOutputElement>('count');
const status = $<HTMLSpanElement>('status');
const reshuffle = $<HTMLButtonElement>('reshuffle');
const gaps = $<HTMLInputElement>('gaps');
const compare = $<HTMLButtonElement>('compare');
const svgButton = $<HTMLButtonElement>('svg');
const pngButton = $<HTMLButtonElement>('png');
const pngSize = $<HTMLSelectElement>('pngsize');
const another = $<HTMLButtonElement>('another');

const MIN_SQUARES = 40;
const MAX_SQUARES = 1500;
const WORKING_SIZE = 1024;
const REFINE_MS = 4000;

const toCount = (v: number) =>
  Math.round(MIN_SQUARES * Math.pow(MAX_SQUARES / MIN_SQUARES, v / 1000));

const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
const send = (m: ToWorker, transfer: Transferable[] = []) => worker.postMessage(m, transfer);
const renderer = new Renderer(canvas);

let photo: ImageBitmap | null = null;
let seed = 1;
let lastCount = 0;

const paper = () => getComputedStyle(document.documentElement).getPropertyValue('--paper').trim();
renderer.paper = paper();
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
  renderer.paper = paper();
  renderer.schedule();
});
if (matchMedia('(pointer: coarse)').matches) hint.textContent = 'Tap to choose an image.';

async function load(f: File) {
  note.hidden = true;
  let source: ImageBitmap;
  try {
    source = await createImageBitmap(f);
  } catch {
    note.textContent = 'That file did not decode. JPEG, PNG, and WebP work everywhere.';
    note.hidden = false;
    return;
  }
  const k = Math.min(1, WORKING_SIZE / Math.max(source.width, source.height));
  const w = Math.max(1, Math.round(source.width * k));
  const h = Math.max(1, Math.round(source.height * k));
  const off = new OffscreenCanvas(w, h);
  const ctx = off.getContext('2d')!;
  ctx.drawImage(source, 0, 0, w, h);
  source.close();
  const data = ctx.getImageData(0, 0, w, h);
  photo?.close();
  photo = await createImageBitmap(off);
  renderer.photo = photo;
  renderer.clear();
  stage.classList.add('has-image');
  stage.removeAttribute('role');
  stage.removeAttribute('aria-label');
  stage.tabIndex = -1;
  canvas.hidden = false;
  hint.hidden = true;
  controls.hidden = false;
  send({ type: 'photo', rgba: data.data, width: w, height: h }, [data.data.buffer]);
  start();
}

function start() {
  if (!photo) return;
  status.textContent = 'growing';
  send({ type: 'start', squares: toCount(Number(density.value)), seed, refineMs: REFINE_MS });
}

worker.onmessage = (ev: MessageEvent<FromWorker>) => {
  const m = ev.data;
  switch (m.type) {
    case 'state':
      stage.style.setProperty('--aspect', String(m.width));
      renderer.frame = m.frame;
      renderer.fit();
      renderer.update(m.squares, m.width);
      lastCount = m.count;
      status.textContent = m.stage < 2 ? 'growing' : m.stage === 2 ? 'refining' : '';
      break;
    case 'done':
      status.textContent = '';
      break;
    case 'svg':
      download(new Blob([m.svg], { type: 'image/svg+xml' }), `squares-${lastCount}.svg`);
      break;
    case 'ready':
      break;
  }
};

const pick = () => file.click();
stage.addEventListener('click', () => {
  if (!photo) pick();
});
stage.addEventListener('keydown', (e) => {
  if (!photo && (e.key === 'Enter' || e.key === ' ')) {
    e.preventDefault();
    pick();
  }
});
file.addEventListener('change', () => {
  const f = file.files?.[0];
  if (f) void load(f);
  file.value = '';
});
another.addEventListener('click', pick);

document.addEventListener('dragover', (e) => {
  e.preventDefault();
  stage.classList.add('drag');
});
document.addEventListener('dragleave', (e) => {
  if (e.relatedTarget === null) stage.classList.remove('drag');
});
document.addEventListener('drop', (e) => {
  e.preventDefault();
  stage.classList.remove('drag');
  const f = e.dataTransfer?.files[0];
  if (f) void load(f);
});
document.addEventListener('paste', (e) => {
  const f = [...(e.clipboardData?.files ?? [])][0];
  if (f) void load(f);
});

density.addEventListener('input', () => {
  count.value = String(toCount(Number(density.value)));
});
density.addEventListener('change', () => {
  send({ type: 'stop' });
  start();
});
count.value = String(toCount(Number(density.value)));

reshuffle.addEventListener('click', () => {
  seed = (Math.random() * 0x7fffffff) | 0;
  send({ type: 'stop' });
  start();
});

gaps.addEventListener('change', () => {
  renderer.gap = gaps.checked;
  renderer.schedule();
});

const hold = (on: boolean) => {
  renderer.showPhoto = on;
  compare.classList.toggle('held', on);
  renderer.schedule();
};
compare.addEventListener('pointerdown', (e) => {
  e.preventDefault();
  hold(true);
});
for (const ev of ['pointerup', 'pointercancel'] as const)
  window.addEventListener(ev, () => hold(false));
compare.addEventListener('keydown', (e) => {
  if (e.key === ' ' || e.key === 'Enter') {
    e.preventDefault();
    hold(true);
  }
});
compare.addEventListener('keyup', () => hold(false));
compare.addEventListener('blur', () => hold(false));

svgButton.addEventListener('click', () => {
  send({ type: 'svg', gap: gaps.checked ? 0.002 : 0, background: gaps.checked ? paper() : null });
});
pngButton.addEventListener('click', async () => {
  const blob = await renderPng(
    renderer.finalTiles(),
    renderer.width,
    Number(pngSize.value),
    gaps.checked,
    paper(),
  );
  download(blob, `squares-${lastCount}.png`);
});

new ResizeObserver(() => renderer.fit()).observe(stage);
