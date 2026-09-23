//! Potentials on the map with unit resistances: source at 0, sink at 1.
//!
//! The reduced Laplacian over the interior vertices is factored as L D Lᵀ by
//! exact minimum-degree elimination, which produces structure and values in
//! one pass and keeps fill small on planar graphs. Adding or removing one
//! edge between existing vertices is a rank-one change, so those candidates
//! are solved from the existing factor by Sherman-Morrison (Woodbury for a
//! pair) instead of factoring again.

use crate::map::{edge_of, Map, NONE, POLE};

#[derive(Clone, Debug)]
pub struct Solver {
    /// Vertex id to dense index, NONE for dead, source, and sink.
    idx: Vec<u32>,
    verts: Vec<u32>,
    source: u32,
    sink: u32,
    /// Dense indices in elimination order.
    order: Vec<usize>,
    /// For each dense index, the below-diagonal entries of its L column as
    /// (dense index, value).
    lcols: Vec<Vec<(usize, f64)>>,
    diag: Vec<f64>,
    b: Vec<f64>,
    x: Vec<f64>,
}

impl Solver {
    pub fn new(map: &Map) -> Solver {
        let cap = map.vertex_capacity();
        let source = map.source();
        let sink = map.sink();
        let mut idx = vec![NONE; cap];
        let mut verts = Vec::with_capacity(map.vertex_count());
        for v in 0..cap as u32 {
            if map.vertex_alive(v) && v != source && v != sink {
                idx[v as usize] = verts.len() as u32;
                verts.push(v);
            }
        }
        let n = verts.len();
        let mut rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        let mut diag = vec![0.0; n];
        let mut b = vec![0.0; n];
        for (i, &v) in verts.iter().enumerate() {
            for h in map.star_iter(v) {
                if edge_of(h) == POLE {
                    continue;
                }
                diag[i] += 1.0;
                let w = map.dest(h);
                if w == sink {
                    b[i] += 1.0;
                } else if w != source {
                    let j = idx[w as usize] as usize;
                    match rows[i].iter_mut().find(|e| e.0 == j) {
                        Some(e) => e.1 -= 1.0,
                        None => rows[i].push((j, -1.0)),
                    }
                }
            }
        }
        let (order, lcols, diag) = factor(rows, diag);
        let mut s = Solver {
            idx,
            verts,
            source,
            sink,
            order,
            lcols,
            diag,
            b,
            x: Vec::new(),
        };
        s.x = s.apply_inverse(&s.b);
        s
    }

    pub fn dimension(&self) -> usize {
        self.verts.len()
    }

    /// A⁻¹ y for a dense right-hand side.
    fn apply_inverse(&self, y: &[f64]) -> Vec<f64> {
        let mut z = y.to_vec();
        for &k in &self.order {
            let zk = z[k];
            if zk != 0.0 {
                for &(i, l) in &self.lcols[k] {
                    z[i] -= l * zk;
                }
            }
        }
        for (zk, d) in z.iter_mut().zip(&self.diag) {
            *zk /= d;
        }
        for &k in self.order.iter().rev() {
            let mut acc = z[k];
            for &(i, l) in &self.lcols[k] {
                acc -= l * z[i];
            }
            z[k] = acc;
        }
        z
    }

    fn write(&self, x: &[f64], pot: &mut Vec<f64>) {
        if pot.len() < self.idx.len() {
            pot.resize(self.idx.len(), 0.5);
        }
        pot[self.source as usize] = 0.0;
        pot[self.sink as usize] = 1.0;
        for (i, &v) in self.verts.iter().enumerate() {
            pot[v as usize] = x[i];
        }
    }

    /// Potentials of the factored map.
    pub fn potentials(&self, pot: &mut Vec<f64>) {
        self.write(&self.x, pot);
    }

