use crate::map::{edge_of, Map, POLE};

/// Potentials on the map with unit resistances: source at 0, sink at 1.
/// Conjugate gradients with a Jacobi preconditioner, warm-started from the
/// values already in `pot`. Returns the iteration count.
pub fn solve(map: &Map, pot: &mut Vec<f64>, tol: f64, max_iter: usize) -> usize {
    let n = map.vertex_capacity();
    if pot.len() < n {
        pot.resize(n, 0.5);
    }
    let source = map.source();
    let sink = map.sink();
    pot[source as usize] = 0.0;
    pot[sink as usize] = 1.0;

    let mut unknown = vec![false; n];
    let mut diag = vec![0.0f64; n];
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); n];
    for v in 0..n as u32 {
        if !map.vertex_alive(v) || v == source || v == sink {
            continue;
        }
        unknown[v as usize] = true;
        for h in map.star(v) {
            if edge_of(h) == POLE {
                continue;
            }
            adj[v as usize].push(map.dest(h));
            diag[v as usize] += 1.0;
        }
    }

    let apply = |x: &[f64], out: &mut [f64]| {
        for v in 0..n {
            if !unknown[v] {
                out[v] = 0.0;
                continue;
            }
            let mut acc = diag[v] * x[v];
            for &w in &adj[v] {
                if unknown[w as usize] {
                    acc -= x[w as usize];
                }
            }
            out[v] = acc;
        }
    };

    let mut b = vec![0.0f64; n];
    for v in 0..n {
        if unknown[v] {
            for &w in &adj[v] {
                if !unknown[w as usize] {
                    b[v] += pot[w as usize];
                }
            }
        }
    }

    let mut r = vec![0.0f64; n];
    let mut ax = vec![0.0f64; n];
    apply(pot, &mut ax);
    for v in 0..n {
        r[v] = if unknown[v] { b[v] - ax[v] } else { 0.0 };
    }
    let mut z: Vec<f64> = (0..n)
        .map(|v| if unknown[v] { r[v] / diag[v] } else { 0.0 })
        .collect();
    let mut p = z.clone();
    let mut rz: f64 = (0..n).map(|v| r[v] * z[v]).sum();
    let mut ap = vec![0.0f64; n];
    let mut iters = 0;
    while iters < max_iter {
        let rmax = r.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        if rmax < tol {
            break;
        }
        apply(&p, &mut ap);
        let pap: f64 = (0..n).map(|v| p[v] * ap[v]).sum();
        if pap <= 0.0 {
            break;
        }
        let alpha = rz / pap;
        for v in 0..n {
            if unknown[v] {
                pot[v] += alpha * p[v];
                r[v] -= alpha * ap[v];
                z[v] = r[v] / diag[v];
            }
        }
        let rz_new: f64 = (0..n).map(|v| r[v] * z[v]).sum();
        let beta = rz_new / rz;
        rz = rz_new;
        for v in 0..n {
            if unknown[v] {
                p[v] = z[v] + beta * p[v];
            }
        }
        iters += 1;
    }
    iters
}
