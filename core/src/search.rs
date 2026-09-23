use crate::geometry::Square;
use crate::layout::layout;
use crate::map::{Map, POLE};
use crate::moves::{apply, grow_near, shrink, Move};
use crate::rng::Rng;
use crate::seed::seed_map;
use crate::solve::solve;
use crate::target::{Frame, Target};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub squares: usize,
    /// Largest fraction of the cropped dimension that may be cut away.
    pub crop_max: f64,
    /// Smallest square side in pixels of the target.
    pub min_side_px: f64,
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
            min_side_px: 1.0,
            candidates: 3,
            refine_steps: 4000,
            temp_start: 0.3,
            temp_end: 0.005,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
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
    target: Target,
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
}

impl Search {
    pub fn new(target: Target, params: Params, seed: u64) -> Search {
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
            stage: Stage::Growing,
            refine_done: 0,
            accepted: 0,
            penalty_scale: 1.0,
            solver_iters: 0,
            solves: 0,
            last_move: None,
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

    fn min_side(&self) -> f64 {
        // Tiling height is 1, mapped onto frame.scale pixels.
        self.params.min_side_px / self.state.frame.scale.max(1.0)
    }

    fn evaluate(&mut self, map: Map, pot: &mut Vec<f64>) -> Option<State> {
        let iters = solve(&map, pot, 1e-12, 20_000);
        self.solver_iters += iters;
        self.solves += 1;
        let min_side = self.min_side();
        let (squares, width) = layout(&map, pot, min_side)?;
        let frame = self.target.frame(width);
        let mut errs = vec![0.0; map.edge_capacity()];
        let mut total = 0.0;
        for s in &squares {
            let e = self.target.error(s, &frame);
            errs[s.id as usize] = e;
            total += e;
        }
        let mse = total / self.target.pixels();
        let excess = (frame.crop - self.params.crop_max).max(0.0);
        let cost = mse + self.penalty_scale * 10.0 * excess / self.params.crop_max;
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
            let v2 = map.vertex_capacity() as u32 - 1;
            let v2 = if map.vertex_alive(v2) && !self.state.map.vertex_alive(v2) {
                v2
            } else {
                (0..map.vertex_capacity() as u32)
                    .find(|&x| map.vertex_alive(x) && !self.state.map.vertex_alive(x))
                    .unwrap()
            };
            pot[v2 as usize] = pot[v as usize];
        }
        self.evaluate(map, &mut pot)
    }

    /// One unit of work. Returns true when the visible tiling changed.
    pub fn step(&mut self) -> bool {
        match self.stage {
            Stage::Growing => self.grow_step(),
            Stage::Refining => self.refine_step(),
            Stage::Done => false,
        }
    }

    fn grow_step(&mut self) -> bool {
        if self.state.map.edge_count() - 1 >= self.params.squares {
            self.stage = Stage::Refining;
            return false;
        }
        let mut best: Option<(State, Move)> = None;
        for attempt in 0..8 {
            let e = if attempt < 4 {
                self.pick_by_error()
            } else {
                None
            }
            .unwrap_or_else(|| self.pick_uniform_edge());
            for _ in 0..self.params.candidates {
                let Some(mv) = grow_near(&self.state.map, e, &mut self.rng) else {
                    continue;
                };
                if let Some(st) = self.try_move(mv) {
                    if best.as_ref().map_or(true, |(b, _)| st.cost < b.cost) {
                        best = Some((st, mv));
                    }
                }
            }
            if best.is_some() {
                break;
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
        let er = self.pick_uniform_edge();
        let Some(m1) = shrink(&self.state.map, er, &mut self.rng) else {
            return false;
        };
        let mut map = self.state.map.clone();
        if !apply(&mut map, m1) {
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
        let Some(m2) = grow_near(&map, ea, &mut self.rng) else {
            return false;
        };
        let before = map.vertex_capacity();
        let split_from = if let Move::Split { h_start, .. } = m2 {
            Some(map.origin(h_start))
        } else {
            None
        };
        if !apply(&mut map, m2) {
            return false;
        }
        let mut pot = self.state.pot.clone();
        if pot.len() < map.vertex_capacity() {
            pot.resize(map.vertex_capacity(), 0.5);
        }
        if let Some(v) = split_from {
            let v2 = (0..map.vertex_capacity() as u32)
                .rev()
                .find(|&x| {
                    map.vertex_alive(x) && (x as usize >= before || !self.state.map.vertex_alive(x))
                })
                .unwrap();
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
        }
        accept
    }

    pub fn in_band(&self) -> bool {
        self.state.frame.crop <= self.params.crop_max + 1e-9
    }

    pub fn map(&self) -> &Map {
        &self.state.map
    }
}
