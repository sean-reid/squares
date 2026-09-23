# squares

Drop in a photo and get it back as a simple squared rectangle: a rectangle tiled by squares where no group of squares forms a smaller rectangle. Square sizes fall out of Kirchhoff's laws on a planar network, so the search picks arrangements, not sizes, and colors each square with the mean of the photo underneath.

Live at https://squares.dwainosaur.com. Everything runs in the browser. Nothing is uploaded.

## Layout

- `core/` is the Rust crate: the planar network, the solver, the search, and the renderer. It compiles to WebAssembly for the site and runs natively for tests and benchmarks.
- `web/` is the page: TypeScript, Vite, a Web Worker that drives the WebAssembly module, and a Cloudflare Worker that serves the built assets.

## Develop

Rust stable with the `wasm32-unknown-unknown` target, `wasm-pack`, Node 24, and pnpm.

```
cd core && cargo test
cd web && pnpm install && pnpm dev
```

## License

MIT
