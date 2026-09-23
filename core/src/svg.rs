use crate::geometry::Square;
use std::fmt::Write;

/// One rect per square. `gap` is a fraction of the shorter side left as
/// background between squares; zero means edge to edge.
pub fn render(
    squares: &[Square],
    colors: &[[u8; 3]],
    width: f64,
    gap: f64,
    background: Option<&str>,
) -> String {
    let unit = 1000.0;
    let w = width * unit;
    let mut out = String::with_capacity(squares.len() * 60 + 200);
    let _ = write!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {:.2} {:.2}\" shape-rendering=\"crispEdges\">",
        w, unit
    );
    if let Some(bg) = background {
        let _ = write!(
            out,
            "<rect width=\"{:.2}\" height=\"{:.2}\" fill=\"{}\"/>",
            w, unit, bg
        );
    }
    let g = gap * unit / 2.0;
    for (s, c) in squares.iter().zip(colors) {
        let _ = write!(
            out,
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"#{:02x}{:02x}{:02x}\"/>",
            s.x * unit + g,
            s.y * unit + g,
            (s.side * unit - 2.0 * g).max(0.0),
            (s.side * unit - 2.0 * g).max(0.0),
            c[0],
            c[1],
            c[2]
        );
    }
    out.push_str("</svg>");
    out
}
