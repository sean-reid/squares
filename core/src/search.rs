use crate::geometry::Square;
use crate::layout::{layout, Reject};
use crate::map::{Map, POLE};
use crate::moves::{apply, grow_near, shrink, Grow, Move};
use crate::rng::Rng;
use crate::seed::seed_map;
use crate::solve::solve;
use crate::target::{Frame, Target};
use std::rc::Rc;

/// Tiling units; the tiling is 1 tall.
const CROSS_EPS: f64 = 1e-7;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub squares: usize,
    /// Largest fraction of the cropped dimension that may be cut away.
    pub crop_max: f64,
    /// Squares thinner than this many target pixels are rejected outright.
    pub floor_px: f64,
    /// Squares thinner than this many target pixels are penalized, more the
    /// thinner they get.
    pub soft_px: f64,
    /// Cost of one fully collapsed square, in units of the seed error per square.
    pub thin_weight: f64,
    /// Growth candidates evaluated per added square.
    pub candidates: usize,
    /// Compound moves in the refinement stage.
    pub refine_steps: usize,
    pub temp_start: f64,
    pub temp_end: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            squares: 300,
            crop_max: 0.03,
            floor_px: 0.75,
            soft_px: 3.0,
            thin_weight: 4.0,
            candidates: 3,
            refine_steps: 4000,
            temp_start: 0.3,
            temp_end: 0.005,
        }
    }
}

/// The vertex that exists in `after` but not in `before`.
fn new_vertex(before: &Map, after: &Map) -> u32 {
    (0..after.vertex_capacity() as u32)
        .rev()
        .find(|&x| {
            after.vertex_alive(x)
                && (x as usize >= before.vertex_capacity() || !before.vertex_alive(x))
        })
        .expect("a split always creates a vertex")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Random growth that only steers the aspect ratio into the crop band.
    Seeding,
    Growing,
    Refining,
    Done,
}

#[derive(Clone, Debug)]
struct State {
    map: Map,
    pot: Vec<f64>,
    squares: Vec<Square>,
    width: f64,
    frame: Frame,
    /// Per edge id; zero for dead edges and the pole.
    errs: Vec<f64>,
    /// Mean squared Oklab error per pixel.
    mse: f64,
    /// mse plus the crop penalty.
    cost: f64,
}

