use crate::geometry::Square;
use crate::map::Map;

/// Squares of a Bouwkamp code: sizes listed top to bottom, left to right,
/// each placed at the lowest, then leftmost, free spot of the skyline.
pub fn squares_from_bouwkamp(width: u32, sizes: &[u32]) -> Vec<(u32, u32, u32)> {
    let mut skyline = vec![0u32; width as usize];
    let mut out = Vec::with_capacity(sizes.len());
    for &s in sizes {
        let low = *skyline.iter().min().unwrap();
        let x = skyline.iter().position(|&h| h == low).unwrap();
        out.push((x as u32, low, s));
        for h in &mut skyline[x..x + s as usize] {
            *h = low + s;
        }
    }
    out
}

/// Build the map of a tiling given as integer squares. Horizontal segments
/// become vertices, squares become edges, and the clockwise order at each
/// vertex comes from the square centers.
pub fn map_from_squares(width: u32, height: u32, squares: &[(u32, u32, u32)]) -> Map {
    // Maximal horizontal segments per y coordinate.
    let mut segs: Vec<(u32, u32, u32)> = Vec::new(); // (y, x0, x1)
    let mut by_y: std::collections::BTreeMap<u32, Vec<(u32, u32)>> = Default::default();
    for &(x, y, s) in squares {
        by_y.entry(y).or_default().push((x, x + s));
        by_y.entry(y + s).or_default().push((x, x + s));
    }
    for (&y, spans) in by_y.iter_mut() {
        spans.sort();
        let mut cur = spans[0];
        for &(a, b) in &spans[1..] {
            if a <= cur.1 {
                cur.1 = cur.1.max(b);
            } else {
                segs.push((y, cur.0, cur.1));
                cur = (a, b);
            }
        }
        segs.push((y, cur.0, cur.1));
    }
    // Source first, sink second.
    segs.sort_by_key(|&(y, x0, _)| {
        (
            if y == 0 {
                0
            } else if y == height {
                1
            } else {
                2
            },
            y,
            x0,
        )
    });
    assert_eq!(segs[0], (0, 0, width));
    assert_eq!(segs[1], (height, 0, width));
    let find = |y: u32, x: u32| -> u32 {
        segs.iter()
            .position(|&(sy, a, b)| sy == y && a <= x && x < b)
            .unwrap() as u32
    };
    let mut edges: Vec<(u32, u32)> = vec![(0, 1)];
    // Angle for the clockwise sort, with y down: atan2(dy, dx) increasing.
    let mut angles: Vec<Vec<(f64, u32)>> = vec![Vec::new(); segs.len()];
    angles[0].push((-std::f64::consts::FRAC_PI_2, 0));
    angles[1].push((std::f64::consts::FRAC_PI_2, 0));
    for &(x, y, s) in squares {
        let top = find(y, x);
        let bottom = find(y + s, x);
        let e = edges.len() as u32;
        edges.push((top, bottom));
        let cx = x as f64 + s as f64 / 2.0;
        let mid_top = (segs[top as usize].1 + segs[top as usize].2) as f64 / 2.0;
        let mid_bot = (segs[bottom as usize].1 + segs[bottom as usize].2) as f64 / 2.0;
        angles[top as usize].push(((s as f64 / 2.0).atan2(cx - mid_top), e));
        angles[bottom as usize].push(((-(s as f64) / 2.0).atan2(cx - mid_bot), e));
    }
    let rotations: Vec<Vec<u32>> = angles
        .into_iter()
        .map(|mut a| {
            a.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap());
            a.into_iter().map(|(_, e)| e).collect()
        })
        .collect();
    Map::from_rotations(&edges, &rotations)
}

/// The 32 by 33 simple perfect squared rectangle of order 9.
pub fn seed_squares() -> (u32, u32, Vec<(u32, u32, u32)>) {
    (
        32,
        33,
        squares_from_bouwkamp(32, &[15, 8, 9, 7, 1, 10, 18, 4, 14]),
    )
}

pub fn seed_map() -> Map {
    let (w, h, sq) = seed_squares();
    map_from_squares(w, h, &sq)
}

pub fn scaled(squares: &[(u32, u32, u32)], height: u32) -> Vec<Square> {
    let h = height as f64;
    squares
        .iter()
        .enumerate()
        .map(|(i, &(x, y, s))| Square {
            id: i as u32 + 1,
            x: x as f64 / h,
            y: y as f64 / h,
            side: s as f64 / h,
        })
        .collect()
}
