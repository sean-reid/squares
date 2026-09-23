use crate::geometry::Square;
use crate::map::{edge_of, twin, Map, POLE};

/// Place every square from the potentials. Returns None when some square
/// would be thinner than `min_side`, when four squares meet at a point, or
/// when the up and down edges at a vertex do not form two contiguous runs,
/// which cannot happen for a valid map.
///
/// A cross is either an up boundary and a down boundary at one vertex within
/// `min_side` of each other, or two vertices at the same potential whose
/// segments abut end to end.
pub fn layout(map: &Map, pot: &[f64], min_side: f64) -> Option<(Vec<Square>, f64)> {
    let nv = map.vertex_capacity();
    let mut down: Vec<Vec<u32>> = vec![Vec::new(); nv];
    let mut up: Vec<Vec<u32>> = vec![Vec::new(); nv];
    let mut first_up: Vec<u32> = vec![u32::MAX; nv];
    let mut order: Vec<u32> = Vec::with_capacity(map.vertex_count());
    let source = map.source();
    let sink = map.sink();

    for v in 0..nv as u32 {
        if !map.vertex_alive(v) {
            continue;
        }
        order.push(v);
        let star = map.star(v);
        let pv = pot[v as usize];
        // 1 for up, 2 for down, 0 for the pole edge.
        let class: Vec<u8> = star
            .iter()
            .map(|&h| {
                if edge_of(h) == POLE {
                    0
                } else {
                    let d = pot[map.dest(h) as usize] - pv;
                    if d.abs() < min_side {
                        3
                    } else if d < 0.0 {
                        1
                    } else {
                        2
                    }
                }
            })
            .collect();
        if class.contains(&3) {
            return None;
        }
        let n = star.len();
        let mut ups = Vec::new();
        let mut downs = Vec::new();
        let mut transitions = 0;
        for i in 0..n {
            let a = class[i];
            let b = class[(i + 1) % n];
            if a != 0 && b != 0 && a != b {
                transitions += 1;
            }
            match a {
                1 => ups.push(star[i]),
                2 => downs.push(star[i]),
                _ => {}
            }
        }
        // Clockwise: up edges read left to right, down edges right to left.
        // Rotate each run so it starts at its first member.
        let start_of = |run: &[u32], cls: u8| -> usize {
            for i in 0..n {
                if class[i] == cls && class[(i + n - 1) % n] != cls {
                    return run.iter().position(|&h| h == star[i]).unwrap();
                }
            }
            0
        };
        if v == source {
            if !ups.is_empty() || downs.is_empty() {
                return None;
            }
        } else if v == sink {
            if !downs.is_empty() || ups.is_empty() {
                return None;
            }
        } else if ups.is_empty() || downs.is_empty() || transitions != 2 {
            return None;
        }
        if !ups.is_empty() {
            let s = start_of(&ups, 1);
            ups.rotate_left(s);
            first_up[v as usize] = ups[0];
            up[v as usize] = ups;
        }
        if !downs.is_empty() {
            let s = start_of(&downs, 2);
            downs.rotate_left(s);
            downs.reverse();
        }
        down[v as usize] = downs;
    }

    order.sort_by(|&a, &b| pot[a as usize].partial_cmp(&pot[b as usize]).unwrap());
    let mut left = vec![f64::NAN; nv];
    left[source as usize] = 0.0;
    let mut squares = vec![
        Square {
            id: 0,
            x: 0.0,
            y: 0.0,
            side: 0.0
        };
        map.edge_capacity()
    ];
    for &v in &order {
        let mut x = left[v as usize];
        if x.is_nan() {
            return None;
        }
        for &h in &down[v as usize] {
            let m = map.dest(h);
            let side = pot[m as usize] - pot[v as usize];
            squares[edge_of(h) as usize] = Square {
                id: edge_of(h),
                x,
                y: pot[v as usize],
                side,
            };
            if first_up[m as usize] == twin(h) {
                left[m as usize] = x;
            }
            x += side;
        }
    }
    for &v in &order {
        let ups = &up[v as usize];
        let downs = &down[v as usize];
        if ups.len() < 2 || downs.len() < 2 {
            continue;
        }
        let mut i = 0;
        let mut j = 0;
        while i + 1 < ups.len() && j + 1 < downs.len() {
            let su = &squares[edge_of(ups[i]) as usize];
            let sd = &squares[edge_of(downs[j]) as usize];
            let bu = su.x + su.side;
            let bd = sd.x + sd.side;
            if (bu - bd).abs() < min_side {
                return None;
            }
            if bu < bd {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    let mut right = vec![0.0; nv];
    for &v in &order {
        let downs = &down[v as usize];
        right[v as usize] = match downs.last() {
            Some(&h) => {
                let s = &squares[edge_of(h) as usize];
                s.x + s.side
            }
            None => {
                let s = &squares[edge_of(up[v as usize][up[v as usize].len() - 1]) as usize];
                s.x + s.side
            }
        };
    }
    for i in 0..order.len() {
        let a = order[i] as usize;
        for j in (i + 1)..order.len() {
            let b = order[j] as usize;
            if pot[b] - pot[a] >= min_side {
                break;
            }
            if (right[a] - left[b]).abs() < min_side || (right[b] - left[a]).abs() < min_side {
                return None;
            }
        }
    }
    let width: f64 = down[source as usize]
        .iter()
        .map(|&h| pot[map.dest(h) as usize])
        .sum();
    squares.retain(|s| s.id != POLE && s.side > 0.0);
    Some((squares, width))
}
