/// A square in tiling units: the tiling is `width` wide and exactly 1 tall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Square {
    pub id: u32,
    pub x: f64,
    pub y: f64,
    pub side: f64,
}

/// Every square lies inside the rectangle, the areas sum to it, and no two
/// squares overlap. Used as a test oracle for layouts.
pub fn tiles_rectangle(squares: &[Square], width: f64, tol: f64) -> Result<(), String> {
    let mut area = 0.0;
    for s in squares {
        if s.side <= tol {
            return Err(format!("square {} has side {}", s.id, s.side));
        }
        if s.x < -tol || s.y < -tol || s.x + s.side > width + tol || s.y + s.side > 1.0 + tol {
            return Err(format!("square {} leaves the rectangle: {:?}", s.id, s));
        }
        area += s.side * s.side;
    }
    if (area - width).abs() > tol * squares.len() as f64 {
        return Err(format!("area {} differs from {}", area, width));
    }
    for (i, a) in squares.iter().enumerate() {
        for b in &squares[i + 1..] {
            let ox = (a.x + a.side).min(b.x + b.side) - a.x.max(b.x);
            let oy = (a.y + a.side).min(b.y + b.side) - a.y.max(b.y);
            if ox > tol && oy > tol {
                return Err(format!("squares {} and {} overlap", a.id, b.id));
            }
        }
    }
    Ok(())
}

/// Four squares meeting at one point.
pub fn has_cross(squares: &[Square], tol: f64) -> bool {
    let mut corners: Vec<(f64, f64)> = Vec::with_capacity(squares.len() * 4);
    for s in squares {
        corners.push((s.x, s.y));
        corners.push((s.x + s.side, s.y));
        corners.push((s.x, s.y + s.side));
        corners.push((s.x + s.side, s.y + s.side));
    }
    corners.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut i = 0;
    while i < corners.len() {
        let mut j = i + 1;
        while j < corners.len()
            && (corners[j].0 - corners[i].0).abs() <= tol
            && (corners[j].1 - corners[i].1).abs() <= tol
        {
            j += 1;
        }
        if j - i >= 4 {
            return true;
        }
        i = j;
    }
    false
}

/// A proper subset of two or more squares forming a rectangle. Brute force over
/// every candidate rectangle spanned by square edges; test oracle only.
pub fn find_subrectangle(squares: &[Square], width: f64, tol: f64) -> Option<(f64, f64, f64, f64)> {
    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for s in squares {
        xs.push(s.x);
        xs.push(s.x + s.side);
        ys.push(s.y);
        ys.push(s.y + s.side);
    }
    dedup_sorted(&mut xs, tol);
    dedup_sorted(&mut ys, tol);
    for (i, &x0) in xs.iter().enumerate() {
        for &x1 in &xs[i + 1..] {
            for (k, &y0) in ys.iter().enumerate() {
                for &y1 in &ys[k + 1..] {
                    let whole = x0 <= tol && y0 <= tol && x1 >= width - tol && y1 >= 1.0 - tol;
                    if whole {
                        continue;
                    }
                    let mut inside = 0;
                    let mut straddles = false;
                    for s in squares {
                        let sx1 = s.x + s.side;
                        let sy1 = s.y + s.side;
                        let ox = sx1.min(x1) - s.x.max(x0);
                        let oy = sy1.min(y1) - s.y.max(y0);
                        if ox <= tol || oy <= tol {
                            continue;
                        }
                        let contained = s.x >= x0 - tol
                            && sx1 <= x1 + tol
                            && s.y >= y0 - tol
                            && sy1 <= y1 + tol;
                        if contained {
                            inside += 1;
                        } else {
                            straddles = true;
                            break;
                        }
                    }
                    if !straddles && inside >= 2 {
                        return Some((x0, y0, x1, y1));
                    }
                }
            }
        }
    }
    None
}

fn dedup_sorted(v: &mut Vec<f64>, tol: f64) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v.dedup_by(|a, b| (*a - *b).abs() <= tol);
}