    /// Potentials of a map that differs from the factored one by `delta`,
    /// all solved from the existing factor. None when the change cannot be
    /// expressed: a pole edge moving, a pole merged, a second new vertex, or
    /// a singular update, which a 3-connected result never gives.
    ///
    /// Edge changes and the block of a split are rank-one terms handled by
    /// Woodbury. A contraction is the removal of the contracted edge plus the
    /// constraint that its two ends share a potential, applied as a Lagrange
    /// correction. A split adds one unknown, eliminated through its Schur
    /// complement.
    pub fn updated(&self, delta: &Delta, pot: &mut Vec<f64>) -> Option<()> {
        let n = self.verts.len();
        let mut terms: Vec<Term> = Vec::new();
        let mut b = self.b.clone();
        for up in &delta.updates {
            if delta.touches_new_or_gone(up.u) || delta.touches_new_or_gone(up.v) {
                return None;
            }
            let term = self.edge_term(up.u, up.v, up.sign)?;
            if let Some((i, t)) = term.rhs {
                b[i] += up.sign * t;
            }
            terms.push(term);
        }
        let mut constraint: Option<(usize, usize)> = None;
        if let Some(c) = &delta.contract {
            if c.keep == self.source
                || c.keep == self.sink
                || c.gone == self.source
                || c.gone == self.sink
            {
                return None;
            }
            if delta
                .split
                .as_ref()
                .is_some_and(|s| s.v2 == c.keep || s.v2 == c.gone)
            {
                return None;
            }
            terms.push(self.edge_term(c.keep, c.gone, -1.0)?);
            constraint = Some((self.dense(c.keep)?, self.dense(c.gone)?));
        }
        // The split's new unknown: coupled old vertices, its diagonal, and its
        // right-hand side.
        let mut border: Option<(Vec<usize>, f64, f64)> = None;
        let mut pole_swap = false;
        if let Some(sp) = &delta.split {
            if delta.contract.is_some_and(|c| c.gone == sp.v) {
                return None;
            }
            let fixed_v = sp.v == self.source || sp.v == self.sink;
            let tv = if sp.v == self.sink { 1.0 } else { 0.0 };
            let mut coupled = Vec::new();
            let mut b2 = 0.0;
            let mut d = sp.moved.len() as f64 + 1.0;
            let other_pole = if sp.v == self.sink {
                self.source
            } else {
                self.sink
            };
            if fixed_v && sp.moved.contains(&other_pole) {
                debug_assert!(sp.pole_swap);
                // The pole edge moved: v2 is the new pole at the same
                // potential and v becomes the new unknown, coupled to the
                // neighbors it kept. Moved neighbors see no change.
                pole_swap = true;
                d = sp.kept.len() as f64 + 1.0;
                b2 += tv;
                for &n in &sp.kept {
                    if n == self.source || n == self.sink || delta.touches_new_or_gone(n) {
                        return None;
                    }
                    let n = self.dense(n)?;
                    b[n] -= tv;
                    coupled.push(n);
                }
            } else if fixed_v {
                // v stays a pole; v2 is a new interior vertex joined to it.
                b2 += tv;
                for &m in &sp.moved {
                    if m == self.source || m == self.sink || delta.touches_new_or_gone(m) {
                        return None;
                    }
                    let m = self.dense(m)?;
                    b[m] -= tv;
                    coupled.push(m);
                }
            } else {
                let v = self.dense(sp.v)?;
                coupled.push(v);
                for &m in &sp.moved {
                    if delta.touches_new_or_gone(m) {
                        return None;
                    }
                    if m == self.sink || m == self.source {
                        let t = if m == self.sink { 1.0 } else { 0.0 };
                        terms.push(Term::diag(v, -1.0));
                        b[v] -= t;
                        b2 += t;
                    } else {
                        let m = self.dense(m)?;
                        terms.push(Term::diag(m, 1.0));
                        terms.push(Term::pair(v, m, -1.0));
                        coupled.push(m);
                    }
                }
                terms.push(Term::diag(v, 1.0));
            }
            border = Some((coupled, d, b2));
        }
        let k = terms.len();
        let zs: Vec<Vec<f64>> = terms
            .iter()
            .map(|t| {
                let mut y = vec![0.0; n];
                for &(i, c) in &t.w {
                    y[i] = c;
                }
                self.apply_inverse(&y)
            })
            .collect();
        let mut m = vec![0.0f64; k * k];
        for i in 0..k {
            for j in 0..k {
                m[i * k + j] = dot(&terms[i].w, &zs[j]);
            }
            m[i * k + i] += 1.0 / terms[i].sign;
        }
        let lu = Lu::new(m, k)?;
        let woodbury = |y: &[f64]| -> Vec<f64> {
            let mut x = self.apply_inverse(y);
            if k > 0 {
                let r: Vec<f64> = terms.iter().map(|t| dot(&t.w, &x)).collect();
                let coef = lu.solve(&r);
                for (zi, &c) in zs.iter().zip(&coef) {
                    for j in 0..n {
                        x[j] -= zi[j] * c;
                    }
                }
            }
            x
        };
        // With a contraction, every solve is projected onto x_keep = x_gone.
        let zc = constraint.map(|(a, g)| {
            let mut y = vec![0.0; n];
            y[a] = 1.0;
            y[g] = -1.0;
            woodbury(&y)
        });
        let apply = |y: &[f64]| -> Option<Vec<f64>> {
            let mut x = woodbury(y);
            if let (Some((a, g)), Some(z)) = (constraint, &zc) {
                let denom = z[a] - z[g];
                if denom.abs() < 1e-12 {
                    return None;
                }
                let lambda = (x[a] - x[g]) / denom;
                for j in 0..n {
                    x[j] -= z[j] * lambda;
                }
            }
            Some(x)
        };
        let mut x = apply(&b)?;
        let mut x2 = None;
        if let Some((coupled, d, b2)) = &border {
            let mut c = vec![0.0; n];
            for &i in coupled {
                c[i] = -1.0;
            }
            let q = apply(&c)?;
            let ctp: f64 = coupled.iter().map(|&i| -x[i]).sum();
            let ctq: f64 = coupled.iter().map(|&i| -q[i]).sum();
            let denom = d - ctq;
            if denom.abs() < 1e-12 {
                return None;
            }
            let xv2 = (b2 - ctp) / denom;
            for j in 0..n {
                x[j] -= q[j] * xv2;
            }
            x2 = Some(xv2);
        }
        self.write(&x, pot);
        if let (Some(sp), Some(xv2)) = (&delta.split, x2) {
            if pot.len() <= sp.v2 as usize {
                pot.resize(sp.v2 as usize + 1, 0.5);
            }
            if pole_swap {
                pot[sp.v2 as usize] = if sp.v == self.sink { 1.0 } else { 0.0 };
                pot[sp.v as usize] = xv2;
            } else {
                pot[sp.v2 as usize] = xv2;
            }
        }
        Some(())
    }