pub struct Search {
    target: Rc<Target>,
    params: Params,
    rng: Rng,
    state: State,
    stage: Stage,
    refine_done: usize,
    accepted: usize,
    penalty_scale: f64,
    pub solver_iters: usize,
    pub solves: usize,
    pub last_move: Option<Move>,
    /// Counts of rejected candidates by reason, for the bench.
    pub rejections: Vec<(&'static str, usize)>,
}

impl Search {
    pub fn new(target: Rc<Target>, params: Params, seed: u64) -> Search {
        let map = seed_map();
        let mut pot = vec![0.5; map.vertex_capacity()];
        let mut s = Search {
            target,
            params,
            rng: Rng::new(seed),
            state: State {
                map: map.clone(),
                pot: Vec::new(),
                squares: Vec::new(),
                width: 1.0,
                frame: Frame {
                    scale: 1.0,
                    ox: 0.0,
                    oy: 0.0,
                    crop: 0.0,
                },
                errs: Vec::new(),
                mse: 0.0,
                cost: 0.0,
            },
            stage: Stage::Seeding,
            refine_done: 0,
            accepted: 0,
            penalty_scale: 1.0,
            solver_iters: 0,
            solves: 0,
            last_move: None,
            rejections: Vec::new(),
        };
        let st = s.evaluate(map, &mut pot).expect("seed tiling is valid");
        s.penalty_scale = st.mse.max(1e-6);
        s.state = st;
        s
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    pub fn squares(&self) -> &[Square] {
        &self.state.squares
    }

    pub fn width(&self) -> f64 {
        self.state.width
    }

    pub fn mse(&self) -> f64 {
        self.state.mse
    }

    pub fn crop(&self) -> f64 {
        self.state.frame.crop
    }

    pub fn accepted(&self) -> usize {
        self.accepted
    }

    pub fn colors(&self) -> Vec<[u8; 3]> {
        self.state
            .squares
            .iter()
            .map(|s| self.target.mean_rgb(s, &self.state.frame))
            .collect()
    }

    pub fn target(&self) -> &Target {
        &self.target
    }

    pub fn frame(&self) -> Frame {
        self.state.frame
    }

    /// Tiling units per target pixel. The tiling is 1 tall and maps onto at
    /// most the image height.
    fn px(&self) -> f64 {
        1.0 / self.target.height as f64
    }

    /// Solve and lay out the map. A square thinner than the floor is healed
    /// by contracting or deleting its edge, a few times over, because a
    /// sliver is a cross in the making and removing it barely moves anything.
    fn evaluate(&mut self, mut map: Map, pot: &mut Vec<f64>) -> Option<State> {
        let px = self.px();
        let (squares, width) = loop {
            let iters = solve(&map, pot, 1e-12, 20_000);
            self.solver_iters += iters;
            self.solves += 1;
            match layout(&map, pot, self.params.floor_px * px, CROSS_EPS) {
                Ok(v) => break v,
                Err(Reject::Thin(e)) => {
                    if map.edge_count() - 1 < 9 {
                        self.reject("thin square");
                        return None;
                    }
                    let mut healed = false;
                    for mv in [Move::Contract { e }, Move::Delete { e }] {
                        let mut trial = map.clone();
                        if apply(&mut trial, mv) {
                            map = trial;
                            healed = true;
                            break;
                        }
                    }
                    if !healed {
                        self.reject("thin square");
                        return None;
                    }
                    self.reject("healed");
                }
                Err(r) => {
                    self.reject(r.name());
                    return None;
                }
            }
        };
        let frame = self.target.frame(width);
        let mut errs = vec![0.0; map.edge_capacity()];
        let mut total = 0.0;
        let soft = self.params.soft_px * px;
        let mut thin = 0.0;
        for s in &squares {
            let e = self.target.error(s, &frame);
            errs[s.id as usize] = e;
            total += e;
            if s.side < soft {
                let d = (soft - s.side) / soft;
                thin += d * d;
            }
        }
        let mse = total / self.target.pixels();
        let excess = (frame.crop - self.params.crop_max).max(0.0);
        let per_square = self.penalty_scale / self.params.squares as f64;
        let cost = mse
            + self.penalty_scale * 10.0 * excess / self.params.crop_max
            + per_square * self.params.thin_weight * thin;
        Some(State {
            map,
            pot: pot.clone(),
            squares,
            width,
            frame,
            errs,
            mse,
            cost,
        })
    }

    fn reject(&mut self, reason: &'static str) {
        match self.rejections.iter_mut().find(|(r, _)| *r == reason) {
            Some((_, n)) => *n += 1,
            None => self.rejections.push((reason, 1)),
        }
    }

    fn pick_weighted(&mut self, weights: &[f64]) -> Option<u32> {
        let total: f64 = weights.iter().sum();
        if total <= 0.0 {
            return None;
        }
        let mut r = self.rng.unit() * total;
        for (i, &w) in weights.iter().enumerate() {
            if w <= 0.0 {
                continue;
            }
            if r < w {
                return Some(i as u32);
            }
            r -= w;
        }
        weights.iter().rposition(|&w| w > 0.0).map(|i| i as u32)
    }

    fn pick_by_error(&mut self) -> Option<u32> {
        let errs = self.state.errs.clone();
        self.pick_weighted(&errs)
    }

    /// A square chosen by how far it falls below the comfortable size.
    fn pick_thin(&mut self) -> Option<u32> {
        let soft = self.params.soft_px * self.px();
        let mut w = vec![0.0; self.state.map.edge_capacity()];
        let mut any = false;
        for s in &self.state.squares {
            if s.side < soft {
                let d = (soft - s.side) / soft;
                w[s.id as usize] = d * d;
                any = true;
            }
        }
        if any {
            self.pick_weighted(&w)
        } else {
            None
        }
    }

    fn pick_uniform_edge(&mut self) -> u32 {
        loop {
            let e = self.rng.below(self.state.map.edge_capacity() as u32);
            if e != POLE && self.state.map.edge_alive(e) {
                return e;
            }
        }
    }

    fn try_move(&mut self, mv: Move) -> Option<State> {
        let mut map = self.state.map.clone();
        if !apply(&mut map, mv) {
            return None;
        }
        let mut pot = self.state.pot.clone();
        if pot.len() < map.vertex_capacity() {
            pot.resize(map.vertex_capacity(), 0.5);
        }
        if let Move::Split { h_start, .. } = mv {
            let v = self.state.map.origin(h_start);
            let v2 = new_vertex(&self.state.map, &map);
            pot[v2 as usize] = pot[v as usize];
        }
        self.evaluate(map, &mut pot)
    }

    /// One unit of work. Returns true when the visible tiling changed.
    pub fn step(&mut self) -> bool {
        match self.stage {
            Stage::Seeding => self.seed_step(),
            Stage::Growing => self.grow_step(),
            Stage::Refining => self.refine_step(),
            Stage::Done => false,
        }
    }

    /// Add one square anywhere, preferring the kind that moves the ratio the
    /// right way, until the tiling sits in the crop band or the seeding budget
    /// runs out. Image error plays no part here.
    fn seed_step(&mut self) -> bool {
        let budget = (self.params.squares / 3).clamp(9, 40);
        if self.in_band() || self.state.map.edge_count() > budget {
            self.stage = Stage::Growing;
            return false;
        }
        let kind = self.direction();
        for attempt in 0..16 {
            let e = self.pick_uniform_edge();
            let k = if attempt < 8 { kind } else { Grow::Any };
            let Some(mv) = grow_near(
                &self.state.map,
                &self.state.pot,
                e,
                k,
                self.params.soft_px * self.px(),
                &mut self.rng,
            ) else {
                continue;
            };
            if let Some(st) = self.try_move(mv) {
                self.state = st;
                self.last_move = Some(mv);
                self.accepted += 1;
                return true;
            }
        }
        false
    }

    fn grow_step(&mut self) -> bool {
        if self.state.map.edge_count() > self.params.squares {
            self.stage = Stage::Refining;
            return false;
        }
        let kind = self.direction();
        let mut best: Option<(State, Move)> = None;
        let mut tried = 0;
        for attempt in 0..24 {
            if tried >= self.params.candidates && best.is_some() {
                break;
            }
            let e = if attempt < 16 {
                self.pick_by_error()
            } else {
                None
            }
            .unwrap_or_else(|| self.pick_uniform_edge());
            let k = if attempt % 2 == 0 { kind } else { Grow::Any };
            for _ in 0..1 {
                tried += 1;
                let Some(mv) = grow_near(
                    &self.state.map,
                    &self.state.pot,
                    e,
                    k,
                    self.params.soft_px * self.px(),
                    &mut self.rng,
                ) else {
                    continue;
                };
                if let Some(st) = self.try_move(mv) {
                    if best.as_ref().is_none_or(|(b, _)| st.cost < b.cost) {
                        best = Some((st, mv));
                    }
                }
            }
        }
        match best {
            Some((st, mv)) => {
                self.state = st;
                self.last_move = Some(mv);
                self.accepted += 1;
                true
            }
            None => false,
        }
    }

    fn temperature(&self) -> f64 {
        let t = self.refine_done as f64 / self.params.refine_steps.max(1) as f64;
        let per_square = self.state.mse / self.params.squares as f64;
        per_square
            * self.params.temp_start
            * (self.params.temp_end / self.params.temp_start).powf(t)
    }

    fn refine_step(&mut self) -> bool {
        if self.refine_done >= self.params.refine_steps {
            self.stage = Stage::Done;
            return false;
        }
        self.refine_done += 1;
        let er = if self.rng.below(2) == 0 {
            self.pick_thin().unwrap_or_else(|| self.pick_uniform_edge())
        } else {
            self.pick_uniform_edge()
        };
        let Some(m1) = shrink(&self.state.map, er, &mut self.rng) else {
            self.reject("no shrink at edge");
            return false;
        };
        let mut map = self.state.map.clone();
        if !apply(&mut map, m1) {
            self.reject("shrink breaks 3-connectivity");
            return false;
        }
        let mut errs = self.state.errs.clone();
        errs[er as usize] = 0.0;
        let Some(ea) = self.pick_weighted(&errs) else {
            return false;
        };
        if !map.edge_alive(ea) {
            return false;
        }
        let kind = if self.rng.below(2) == 0 {
            self.direction()
        } else {
            Grow::Any
        };
        let Some(m2) = grow_near(
            &map,
            &self.state.pot,
            ea,
            kind,
            self.params.soft_px * self.px(),
            &mut self.rng,
        ) else {
            self.reject("no growth near edge");
            return false;
        };
        let split_from = if let Move::Split { h_start, .. } = m2 {
            Some(map.origin(h_start))
        } else {
            None
        };
        let before = map.clone();
        if !apply(&mut map, m2) {
            return false;
        }
        let mut pot = self.state.pot.clone();
        if pot.len() < map.vertex_capacity() {
            pot.resize(map.vertex_capacity(), 0.5);
        }
        if let Some(v) = split_from {
            let v2 = new_vertex(&before, &map);
            pot[v2 as usize] = pot[v as usize];
        }
        let Some(st) = self.evaluate(map, &mut pot) else {
            return false;
        };
        let delta = st.cost - self.state.cost;
        let t = self.temperature();
        let accept = delta <= 0.0 || (t > 0.0 && self.rng.unit() < (-delta / t).exp());
        if accept {
            self.state = st;
            self.last_move = Some(m2);
            self.accepted += 1;
        } else {
            self.reject("worse");
        }
        accept
    }

    /// Which way the aspect ratio must move to land inside the crop band.
    fn direction(&self) -> Grow {
        let r0 = self.target.width as f64 / self.target.height as f64;
        let w = self.state.width;
        if w < r0 * (1.0 - self.params.crop_max) {
            Grow::Widen
        } else if w > r0 / (1.0 - self.params.crop_max) {
            Grow::Narrow
        } else {
            Grow::Any
        }
    }

    pub fn in_band(&self) -> bool {
        self.state.frame.crop <= self.params.crop_max + 1e-9
    }

    pub fn map(&self) -> &Map {
        &self.state.map
    }
}
