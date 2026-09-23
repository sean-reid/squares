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

    let env = |k: &str, d: f64| -> f64 {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let dflt = Params::default();
    let params = Params {
        squares: n,
        refine_steps: refine,
        thin_weight: env("SQ_THIN", dflt.thin_weight),
        soft_px: env("SQ_SOFT", dflt.soft_px),
        candidates: env("SQ_CAND", dflt.candidates as f64) as usize,
        temp_start: env("SQ_T0", dflt.temp_start),
        temp_end: env("SQ_T1", dflt.temp_end),
        ..dflt
    };
    let t1 = Instant::now();
    let mut s = Search::new(std::rc::Rc::new(target), params, seed);
    let mut steps = 0;
    while s.stage() == Stage::Seeding && steps < 100_000 {
        s.step();
        steps += 1;
    }
    println!(
        "seed {} squares in {:.1}ms ({} steps) crop={:.3} width={:.3}",
        s.squares().len(),
        t1.elapsed().as_secs_f64() * 1e3,
        steps,
        s.crop(),
        s.width()
    );
    let mut next_mark = 100;
    let verify = std::env::var("SQ_VERIFY").is_ok();
    let mut worst = 0.0f64;
    while s.stage() == Stage::Growing && steps < 20_000 {
        if s.step() && verify {
            let d = s.verify();
            if d > worst {
                worst = d;
                println!(
                    "  verify step {} drift {:.3e} last move {:?}",
                    steps, d, s.last_move
                );
            }
        }
        steps += 1;
        if s.squares().len() >= next_mark {
            println!(
                "  grow {:>5} squares {:>8.1}ms {} solves crop={:.3}",
                s.squares().len(),
                t1.elapsed().as_secs_f64() * 1e3,
                s.solves,
                s.crop()
            );
            next_mark += 100;
        }
    }
    println!("rejections after growth: {:?}", s.rejections);
    s.rejections.clear();
    let grow_ms = t1.elapsed().as_secs_f64() * 1e3;
    println!(
        "grow {} squares in {:.1}ms ({} steps, {} factorizations, {} quick solves) mse={:.5} crop={:.3} width={:.3}",
        s.squares().len(),
        grow_ms,
        steps,
        s.factorizations,
        s.solves,
        s.mse(),
        s.crop(),
        s.width()
    );

    let t2 = Instant::now();
    let solves_before = s.solves;
    let factor_before = s.factorizations;
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
        "refine {} steps in {:.1}ms ({} factorizations, {} quick solves) mse={:.5} crop={:.3}",
        k,
        refine_ms,
        s.factorizations - factor_before,
        solves,
        s.mse(),
        s.crop()
    );
    println!("rejections after refine: {:?}", s.rejections);
    if std::env::var("SQ_MICRO").is_ok() {
        use squares_core::layout::layout;
        use squares_core::solve::{Delta, Solver, Update};
        let map = s.map().clone();
        let mut pot = Vec::new();
        let t = Instant::now();
        for _ in 0..20 {
            Solver::new(&map).potentials(&mut pot);
        }
        println!(
            "micro factor+solve {:.3}ms (n={})",
            t.elapsed().as_secs_f64() * 1e3 / 20.0,
            Solver::new(&map).dimension()
        );
        let solver = Solver::new(&map);
        let e = map.live_edge_ids()[5];
        let up = Update {
            u: map.origin(2 * e),
            v: map.dest(2 * e),
            sign: -1.0,
        };
        let t = Instant::now();
        for _ in 0..200 {
            solver.updated(
                &Delta {
                    updates: vec![up],
                    split: None,
                    contract: None,
                },
                &mut pot,
            );
        }
        println!(
            "micro quick solve {:.3}ms",
            t.elapsed().as_secs_f64() * 1e3 / 200.0
        );
        let t = Instant::now();
        for _ in 0..200 {
            let _ = layout(&map, &pot, 1e-6, 1e-7);
        }
        println!(
            "micro layout {:.3}ms",
            t.elapsed().as_secs_f64() * 1e3 / 200.0
        );
        let t = Instant::now();
        for _ in 0..200 {
            let c = map.clone();
            std::hint::black_box(c);
        }
        println!(
            "micro map clone {:.3}ms",
            t.elapsed().as_secs_f64() * 1e3 / 200.0
        );
        let frame = s.target().frame(s.width());
        let t = Instant::now();
        for _ in 0..200 {
            let mut acc = 0.0;
            for sq in s.squares() {
                acc += s.target().error(sq, &frame);
            }
            std::hint::black_box(acc);
        }
        println!(
            "micro error {:.3}ms",
            t.elapsed().as_secs_f64() * 1e3 / 200.0
        );
    }
    let colors = s.colors();
    std::fs::write(
        &out,
        svg::render(s.squares(), &colors, s.width(), 0.0, None),
    )
    .unwrap();
    let png = out.replace(".svg", ".png");
    let ph = 640u32;
    let pw = (ph as f64 * s.width()).round() as u32;
    let mut img = image::RgbImage::new(pw, ph);
    for (sq, c) in s.squares().iter().zip(&colors) {
        let x0 = (sq.x * ph as f64).round() as u32;
        let y0 = (sq.y * ph as f64).round() as u32;
        let x1 = ((sq.x + sq.side) * ph as f64).round().min(pw as f64) as u32;
        let y1 = ((sq.y + sq.side) * ph as f64).round().min(ph as f64) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                img.put_pixel(x, y, image::Rgb(*c));
            }
        }
    }
    img.save(&png).unwrap();
    let sides: Vec<f64> = s
        .squares()
        .iter()
        .map(|q| q.side * s.target().height as f64)
        .collect();
    let min = sides.iter().cloned().fold(f64::MAX, f64::min);
    let max = sides.iter().cloned().fold(0.0, f64::max);
    let thin = sides.iter().filter(|&&v| v < 3.0).count();
    println!("sides px: min {:.2} max {:.1} under 3px {}", min, max, thin);
    println!("wrote {} and {}", out, png);
}