    fn dense(&self, v: u32) -> Option<usize> {
        let i = *self.idx.get(v as usize)?;
        if i == NONE {
            None
        } else {
            Some(i as usize)
        }
    }

    /// The rank-one term for adding (`sign` 1) or removing (`sign` -1) an
    /// edge between two vertices of the factored map.
    fn edge_term(&self, u: u32, v: u32, sign: f64) -> Option<Term> {
        let mut w = Vec::new();
        let mut rhs = None;
        let mut t = 0.0;
        for (vert, coef) in [(u, 1.0), (v, -1.0)] {
            if vert == self.sink {
                t = 1.0;
            } else if vert != self.source {
                w.push((self.dense(vert)?, coef));
            }
        }
        if w.is_empty() {
            return None;
        }
        if w.len() == 1 && t != 0.0 {
            // The interior endpoint's coefficient is ±1; the rhs gains the
            // fixed potential with that coefficient squared, so just t.
            rhs = Some((w[0].0, t));
            w[0].1 = 1.0;
        }
        Some(Term { w, sign, rhs })
    }
}

/// One edge added between two vertices of the factored map (`sign` 1) or
/// removed (`sign` -1).
#[derive(Clone, Copy, Debug)]
pub struct Update {
    pub u: u32,
    pub v: u32,
    pub sign: f64,
}

/// A vertex split: `moved` lists the far endpoints of the edges that left
/// `v` for the new vertex `v2`, which is also joined to `v`; `kept` lists the
/// far endpoints of the edges that stayed.
#[derive(Clone, Debug)]
pub struct Split {
    pub v: u32,
    pub v2: u32,
    pub moved: Vec<u32>,
    pub kept: Vec<u32>,
    /// The pole edge moved with the run, so `v2` is the new pole and `v`
    /// became an interior vertex.
    pub pole_swap: bool,
}

/// A contraction: `gone` merged into `keep`, the edge between them removed.
#[derive(Clone, Copy, Debug)]
pub struct Contract {
    pub keep: u32,
    pub gone: u32,
}

