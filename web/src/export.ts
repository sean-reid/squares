export function download(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

export function renderPng(
  tiles: { x: number; y: number; side: number; r: number; g: number; b: number }[],
  width: number,
  pixelWidth: number,
  gap: boolean,
  paper: string,
): Promise<Blob> {
  const scale = pixelWidth / width;
  const canvas = document.createElement('canvas');
  canvas.width = pixelWidth;
  canvas.height = Math.round(scale);
  const ctx = canvas.getContext('2d')!;
  ctx.fillStyle = paper;
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  const inset = gap ? Math.max(0.5, scale * 0.0008) : 0;
  const sorted = [...tiles].sort((a, b) => b.side - a.side);
  for (const t of sorted) {
    const px = t.x * scale + inset;
    const py = t.y * scale + inset;
    const ps = t.side * scale - 2 * inset;
    if (ps <= 0) continue;
    ctx.fillStyle = `rgb(${t.r | 0} ${t.g | 0} ${t.b | 0})`;
    if (gap) ctx.fillRect(px, py, ps, ps);
    else
      ctx.fillRect(
        Math.round(px),
        Math.round(py),
        Math.max(1, Math.round(px + ps) - Math.round(px)),
        Math.max(1, Math.round(py + ps) - Math.round(py)),
      );
  }
  return new Promise((resolve, reject) =>
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error('png failed'))), 'image/png'),
  );
}
