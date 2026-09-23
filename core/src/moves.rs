use crate::map::{edge_of, twin, Map, POLE};
use crate::rng::Rng;

#[derive(Clone, Copy, Debug)]
pub enum Move {
    /// New edge between the origins of two half-edges on one face.
    Insert {
        ha: u32,
        hb: u32,
    },
    /// Split the origin of `h_start`, moving `k` clockwise half-edges away.
    Split {
        h_start: u32,
        k: u32,
    },
    Delete {
        e: u32,
    },
    Contract {
        e: u32,
    },
}

/// Apply a move. Returns false when the move would break 3-connectivity or
/// simplicity; the map is then left in that broken state and must be dropped.
pub fn apply(map: &mut Map, mv: Move) -> bool {
    match mv {
        Move::Insert { ha, hb } => {
            map.insert_edge(ha, hb);
            true
        }
        Move::Split { h_start, k } => {
            map.split_vertex(h_start, k);
            true
        }
        Move::Delete { e } => {
            if e == POLE {
                return false;
            }
            let u = map.origin(2 * e);
            let v = map.origin(2 * e + 1);
            if map.degree(u) < 4 || map.degree(v) < 4 {
                return false;
            }
            let f = map.delete_edge(e);
            map.face_is_polyhedral(f)
        }
        Move::Contract { e } => {
            if e == POLE {
                return false;
            }
            if map.face_size(map.face(2 * e)) < 4 || map.face_size(map.face(2 * e + 1)) < 4 {
                return false;
            }
            let u = map.contract_edge(e);
            map.vertex_is_polyhedral(u)
        }
    }
}

/// Which growth moves to offer. Inserting an edge raises the conductance
/// between the poles and so widens the tiling; splitting a vertex lowers it
/// and narrows the tiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grow {
    Any,
    Widen,
    Narrow,
}

/// A move that adds one square near edge `e`. Inserts only join vertices
/// whose potentials differ by at least `min_gap`, since that difference is the
/// new square's side before the rest of the tiling adjusts.
pub fn grow_near(
    map: &Map,
    pot: &[f64],
    e: u32,
    kind: Grow,
    min_gap: f64,
    rng: &mut Rng,
) -> Option<Move> {
    let h = 2 * e + rng.below(2);
    let mut options: Vec<Move> = Vec::new();
    if kind != Grow::Narrow {
        for side in [h, twin(h)] {
            let f = map.face(side);
            let n = map.face_size(f) as usize;
            if n < 4 {
                continue;
            }
            let cyc = map.cycle(f);
            let i = if rng.below(2) == 0 {
                cyc.iter().position(|&x| x == side).unwrap()
            } else {
                rng.below(n as u32) as usize
            };
            let pi = pot[map.origin(cyc[i]) as usize];
            let mut pairs: Vec<u32> = Vec::new();
            for off in 2..(n - 1) {
                let hb = cyc[(i + off) % n];
                if (pot[map.origin(hb) as usize] - pi).abs() >= min_gap {
                    pairs.push(hb);
                }
            }
            if let Some(&hb) = rng.pick(&pairs) {
                options.push(Move::Insert { ha: cyc[i], hb });
            }
        }
    }
    if kind != Grow::Widen {
        for v in [map.origin(h), map.dest(h)] {
            let d = map.degree(v) as usize;
            if d < 4 {
                continue;
            }
            let star = map.star(v);
            // Signed flow out of v along each half-edge: squares below are
            // positive, squares above negative, and the pole edge carries the
            // whole width back the other way.
            let mut flow: Vec<f64> = star
                .iter()
                .map(|&g| {
                    if edge_of(g) == POLE {
                        0.0
                    } else {
                        pot[map.dest(g) as usize] - pot[v as usize]
                    }
                })
                .collect();
            if let Some(i) = star.iter().position(|&g| edge_of(g) == POLE) {
                flow[i] = -flow.iter().sum::<f64>();
            }
            let mut runs: Vec<(u32, u32)> = Vec::new();
            for start in 0..d {
                let mut acc = 0.0;
                for k in 1..=(d - 2) {
                    acc += flow[(start + k - 1) % d];
                    if k >= 2 && acc.abs() >= min_gap {
                        runs.push((star[start], k as u32));
                    }
                }
            }
            if let Some(&(h_start, k)) = rng.pick(&runs) {
                options.push(Move::Split { h_start, k });
            }
        }
    }
    rng.pick(&options).copied()
}

/// A move that removes edge `e`, if any is possible. Contraction merges the
/// square's two segments, which barely disturbs the tiling when the square is
/// thin, so it is preferred for thin squares.
pub fn shrink(map: &Map, e: u32, rng: &mut Rng) -> Option<Move> {
    if e == POLE || !map.edge_alive(e) {
        return None;
    }
    let h = 2 * e;
    let can_delete = map.degree(map.origin(h)) >= 4 && map.degree(map.dest(h)) >= 4;
    let can_contract = map.face_size(map.face(h)) >= 4 && map.face_size(map.face(twin(h))) >= 4;
    match (can_delete, can_contract) {
        (false, false) => None,
        (true, false) => Some(Move::Delete { e }),
        (false, true) => Some(Move::Contract { e }),
        (true, true) => Some(if rng.below(4) == 0 {
            Move::Delete { e }
        } else {
            Move::Contract { e }
        }),
    }
}

/// Edge ids whose squares were created or changed by a move, for animation.
pub fn touched_edges(mv: Move, map_after: &Map) -> Vec<u32> {
    match mv {
        Move::Insert { .. } | Move::Split { .. } => vec![map_after.edge_capacity() as u32 - 1],
        Move::Delete { e } | Move::Contract { e } => vec![e],
    }
}

pub fn edge_of_half(h: u32) -> u32 {
    edge_of(h)
}
