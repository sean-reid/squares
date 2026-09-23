use crate::search::{Params, Search, Stage};
use crate::svg;
use crate::target::Target;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

/// A decoded photo, shared by every session that renders it.
#[wasm_bindgen]
pub struct Photo {
    target: Rc<Target>,
}

#[wasm_bindgen]
impl Photo {
    #[wasm_bindgen(constructor)]
    pub fn new(rgba: &[u8], width: u32, height: u32) -> Photo {
        Photo {
            target: Rc::new(Target::from_rgba(rgba, width as usize, height as usize)),
        }
    }

    pub fn width(&self) -> u32 {
        self.target.width as u32
    }

    pub fn height(&self) -> u32 {
        self.target.height as u32
    }
}

/// One search over one photo at one square count.
#[wasm_bindgen]
pub struct Session {
    search: Search,
}

#[wasm_bindgen]
impl Session {
    #[wasm_bindgen(constructor)]
    pub fn new(photo: &Photo, squares: u32, seed: u32) -> Session {
        let params = Params {
            squares: squares as usize,
            refine_steps: usize::MAX / 2,
            ..Params::default()
        };
        Session {
            search: Search::new(photo.target.clone(), params, seed as u64),
        }
    }

    /// Run up to `steps` search steps. Returns true when the tiling changed.
    pub fn run(&mut self, steps: u32) -> bool {
        let mut changed = false;
        for _ in 0..steps {
            if self.search.stage() == Stage::Done {
                break;
            }
            changed |= self.search.step();
        }
        changed
    }

    /// 0 seeding, 1 growing, 2 refining, 3 done.
    pub fn stage(&self) -> u8 {
        match self.search.stage() {
            Stage::Seeding => 0,
            Stage::Growing => 1,
            Stage::Refining => 2,
            Stage::Done => 3,
        }
    }

    /// Width of the tiling; the height is 1.
    pub fn width(&self) -> f64 {
        self.search.width()
    }

    pub fn count(&self) -> u32 {
        self.search.squares().len() as u32
    }

    /// Mean squared Oklab error per pixel.
    pub fn error(&self) -> f64 {
        self.search.mse()
    }

    /// Photo pixels per tiling unit, then the crop offset in photo pixels,
    /// then the cropped fraction: [scale, ox, oy, crop].
    pub fn frame(&self) -> Vec<f64> {
        let f = self.search.frame();
        vec![f.scale, f.ox, f.oy, f.crop]
    }

    /// Seven numbers per square: id, x, y, side, r, g, b.
    pub fn squares(&self) -> Vec<f32> {
        let colors = self.search.colors();
        let mut out = Vec::with_capacity(colors.len() * 7);
        for (s, c) in self.search.squares().iter().zip(colors) {
            out.extend_from_slice(&[
                s.id as f32,
                s.x as f32,
                s.y as f32,
                s.side as f32,
                c[0] as f32,
                c[1] as f32,
                c[2] as f32,
            ]);
        }
        out
    }

    pub fn svg(&self, gap: f64, background: Option<String>) -> String {
        let colors = self.search.colors();
        svg::render(
            self.search.squares(),
            &colors,
            self.search.width(),
            gap,
            background.as_deref(),
        )
    }
}