/// Everything that separates a candidate map from the factored one: edge
/// changes among its vertices, at most one split, at most one contraction.
#[derive(Clone, Debug, Default)]
pub struct Delta {
    pub updates: Vec<Update>,
    pub split: Option<Split>,
    pub contract: Option<Contract>,
}

impl Delta {
    /// Whether `more` can be appended and still be answered from the same
    /// factor: one split, one contraction, and no reference to a vertex this
    /// delta created or removed.
    pub fn can_take(&self, more: &Delta) -> bool {
        if more.split.is_some() && self.split.is_some() {
            return false;
        }
        if more.contract.is_some() && self.contract.is_some() {
            return false;
        }
        let fresh = |v: u32| self.touches_new_or_gone(v);
        if more.updates.iter().any(|u| fresh(u.u) || fresh(u.v)) {
            return false;
        }
        if let Some(sp) = &more.split {
            if fresh(sp.v) || sp.moved.iter().any(|&m| fresh(m)) {
                return false;
            }
        }
        if let Some(c) = &more.contract {
            if fresh(c.keep) || fresh(c.gone) {
                return false;
            }
        }
        true
    }

    /// Whether `v` is a vertex the factored map never had or no longer has.
    fn touches_new_or_gone(&self, v: u32) -> bool {
        self.split
            .as_ref()
            .is_some_and(|s| s.v2 == v || (s.pole_swap && s.v == v))
            || self.contract.is_some_and(|c| c.gone == v)
    }
}

struct Term {
    w: Vec<(usize, f64)>,
    sign: f64,
    /// (dense index, fixed potential) when the edge touches a pole.
    rhs: Option<(usize, f64)>,
}

impl Term {
    fn diag(i: usize, sign: f64) -> Term {
        Term {
            w: vec![(i, 1.0)],
            sign,
            rhs: None,
        }
    }

    fn pair(i: usize, j: usize, sign: f64) -> Term {
        Term {
            w: vec![(i, 1.0), (j, -1.0)],
            sign,
            rhs: None,
        }
    }
}

fn dot(w: &[(usize, f64)], z: &[f64]) -> f64 {
    w.iter().map(|&(i, c)| c * z[i]).sum()
}

/// Dense LU with partial pivoting for the small Woodbury matrix.
struct Lu {
    a: Vec<f64>,
    piv: Vec<usize>,
    k: usize,
}

impl Lu {
    fn new(mut a: Vec<f64>, k: usize) -> Option<Lu> {
        let mut piv: Vec<usize> = (0..k).collect();
        for c in 0..k {
            let mut best = c;
            for r in c + 1..k {
                if a[r * k + c].abs() > a[best * k + c].abs() {
                    best = r;
                }
            }
            if a[best * k + c].abs() < 1e-12 {
                return None;
            }
            if best != c {
                for j in 0..k {
                    a.swap(c * k + j, best * k + j);
                }
                piv.swap(c, best);
            }
            for r in c + 1..k {
                let f = a[r * k + c] / a[c * k + c];
                a[r * k + c] = f;
                for j in c + 1..k {
                    a[r * k + j] -= f * a[c * k + j];
                }
            }
        }
        Some(Lu { a, piv, k })
    }

    fn solve(&self, r: &[f64]) -> Vec<f64> {
        let k = self.k;
        let mut y: Vec<f64> = self.piv.iter().map(|&p| r[p]).collect();
        for i in 0..k {
            for j in 0..i {
                y[i] -= self.a[i * k + j] * y[j];
            }
        }
        for i in (0..k).rev() {
            for j in i + 1..k {
                y[i] -= self.a[i * k + j] * y[j];
            }
            y[i] /= self.a[i * k + i];
        }
        y
    }
}

