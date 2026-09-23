import { STRIDE } from './protocol';

type Tile = {
  id: number;
  x: number;
  y: number;
  side: number;
  r: number;
  g: number;
  b: number;
  sx: number;
  sy: number;
  sside: number;
  sr: number;
  sg: number;
  sb: number;
  tx: number;
  ty: number;
  tside: number;
  tr: number;
  tg: number;
  tb: number;
  t0: number;
  t1: number;
  dying: boolean;
};

const DURATION = 420;
const RIPPLE_MS_PER_WIDTH = 260;

const easeOut = (p: number) => 1 - Math.pow(1 - p, 3);
const lerp = (a: number, b: number, p: number) => a + (b - a) * p;

/**
 * Draws the tiling and animates between states. A square is an edge of the
 * network, so most squares survive a search move; each keeps its identity
 * and slides to its new place. New squares grow from their center, removed
 * ones shrink away, and every square's start is delayed by its distance from
 * the nearest change so an improvement ripples outward.
 */
export class Renderer {
  private tiles = new Map<number, Tile>();
  private ctx: CanvasRenderingContext2D;
  private raf = 0;
  width = 1;
  gap = false;
  paper = '#f4f0e8';
  photo: ImageBitmap | null = null;
  frame: [number, number, number, number] = [1, 0, 0, 0];
  showPhoto = false;
  reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;

  constructor(private canvas: HTMLCanvasElement) {
    const ctx = canvas.getContext('2d', { alpha: false });
    if (!ctx) throw new Error('no 2d context');
    this.ctx = ctx;
  }

  clear() {
    this.tiles.clear();
    this.schedule();
  }

  update(flat: Float32Array, width: number) {
    const now = performance.now();
    this.width = width;
    const seen = new Set<number>();
    const changed: [number, number][] = [];
    const incoming: Tile[] = [];
    for (let i = 0; i + STRIDE <= flat.length; i += STRIDE) {
      const id = flat[i] as number;
      const x = flat[i + 1] as number;
      const y = flat[i + 2] as number;
      const side = flat[i + 3] as number;
      const r = flat[i + 4] as number;
      const g = flat[i + 5] as number;
      const b = flat[i + 6] as number;
      seen.add(id);
      const t = this.tiles.get(id);
      if (t && !t.dying) {
        t.tx = x;
        t.ty = y;
        t.tside = side;
        t.tr = r;
        t.tg = g;
        t.tb = b;
        incoming.push(t);
      } else {
        const cx = x + side / 2;
        const cy = y + side / 2;
        changed.push([cx, cy]);
        const fresh: Tile = {
          id,
          x: cx,
          y: cy,
          side: 0,
          r,
          g,
          b,
          sx: cx,
          sy: cy,
          sside: 0,
          sr: r,
          sg: g,
          sb: b,
          tx: x,
          ty: y,
          tside: side,
          tr: r,
          tg: g,
          tb: b,
          t0: now,
          t1: now + DURATION,
          dying: false,
        };
        this.tiles.set(id, fresh);
        incoming.push(fresh);
      }
    }
    for (const t of this.tiles.values()) {
      if (!seen.has(t.id) && !t.dying) {
        t.dying = true;
        const cx = t.x + t.side / 2;
        const cy = t.y + t.side / 2;
        changed.push([cx, cy]);
        t.tx = cx;
        t.ty = cy;
        t.tside = 0;
      }
    }
    for (const t of this.tiles.values()) {
      t.sx = t.x;
      t.sy = t.y;
      t.sside = t.side;
      t.sr = t.r;
      t.sg = t.g;
      t.sb = t.b;
      const delay = this.reduceMotion ? 0 : this.rippleDelay(t, changed);
      t.t0 = now + delay;
      t.t1 = t.t0 + (this.reduceMotion ? 0 : DURATION);
    }
    this.schedule();
  }

