use crate::geometry::Square;
use crate::map::{edge_of, twin, Map, POLE};

/// Why a layout was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    /// The square on this edge is thinner than the floor.
    Thin(u32),
    CrossAtVertex,
    CrossBetweenVertices,
    Runs,
}

impl Reject {
    pub fn name(self) -> &'static str {
        match self {
            Reject::Thin(_) => "thin square",
            Reject::CrossAtVertex => "cross at a vertex",
            Reject::CrossBetweenVertices => "cross between vertices",
            Reject::Runs => "runs not contiguous",
        }
    }
}

/// Place every square from the potentials. Returns None when some square
/// would be thinner than `min_side`, when four squares meet at a point, or
/// when the up and down edges at a vertex do not form two contiguous runs,
/// which cannot happen for a valid map.
///
/// A cross is either an up boundary and a down boundary at one vertex within
/// `min_side` of each other, or two vertices at the same potential whose
/// segments abut end to end.
/// `cross_eps` is the coincidence tolerance for crosses; only exact ties are
/// combinatorially a cross, so it stays tiny.
pub fn layout(
    map: &Map,
    pot: &[f64],
    min_side: f64,
    cross_eps: f64,
) -> Result<(Vec<Square>, f64), Reject> {
    let nv = map.vertex_capacity();
    let source = map.source();
    let sink = map.sink();
    // Per vertex: up edges left to right and down edges left to right, as
    // ranges into two flat buffers.
    let mut up_all: Vec<u32> = Vec::with_capacity(map.edge_count());
    let mut down_all: Vec<u32> = Vec::with_capacity(map.edge_count());
    let mut up_range: Vec<(u32, u32)> = vec![(0, 0); nv];
    let mut down_range: Vec<(u32, u32)> = vec![(0, 0); nv];
    let mut order: Vec<u32> = Vec::with_capacity(map.vertex_count());
    let mut star: Vec<u32> = Vec::with_capacity(16);
    let mut class: Vec<u8> = Vec::with_capacity(16);

    for v in 0..nv as u32 {
        if !map.vertex_alive(v) {
            continue;
        }
        order.push(v);
        star.clear();
        class.clear();
        let pv = pot[v as usize];
        // 1 for up, 2 for down, 0 for the pole edge, 3 for too thin.
        for h in map.star_iter(v) {
            star.push(h);
            class.push(if edge_of(h) == POLE {
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
            });
        }
        if let Some(i) = class.iter().position(|&c| c == 3) {
            return Err(Reject::Thin(edge_of(star[i])));
        }
        let n = star.len();
        let mut n_up = 0;
        let mut n_down = 0;
        let mut transitions = 0;
        for i in 0..n {
            let a = class[i];
            let b = class[(i + 1) % n];
            if a != 0 && b != 0 && a != b {
                transitions += 1;
            }
            match a {
                1 => n_up += 1,
                2 => n_down += 1,
                _ => {}
            }
        }
        if v == source {
            if n_up != 0 || n_down == 0 {
                return Err(Reject::Runs);
            }
        } else if v == sink {
            if n_down != 0 || n_up == 0 {
                return Err(Reject::Runs);
            }
        } else if n_up == 0 || n_down == 0 || transitions != 2 {
            return Err(Reject::Runs);
        }
        // Clockwise: the up run reads left to right and the down run right
        // to left. Each run starts where its class begins.
        let run_start = |cls: u8| -> usize {
            (0..n)
                .find(|&i| class[i] == cls && class[(i + n - 1) % n] != cls)
                .unwrap_or(0)
        };
        if n_up > 0 {
            let s0 = run_start(1);
            let begin = up_all.len();
            for k in 0..n_up {
                up_all.push(star[(s0 + k) % n]);
            }
            up_range[v as usize] = (begin as u32, up_all.len() as u32);
        }
        if n_down > 0 {
            let s0 = run_start(2);
            let begin = down_all.len();
            for k in (0..n_down).rev() {
                down_all.push(star[(s0 + k) % n]);
            }
            down_range[v as usize] = (begin as u32, down_all.len() as u32);
        }
    }
    let ups = |v: u32| -> &[u32] {
        let (a, b) = up_range[v as usize];
        &up_all[a as usize..b as usize]
    };
    let downs = |v: u32| -> &[u32] {
        let (a, b) = down_range[v as usize];
        &down_all[a as usize..b as usize]
    };

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
            return Err(Reject::Runs);
        }
        for &h in downs(v) {
            let m = map.dest(h);
            let side = pot[m as usize] - pot[v as usize];
            squares[edge_of(h) as usize] = Square {
                id: edge_of(h),
                x,
                y: pot[v as usize],
                side,
            };
            if ups(m).first() == Some(&twin(h)) {
                left[m as usize] = x;
            }
            x += side;
        }
    }
    // A cross at a vertex: an up boundary meets a down boundary.
    for &v in &order {
        let u = ups(v);
        let d = downs(v);
        if u.len() < 2 || d.len() < 2 {
            continue;
        }
        let mut i = 0;
        let mut j = 0;
        while i + 1 < u.len() && j + 1 < d.len() {
            let su = &squares[edge_of(u[i]) as usize];
            let sd = &squares[edge_of(d[j]) as usize];
            let bu = su.x + su.side;
            let bd = sd.x + sd.side;
            if (bu - bd).abs() < cross_eps {
                return Err(Reject::CrossAtVertex);
            }
            if bu < bd {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    // A cross between vertices: two segments at one potential abut.
    let mut right = vec![0.0; nv];
    for &v in &order {
        let h = match downs(v).last() {
            Some(&h) => h,
            None => *ups(v).last().unwrap(),
        };
        let s = &squares[edge_of(h) as usize];
        right[v as usize] = s.x + s.side;
    }
    for (i, &oa) in order.iter().enumerate() {
        let a = oa as usize;
        for &ob in &order[i + 1..] {
            let b = ob as usize;
            if pot[b] - pot[a] >= cross_eps {
                break;
            }
            if (right[a] - left[b]).abs() < cross_eps || (right[b] - left[a]).abs() < cross_eps {
                return Err(Reject::CrossBetweenVertices);
            }
        }
    }
    let width: f64 = downs(source)
        .iter()
        .map(|&h| pot[map.dest(h) as usize])
        .sum();
    squares.retain(|s| s.id != POLE && s.side > 0.0);
    Ok((squares, width))
}