/// Exact minimum-degree L D Lᵀ elimination on a symmetric matrix given as
/// off-diagonal rows plus a diagonal. Returns the elimination order, the L
/// columns, and the pivots.
#[allow(clippy::type_complexity)]
fn factor(
    mut rows: Vec<Vec<(usize, f64)>>,
    mut diag: Vec<f64>,
) -> (Vec<usize>, Vec<Vec<(usize, f64)>>, Vec<f64>) {
    let n = rows.len();
    let mut done = vec![false; n];
    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); n + 1];
    for i in 0..n {
        buckets[rows[i].len()].push(i);
    }
    let mut lcols: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    let mut order = Vec::with_capacity(n);
    let mut low = 0;
    for _ in 0..n {
        let k = loop {
            while low < buckets.len() && buckets[low].is_empty() {
                low += 1;
            }
            let cand = buckets[low].pop().unwrap();
            if !done[cand] && rows[cand].len() == low {
                break cand;
            }
        };
        done[k] = true;
        order.push(k);
        let d = diag[k];
        let nbrs = std::mem::take(&mut rows[k]);
        let mut col = Vec::with_capacity(nbrs.len());
        for &(i, a) in &nbrs {
            col.push((i, a / d));
        }
        for (p, &(i, ai)) in nbrs.iter().enumerate() {
            let row_i = &mut rows[i];
            if let Some(q) = row_i.iter().position(|e| e.0 == k) {
                row_i.swap_remove(q);
            }
            diag[i] -= ai * ai / d;
            for &(j, aj) in &nbrs[p + 1..] {
                let delta = -ai * aj / d;
                add_entry(&mut rows[i], j, delta);
                add_entry(&mut rows[j], i, delta);
            }
        }
        for &(i, _) in &nbrs {
            let deg = rows[i].len();
            buckets[deg].push(i);
            if deg < low {
                low = deg;
            }
        }
        lcols[k] = col;
    }
    (order, lcols, diag)
}

fn add_entry(row: &mut Vec<(usize, f64)>, j: usize, delta: f64) {
    match row.iter_mut().find(|e| e.0 == j) {
        Some(e) => e.1 += delta,
        None => row.push((j, delta)),
    }
}