  private rippleDelay(t: Tile, changed: [number, number][]) {
    if (changed.length === 0) return 0;
    const cx = t.tx + t.tside / 2;
    const cy = t.ty + t.tside / 2;
    let best = Infinity;
    for (const [x, y] of changed) {
      const d = Math.hypot(cx - x, cy - y);
      if (d < best) best = d;
    }
    return (best / Math.max(this.width, 1)) * RIPPLE_MS_PER_WIDTH;
  }

  private progress(t: Tile, now: number) {
    if (t.t1 <= t.t0) return 1;
    return Math.min(1, Math.max(0, (now - t.t0) / (t.t1 - t.t0)));
  }

  /** Final positions with no animation, for export. */
  finalTiles(): { x: number; y: number; side: number; r: number; g: number; b: number }[] {
    const out = [];
    for (const t of this.tiles.values()) {
      if (t.dying) continue;
      out.push({ x: t.tx, y: t.ty, side: t.tside, r: t.tr, g: t.tg, b: t.tb });
    }
    return out;
  }

  schedule() {
    if (this.raf) return;
    this.raf = requestAnimationFrame(() => {
      this.raf = 0;
      this.draw();
    });
  }

  fit() {
    const dpr = Math.min(devicePixelRatio || 1, 3);
    const w = Math.round(this.canvas.clientWidth * dpr);
    const h = Math.round(this.canvas.clientHeight * dpr);
    if (w > 0 && h > 0 && (this.canvas.width !== w || this.canvas.height !== h)) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    this.schedule();
  }

  private draw() {
    const now = performance.now();
    const { ctx, canvas } = this;
    const scale = canvas.height;
    ctx.fillStyle = this.paper;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    if (this.showPhoto && this.photo) {
      const [s, ox, oy] = this.frame;
      ctx.drawImage(this.photo, ox, oy, s * this.width, s, 0, 0, canvas.width, canvas.height);
      return;
    }
    let live = false;
    const order: Tile[] = [];
    for (const t of this.tiles.values()) {
      const p = easeOut(this.progress(t, now));
      t.x = lerp(t.sx, t.tx, p);
      t.y = lerp(t.sy, t.ty, p);
      t.side = lerp(t.sside, t.tside, p);
      t.r = lerp(t.sr, t.tr, p);
      t.g = lerp(t.sg, t.tg, p);
      t.b = lerp(t.sb, t.tb, p);
      if (p < 1) live = true;
      else if (t.dying) {
        this.tiles.delete(t.id);
        continue;
      }
      order.push(t);
    }
    order.sort((a, b) => b.side - a.side);
    const inset = this.gap ? Math.max(0.5, 0.5 * (devicePixelRatio || 1)) : 0;
    // While squares are moving, the arrangement they are moving toward sits
    // underneath, so a gap between them shows the incoming color rather than
    // the page.
    if (live) {
      for (const t of order) {
        if (t.dying) continue;
        this.rect(t.tx, t.ty, t.tside, t.tr, t.tg, t.tb, scale, inset);
      }
    }
    for (const t of order) {
      this.rect(t.x, t.y, t.side, t.r, t.g, t.b, scale, inset);
    }
    if (live) this.schedule();
  }

  private rect(
    x: number,
    y: number,
    side: number,
    r: number,
    g: number,
    b: number,
    scale: number,
    inset: number,
  ) {
    const px = x * scale + inset;
    const py = y * scale + inset;
    const ps = side * scale - 2 * inset;
    if (ps <= 0) return;
    this.ctx.fillStyle = `rgb(${r | 0} ${g | 0} ${b | 0})`;
    if (this.gap) this.ctx.fillRect(px, py, ps, ps);
    else
      this.ctx.fillRect(
        Math.round(px),
        Math.round(py),
        Math.max(1, Math.round(px + ps) - Math.round(px)),
        Math.max(1, Math.round(py + ps) - Math.round(py)),
      );
  }
}
