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
const VIEW_EASE_MS = 500;

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
  /** Width of the current tiling; the height is 1. */
  width = 1;
  /**
   * Aspect ratio the stage box is showing. It stays put while a search runs,
   * so the box never breathes with every state, and eases to the exact
   * tiling width once the search settles. Meanwhile the tiling is drawn to
   * cover the box, clipping at most a few percent at the edges.
   */
  private view = 1;
  private viewFrom = 1;
  private viewTo = 1;
  private viewT0 = 0;
  private viewT1 = 0;
  onAspect: (aspect: number) => void = () => {};
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

  /** Snap the stage box to an aspect ratio, with no easing. */
  setView(aspect: number) {
    this.view = this.viewFrom = this.viewTo = aspect;
    this.viewT0 = this.viewT1 = 0;
    this.onAspect(aspect);
  }

  /** Ease the stage box to the current tiling width. */
  settleView() {
    if (Math.abs(this.viewTo - this.width) < 1e-6) return;
    const now = performance.now();
    this.viewFrom = this.view;
    this.viewTo = this.width;
    this.viewT0 = now;
    this.viewT1 = now + (this.reduceMotion ? 0 : VIEW_EASE_MS);
    this.schedule();
  }

  private stepView(now: number): boolean {
    if (this.view === this.viewTo) return false;
    const p =
      this.viewT1 <= this.viewT0
        ? 1
        : Math.min(1, (now - this.viewT0) / (this.viewT1 - this.viewT0));
    this.view = lerp(this.viewFrom, this.viewTo, easeOut(p));
    if (p >= 1) this.view = this.viewTo;
    this.onAspect(this.view);
    return this.view !== this.viewTo;
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

  /**
   * Match the canvas to its box. Resizing clears a canvas, so the redraw
   * happens in the same task and no blank frame reaches the screen.
   */
  fit() {
    const dpr = Math.min(devicePixelRatio || 1, 3);
    const w = Math.round(this.canvas.clientWidth * dpr);
    const h = Math.round(this.canvas.clientHeight * dpr);
    if (w > 0 && h > 0 && (this.canvas.width !== w || this.canvas.height !== h)) {
      this.canvas.width = w;
      this.canvas.height = h;
      this.draw();
    }
  }

  /** Scale and offset that cover the canvas with the tiling, centered. */
  private cover() {
    const { canvas } = this;
    const scale = Math.max(canvas.height, canvas.width / Math.max(this.width, 1e-9));
    return {
      scale,
      dx: (canvas.width - this.width * scale) / 2,
      dy: (canvas.height - scale) / 2,
    };
  }

  private draw() {
    const now = performance.now();
    const { ctx, canvas } = this;
    const viewLive = this.stepView(now);
    const { scale, dx, dy } = this.cover();
    ctx.fillStyle = this.paper;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    if (this.showPhoto && this.photo) {
      const [s, ox, oy] = this.frame;
      ctx.drawImage(this.photo, ox, oy, s * this.width, s, dx, dy, this.width * scale, scale);
      if (viewLive) this.schedule();
      return;
    }
    let live = viewLive;
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
        this.rect(t.tx, t.ty, t.tside, t.tr, t.tg, t.tb, scale, dx, dy, inset, false, 1);
      }
    }
    for (const t of order) {
      this.rect(t.x, t.y, t.side, t.r, t.g, t.b, scale, dx, dy, inset, live);
    }
    if (live) this.schedule();
  }

  /**
   * Squares snap to whole pixels only once everything is still. The underlay
   * is drawn snapped and padded by a pixel so it is opaque under the
   * antialiased edges of moving squares.
   */
  private rect(
    x: number,
    y: number,
    side: number,
    r: number,
    g: number,
    b: number,
    scale: number,
    dx: number,
    dy: number,
    inset: number,
    moving: boolean,
    pad = 0,
  ) {
    const px = x * scale + dx + inset - pad;
    const py = y * scale + dy + inset - pad;
    const ps = side * scale - 2 * inset + 2 * pad;
    if (ps <= 0) return;
    this.ctx.fillStyle = `rgb(${r | 0} ${g | 0} ${b | 0})`;
    if (this.gap || moving) this.ctx.fillRect(px, py, ps, ps);
    else
      this.ctx.fillRect(
        Math.round(px),
        Math.round(py),
        Math.max(1, Math.round(px + ps) - Math.round(px)),
        Math.max(1, Math.round(py + ps) - Math.round(py)),
      );
  }
}