/// Solve the whole map from scratch and write the potentials.
pub fn solve(map: &Map, pot: &mut Vec<f64>) {
    Solver::new(map).potentials(pot);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moves::{apply, Move};
    use crate::rng::Rng;
    use crate::seed::seed_map;

    fn residual(map: &Map, pot: &[f64]) -> f64 {
        let mut worst = 0.0f64;
        for v in map.live_vertex_ids() {
            if v == map.source() || v == map.sink() {
                continue;
            }
            let mut acc = 0.0;
            for h in map.star(v) {
                if edge_of(h) != POLE {
                    acc += pot[map.dest(h) as usize] - pot[v as usize];
                }
            }
            worst = worst.max(acc.abs());
        }
        worst
    }

    #[test]
    fn seed_potentials_satisfy_kirchhoff() {
        let map = seed_map();
        let mut pot = Vec::new();
        solve(&map, &mut pot);
        assert!(residual(&map, &pot) < 1e-12);
        assert!((pot[map.source() as usize]).abs() < 1e-15);
        assert!((pot[map.sink() as usize] - 1.0).abs() < 1e-15);
    }

    #[test]
    fn rank_one_updates_match_a_fresh_factorization() {
        let mut rng = Rng::new(3);
        let mut map = seed_map();
        let mut pot = Vec::new();
        let mut checked = 0;
        for _ in 0..60 {
            solve(&map, &mut pot);
            let base = Solver::new(&map);
            let live = map.live_edge_ids();
            let e = live[1 + rng.below(live.len() as u32 - 1) as usize];
            let mv = crate::moves::grow_near(&map, &pot, e, crate::moves::Grow::Any, 0.0, &mut rng);
            let Some(mv) = mv else { continue };
            let mut next = map.clone();
            apply(&mut next, mv);
            if let Move::Insert { ha, hb } = mv {
                let up = Update {
                    u: map.origin(ha),
                    v: map.origin(hb),
                    sign: 1.0,
                };
                let mut fast = Vec::new();
                base.updated(
                    &Delta {
                        updates: vec![up],
                        split: None,
                        contract: None,
                    },
                    &mut fast,
                )
                .unwrap();
                let mut exact = Vec::new();
                solve(&next, &mut exact);
                for v in next.live_vertex_ids() {
                    assert!(
                        (fast[v as usize] - exact[v as usize]).abs() < 1e-9,
                        "vertex {}",
                        v
                    );
                }
                checked += 1;
            }
            map = next;
        }
        assert!(checked > 10, "checked {}", checked);
    }

    #[test]
    fn a_split_matches_a_fresh_factorization() {
        let mut rng = Rng::new(5);
        let mut map = seed_map();
        let mut pot = Vec::new();
        for _ in 0..40 {
            solve(&map, &mut pot);
            let live = map.live_edge_ids();
            let e = live[1 + rng.below(live.len() as u32 - 1) as usize];
            if let Some(mv) =
                crate::moves::grow_near(&map, &pot, e, crate::moves::Grow::Any, 0.0, &mut rng)
            {
                apply(&mut map, mv);
            }
        }
        solve(&map, &mut pot);
        let base = Solver::new(&map);
        let mut checked = 0;
        for _ in 0..200 {
            let live = map.live_edge_ids();
            let e = live[1 + rng.below(live.len() as u32 - 1) as usize];
            let Some(mv @ Move::Split { h_start, k }) =
                crate::moves::grow_near(&map, &pot, e, crate::moves::Grow::Narrow, 0.0, &mut rng)
            else {
                continue;
            };
            let v = map.origin(h_start);
            let mut moved = Vec::new();
            let mut h = h_start;
            for _ in 0..k {
                moved.push(map.dest(h));
                h = map.rot(h);
            }
            let mut kept = Vec::new();
            while h != h_start {
                kept.push(map.dest(h));
                h = map.rot(h);
            }
            let mut next = map.clone();
            apply(&mut next, mv);
            let v2 = (0..next.vertex_capacity() as u32)
                .find(|&x| {
                    next.vertex_alive(x)
                        && (x as usize >= map.vertex_capacity() || !map.vertex_alive(x))
                })
                .unwrap();
            let mut fast = Vec::new();
            base.updated(
                &Delta {
                    updates: vec![],
                    split: Some(Split {
                        v,
                        v2,
                        moved: moved.clone(),
                        kept,
                        pole_swap: (v == map.source() || v == map.sink())
                            && (moved.contains(&map.source()) || moved.contains(&map.sink())),
                    }),
                    contract: None,
                },
                &mut fast,
            )
            .unwrap();
            let mut exact = Vec::new();
            solve(&next, &mut exact);
            for w in next.live_vertex_ids() {
                assert!(
                    (fast[w as usize] - exact[w as usize]).abs() < 1e-9,
                    "vertex {} {} vs {}",
                    w,
                    fast[w as usize],
                    exact[w as usize]
                );
            }
            checked += 1;
            if checked >= 10 {
                break;
            }
        }
        assert!(checked >= 5, "checked {}", checked);
    }

    #[test]
    fn a_contraction_matches_a_fresh_factorization() {
        let mut rng = Rng::new(9);
        let mut map = seed_map();
        let mut pot = Vec::new();
        for _ in 0..40 {
            solve(&map, &mut pot);
            let live = map.live_edge_ids();
            let e = live[1 + rng.below(live.len() as u32 - 1) as usize];
            if let Some(mv) =
                crate::moves::grow_near(&map, &pot, e, crate::moves::Grow::Any, 0.0, &mut rng)
            {
                apply(&mut map, mv);
            }
        }
        solve(&map, &mut pot);
        let base = Solver::new(&map);
        let mut checked = 0;
        for e in map.live_edge_ids().into_iter().skip(1) {
            let (keep, gone) = (map.origin(2 * e), map.dest(2 * e));
            if [keep, gone]
                .iter()
                .any(|&x| x == map.source() || x == map.sink())
            {
                continue;
            }
            let mut next = map.clone();
            if map.face_size(map.face(2 * e)) < 4 || map.face_size(map.face(2 * e + 1)) < 4 {
                continue;
            }
            if !apply(&mut next, Move::Contract { e }) {
                continue;
            }
            let mut fast = Vec::new();
            base.updated(
                &Delta {
                    updates: vec![],
                    split: None,
                    contract: Some(Contract { keep, gone }),
                },
                &mut fast,
            )
            .unwrap();
            let mut exact = Vec::new();
            solve(&next, &mut exact);
            for w in next.live_vertex_ids() {
                assert!(
                    (fast[w as usize] - exact[w as usize]).abs() < 1e-9,
                    "vertex {}",
                    w
                );
            }
            checked += 1;
            if checked >= 8 {
                break;
            }
        }
        assert!(checked >= 3, "checked {}", checked);
    }

    #[test]
    fn a_delete_and_an_insert_together_match() {
        let mut rng = Rng::new(11);
        let mut map = seed_map();
        let mut pot = Vec::new();
        for _ in 0..30 {
            solve(&map, &mut pot);
            let live = map.live_edge_ids();
            let e = live[1 + rng.below(live.len() as u32 - 1) as usize];
            if let Some(mv) =
                crate::moves::grow_near(&map, &pot, e, crate::moves::Grow::Any, 0.0, &mut rng)
            {
                apply(&mut map, mv);
            }
        }
        solve(&map, &mut pot);
        let base = Solver::new(&map);
        let mut checked = 0;
        for e in map.live_edge_ids().into_iter().skip(1) {
            let h = 2 * e;
            let (u, v) = (map.origin(h), map.dest(h));
            if map.degree(u) < 4 || map.degree(v) < 4 {
                continue;
            }
            let mut next = map.clone();
            if !apply(&mut next, Move::Delete { e }) {
                continue;
            }
            let live = next.live_edge_ids();
            let e2 = live[1 + rng.below(live.len() as u32 - 1) as usize];
            let Some(Move::Insert { ha, hb }) =
                crate::moves::grow_near(&next, &pot, e2, crate::moves::Grow::Widen, 0.0, &mut rng)
            else {
                continue;
            };
            let ins = Update {
                u: next.origin(ha),
                v: next.origin(hb),
                sign: 1.0,
            };
            apply(&mut next, Move::Insert { ha, hb });
            let mut fast = Vec::new();
            base.updated(
                &Delta {
                    updates: vec![Update { u, v, sign: -1.0 }, ins],
                    split: None,
                    contract: None,
                },
                &mut fast,
            )
            .unwrap();
            let mut exact = Vec::new();
            solve(&next, &mut exact);
            for w in next.live_vertex_ids() {
                assert!(
                    (fast[w as usize] - exact[w as usize]).abs() < 1e-9,
                    "vertex {}",
                    w
                );
            }
            checked += 1;
            if checked >= 8 {
                break;
            }
        }
        assert!(checked >= 3, "checked {}", checked);
    }
}

