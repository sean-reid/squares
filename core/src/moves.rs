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

/// A move that adds one square near edge `e`.
pub fn grow_near(map: &Map, e: u32, rng: &mut Rng) -> Option<Move> {
    let h = 2 * e + rng.below(2);
    let mut options: Vec<Move> = Vec::new();
    for side in [h, twin(h)] {
        let f = map.face(side);
        let n = map.face_size(f);
        if n < 4 {
            continue;
        }
        let cyc = map.cycle(f);
        let i = if rng.below(2) == 0 {
            cyc.iter().position(|&x| x == side).unwrap()
        } else {
            rng.below(n) as usize
        };
        // Skip i and both neighbors of i on the cycle.
        let off = 2 + rng.below(n - 3) as usize;
        let j = (i + off) % n as usize;
        options.push(Move::Insert {
            ha: cyc[i],
            hb: cyc[j],
        });
    }
    for v in [map.origin(h), map.dest(h)] {
        let d = map.degree(v);
        if d < 4 {
            continue;
        }
        let star = map.star(v);
        let h_start = star[rng.below(d) as usize];
        let k = 2 + rng.below(d - 3);
        options.push(Move::Split { h_start, k });
    }
    rng.pick(&options).copied()
}

/// A move that removes edge `e`, if any is possible.
pub fn shrink(map: &Map, e: u32, rng: &mut Rng) -> Option<Move> {
    if e == POLE || !map.edge_alive(e) {
        return None;
    }
    let mut options: Vec<Move> = Vec::new();
    let h = 2 * e;
    if map.degree(map.origin(h)) >= 4 && map.degree(map.dest(h)) >= 4 {
        options.push(Move::Delete { e });
    }
    if map.face_size(map.face(h)) >= 4 && map.face_size(map.face(twin(h))) >= 4 {
        options.push(Move::Contract { e });
    }
    rng.pick(&options).copied()
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
