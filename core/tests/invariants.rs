use squares_core::geometry::{find_subrectangle, has_cross, tiles_rectangle};
use squares_core::layout::layout;
use squares_core::map::Map;
use squares_core::moves::{apply, grow_near, shrink};
use squares_core::rng::Rng;
use squares_core::seed::{scaled, seed_map, seed_squares};
use squares_core::solve::solve;

const TOL: f64 = 1e-9;

fn solved(map: &Map) -> Vec<f64> {
    let mut pot = vec![0.5; map.vertex_capacity()];
    solve(map, &mut pot, 1e-13, 100_000);
    pot
}

fn check_all(map: &mut Map, context: &str) -> bool {
    map.is_polyhedral()
        .unwrap_or_else(|e| panic!("{}: {}", context, e));
    assert!(map.is_three_connected(), "{}: not 3-connected", context);
    let pot = solved(map);
    let Some((squares, width)) = layout(map, &pot, 1e-9) else {
        return false;
    };
    tiles_rectangle(&squares, width, 1e-7).unwrap_or_else(|e| panic!("{}: {}", context, e));
    assert!(!has_cross(&squares, 1e-7), "{}: cross", context);
    if squares.len() <= 40 {
        assert!(
            find_subrectangle(&squares, width, 1e-7).is_none(),
            "{}: subrectangle",
            context
        );
    }
    true
}

#[test]
fn seed_round_trips_through_solve_and_layout() {
    let mut map = seed_map();
    assert_eq!(map.edge_count(), 10);
    assert_eq!(map.vertex_count(), 6);
    check_all(&mut map, "seed");
    let pot = solved(&map);
    let (squares, width) = layout(&map, &pot, 1e-9).unwrap();
    assert!((width - 32.0 / 33.0).abs() < TOL);
    let (_, h, sq) = seed_squares();
    let expected = scaled(&sq, h);
    for e in &expected {
        let got = squares.iter().find(|s| s.id == e.id).unwrap();
        assert!(
            (got.x - e.x).abs() < 1e-7
                && (got.y - e.y).abs() < 1e-7
                && (got.side - e.side).abs() < 1e-7,
            "{:?} vs {:?}",
            got,
            e
        );
    }
}

#[test]
fn random_moves_keep_every_invariant() {
    let mut degenerate = 0;
    let mut checked = 0;
    for seed in 0..40u64 {
        let mut rng = Rng::new(seed);
        let mut map = seed_map();
        let mut applied = 0;
        while applied < 30 {
            let live = map.live_edge_ids();
            let e = live[1 + rng.below(live.len() as u32 - 1) as usize];
            let Some(mv) = grow_near(&map, e, &mut rng) else {
                continue;
            };
            let mut next = map.clone();
            assert!(
                apply(&mut next, mv),
                "growth moves are always valid: {:?}",
                mv
            );
            map = next;
            applied += 1;
            if check_all(
                &mut map,
                &format!("seed {} grow {} {:?}", seed, applied, mv),
            ) {
                checked += 1;
            } else {
                degenerate += 1;
            }
        }
        let mut removed = 0;
        let mut tries = 0;
        while removed < 25 && tries < 2000 {
            tries += 1;
            let live = map.live_edge_ids();
            let e = live[1 + rng.below(live.len() as u32 - 1) as usize];
            let Some(mv) = shrink(&map, e, &mut rng) else {
                continue;
            };
            let mut next = map.clone();
            if !apply(&mut next, mv) {
                continue;
            }
            map = next;
            removed += 1;
            if check_all(
                &mut map,
                &format!("seed {} shrink {} {:?}", seed, removed, mv),
            ) {
                checked += 1;
            } else {
                degenerate += 1;
            }
        }
        assert!(removed >= 10, "seed {}: only removed {}", seed, removed);
    }
    assert!(
        checked > 1500,
        "checked {} degenerate {}",
        checked,
        degenerate
    );
}

#[test]
fn rejected_shrinks_would_have_broken_three_connectivity() {
    let mut rejected = 0;
    for seed in 0..12u64 {
        let mut rng = Rng::new(seed);
        let mut map = seed_map();
        for _ in 0..60 {
            let live = map.live_edge_ids();
            let e = live[1 + rng.below(live.len() as u32 - 1) as usize];
            if let Some(mv) = grow_near(&map, e, &mut rng) {
                apply(&mut map, mv);
            }
        }
        for e in map.live_edge_ids().into_iter().skip(1) {
            let Some(mv) = shrink(&map, e, &mut rng) else {
                continue;
            };
            let mut next = map.clone();
            if !apply(&mut next, mv) {
                rejected += 1;
                assert!(
                    !next.is_three_connected() || next.is_polyhedral().is_err(),
                    "{:?} rejected but fine",
                    mv
                );
            }
        }
    }
    assert!(rejected > 0);
}
