use squares_core::search::{Params, Search, Stage};
use squares_core::svg;
use squares_core::target::Target;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: bench <image> <squares> [seed] [refine-steps] [out.svg]");
        std::process::exit(2);
    }
    let n: usize = args[2].parse().unwrap();
    let seed: u64 = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(1);
    let refine: usize = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(4000);
    let out = args
        .get(5)
        .cloned()
        .unwrap_or_else(|| "out.svg".to_string());

    let t0 = Instant::now();
    let img = image::open(&args[1]).unwrap();
    let (w, h) = (img.width(), img.height());
    let scale = 1024.0 / w.max(h) as f64;
    let img = if scale < 1.0 {
        img.resize(
            (w as f64 * scale) as u32,
            (h as f64 * scale) as u32,
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    let target = Target::from_rgba(rgba.as_raw(), w, h);
    println!(
        "decode+sat {}x{} {:.1}ms",
        w,
        h,
        t0.elapsed().as_secs_f64() * 1e3
    );

    let params = Params {
        squares: n,
        refine_steps: refine,
        ..Params::default()
    };
    let t1 = Instant::now();
    let mut s = Search::new(target, params, seed);
    let mut steps = 0;
    while s.stage() == Stage::Growing {
        s.step();
        steps += 1;
    }
    let grow_ms = t1.elapsed().as_secs_f64() * 1e3;
    println!(
        "grow {} squares in {:.1}ms ({} steps, {} solves, {:.1} cg iters/solve) mse={:.5} crop={:.3} width={:.3}",
        s.squares().len(),
        grow_ms,
        steps,
        s.solves,
        s.solver_iters as f64 / s.solves.max(1) as f64,
        s.mse(),
        s.crop(),
        s.width()
    );

    let t2 = Instant::now();
    let solves_before = s.solves;
    let iters_before = s.solver_iters;
    let mut k = 0;
    let mut next_report = refine / 10;
    while s.stage() == Stage::Refining {
        s.step();
        k += 1;
        if k == next_report {
            println!(
                "  refine {:>6} steps {:>7.1}ms mse={:.5} crop={:.3} accepted={}",
                k,
                t2.elapsed().as_secs_f64() * 1e3,
                s.mse(),
                s.crop(),
                s.accepted()
            );
            next_report += refine / 10;
        }
    }
    let refine_ms = t2.elapsed().as_secs_f64() * 1e3;
    let solves = s.solves - solves_before;
    println!(
        "refine {} steps in {:.1}ms ({} solves, {:.3}ms/solve, {:.1} cg iters/solve) mse={:.5} crop={:.3}",
        k,
        refine_ms,
        solves,
        refine_ms / solves.max(1) as f64,
        (s.solver_iters - iters_before) as f64 / solves.max(1) as f64,
        s.mse(),
        s.crop()
    );
    let colors = s.colors();
    std::fs::write(
        &out,
        svg::render(s.squares(), &colors, s.width(), 0.0, None),
    )
    .unwrap();
    println!("wrote {}", out);
}