#[cfg(test)]
mod pole_split_tests {
    use super::*;
    use crate::moves::{apply, Move};
    use crate::rng::Rng;
    use crate::seed::seed_map;

    #[test]
    fn interior_split_moving_a_pole_edge_matches() {
        let mut rng = Rng::new(21);
        let mut map = seed_map();
        let mut pot = Vec::new();
        for _ in 0..30 {
            solve(&map, &mut pot);
            let live = map.live_edge_ids();
            let e = live[1 + rng.below(live.len() as u32 - 1) as usize];
            if let Some(mv) =
                crate::moves::grow_near(&map, &pot, e, crate::moves::Grow::Any, 0.0, &mut rng)
            {
                apply(&mut map, mv);
            }
        }
        solve(&map, &mut pot);
        let base = Solver::new(&map);
        let mut checked = 0;
        for v in map.live_vertex_ids() {
            if v == map.source() || v == map.sink() || map.degree(v) < 4 {
                continue;
            }
            let star = map.star(v);
            let d = star.len();
            for start in 0..d {
                for k in 2..=(d - 2) {
                    let run: Vec<u32> = (0..k).map(|i| star[(start + i) % d]).collect();
                    let moved: Vec<u32> = run.iter().map(|&h| map.dest(h)).collect();
                    if !moved.iter().any(|&m| m == map.source() || m == map.sink()) {
                        continue;
                    }
                    let mv = Move::Split {
                        h_start: run[0],
                        k: k as u32,
                    };
                    let mut next = map.clone();
                    apply(&mut next, mv);
                    let v2 = (0..next.vertex_capacity() as u32)
                        .find(|&x| {
                            next.vertex_alive(x)
                                && (x as usize >= map.vertex_capacity() || !map.vertex_alive(x))
                        })
                        .unwrap();
                    let kept: Vec<u32> = star
                        .iter()
                        .filter(|h| !run.contains(h))
                        .map(|&h| map.dest(h))
                        .collect();
                    let mut fast = Vec::new();
                    base.updated(
                        &Delta {
                            updates: vec![],
                            split: Some(Split {
                                v,
                                v2,
                                moved: moved.clone(),
                                kept,
                                pole_swap: false,
                            }),
                            contract: None,
                        },
                        &mut fast,
                    )
                    .unwrap();
                    let mut exact = Vec::new();
                    solve(&next, &mut exact);
                    let drift = next
                        .live_vertex_ids()
                        .iter()
                        .map(|&w| (fast[w as usize] - exact[w as usize]).abs())
                        .fold(0.0, f64::max);
                    assert!(
                        drift < 1e-9,
                        "v {} moved {:?} drift {:.3e} (fast v2 {:.4} exact v2 {:.4})",
                        v,
                        moved,
                        drift,
                        fast[v2 as usize],
                        exact[v2 as usize]
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 5, "checked {}", checked);
    }
}

#[cfg(test)]
mod accumulated_tests {
    use super::*;
    use crate::moves::{apply, grow_near, Grow, Move};
    use crate::rng::Rng;
    use crate::seed::seed_map;

    fn new_vertex(before: &Map, after: &Map) -> u32 {
        (0..after.vertex_capacity() as u32)
            .rev()
            .find(|&x| {
                after.vertex_alive(x)
                    && (x as usize >= before.vertex_capacity() || !before.vertex_alive(x))
            })
            .unwrap()
    }

    #[test]
    fn random_accumulated_deltas_match() {
        let mut failures = Vec::new();
        for seed in 0..300u64 {
            let mut rng = Rng::new(seed);
            let mut map = seed_map();
            let mut pot = Vec::new();
            for _ in 0..25 {
                solve(&map, &mut pot);
                let live = map.live_edge_ids();
                let e = live[1 + rng.below(live.len() as u32 - 1) as usize];
                if let Some(mv) = grow_near(&map, &pot, e, Grow::Any, 0.0, &mut rng) {
                    apply(&mut map, mv);
                }
            }
            solve(&map, &mut pot);
            let base = Solver::new(&map);
            let mut delta = Delta::default();
            let mut cur = map.clone();
            let mut applied: Vec<Move> = Vec::new();
            for _ in 0..6 {
                solve(&cur, &mut pot);
                let live = cur.live_edge_ids();
                let e = live[1 + rng.below(live.len() as u32 - 1) as usize];
                let Some(mv) = grow_near(&cur, &pot, e, Grow::Any, 0.0, &mut rng) else {
                    continue;
                };
                let before = cur.clone();
                match mv {
                    Move::Insert { ha, hb } => {
                        let (u, v) = (cur.origin(ha), cur.origin(hb));
                        if delta.touches_new_or_gone(u) || delta.touches_new_or_gone(v) {
                            continue;
                        }
                        apply(&mut cur, mv);
                        delta.updates.push(Update { u, v, sign: 1.0 });
                    }
                    Move::Split { h_start, k } => {
                        if delta.split.is_some() {
                            continue;
                        }
                        let v = cur.origin(h_start);
                        let mut moved = Vec::new();
                        let mut run = Vec::new();
                        let mut h = h_start;
                        for _ in 0..k {
                            moved.push(cur.dest(h));
                            run.push(h);
                            h = cur.rot(h);
                        }
                        let kept: Vec<u32> = cur
                            .star(v)
                            .iter()
                            .filter(|h| !run.contains(h))
                            .map(|&h| cur.dest(h))
                            .collect();
                        apply(&mut cur, mv);
                        let v2 = new_vertex(&before, &cur);
                        delta.split = Some(Split {
                            v,
                            v2,
                            moved: moved.clone(),
                            kept,
                            pole_swap: (v == map.source() || v == map.sink())
                                && (moved.contains(&map.source()) || moved.contains(&map.sink())),
                        });
                    }
                    _ => continue,
                }
                applied.push(mv);
                let mut fast = Vec::new();
                if base.updated(&delta, &mut fast).is_none() {
                    break;
                }
                let mut exact = Vec::new();
                solve(&cur, &mut exact);
                let drift = cur
                    .live_vertex_ids()
                    .iter()
                    .map(|&w| (fast[w as usize] - exact[w as usize]).abs())
                    .fold(0.0, f64::max);
                if drift > 1e-9 {
                    failures.push(format!(
                        "seed {} drift {:.2e} src {} sink {} moves {:?} delta {:?}",
                        seed,
                        drift,
                        map.source(),
                        map.sink(),
                        applied,
                        delta
                    ));
                    break;
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} failures, first: {}",
            failures.len(),
            failures[0]
        );
    }
}
