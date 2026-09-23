//! Planar map as a half-edge structure. Edge `e` owns half-edges `2e` and
//! `2e + 1`, which are twins. `next` walks a face with the face on its left.
//! `rot(h) = next(twin(h))` is the next outgoing half-edge clockwise around
//! the origin of `h`, in screen coordinates with y growing downward.
//!
//! Edge 0 is the pole edge joining the source (top of the tiling) to the
//! sink (bottom). Every other edge is a square. A split or contraction at a
//! pole can move it to another vertex id, so always read `source()` and
//! `sink()` rather than caching them.

pub const NONE: u32 = u32::MAX;
pub const POLE: u32 = 0;

#[derive(Clone, Debug)]
pub struct Map {
    next: Vec<u32>,
    prev: Vec<u32>,
    origin: Vec<u32>,
    face: Vec<u32>,
    v_half: Vec<u32>,
    v_deg: Vec<u32>,
    f_half: Vec<u32>,
    f_size: Vec<u32>,
    free_edges: Vec<u32>,
    free_verts: Vec<u32>,
    free_faces: Vec<u32>,
    live_edges: usize,
    live_verts: usize,
    live_faces: usize,
    stamp: Vec<u32>,
    stamp_gen: u32,
}

#[inline]
pub fn twin(h: u32) -> u32 {
    h ^ 1
}

#[inline]
pub fn edge_of(h: u32) -> u32 {
    h >> 1
}

impl Map {
    /// Build from an edge list and, per vertex, the clockwise cyclic order of
    /// its incident edges.
    pub fn from_rotations(edges: &[(u32, u32)], rotations: &[Vec<u32>]) -> Map {
        let ne = edges.len();
        let nv = rotations.len();
        let mut m = Map {
            next: vec![NONE; 2 * ne],
            prev: vec![NONE; 2 * ne],
            origin: vec![NONE; 2 * ne],
            face: vec![NONE; 2 * ne],
            v_half: vec![NONE; nv],
            v_deg: vec![0; nv],
            f_half: Vec::new(),
            f_size: Vec::new(),
            free_edges: Vec::new(),
            free_verts: Vec::new(),
            free_faces: Vec::new(),
            live_edges: ne,
            live_verts: nv,
            live_faces: 0,
            stamp: Vec::new(),
            stamp_gen: 0,
        };
        for (e, &(u, v)) in edges.iter().enumerate() {
            m.origin[2 * e] = u;
            m.origin[2 * e + 1] = v;
        }
        for (v, rot) in rotations.iter().enumerate() {
            let v = v as u32;
            m.v_deg[v as usize] = rot.len() as u32;
            let halves: Vec<u32> = rot
                .iter()
                .map(|&e| {
                    if edges[e as usize].0 == v {
                        2 * e
                    } else {
                        2 * e + 1
                    }
                })
                .collect();
            m.v_half[v as usize] = halves[0];
            for i in 0..halves.len() {
                let h = halves[i];
                let r = halves[(i + 1) % halves.len()];
                m.next[twin(h) as usize] = r;
                m.prev[r as usize] = twin(h);
            }
        }
        for h in 0..(2 * ne) as u32 {
            if m.face[h as usize] == NONE {
                let f = m.alloc_face();
                m.assign_face(h, f);
            }
        }
        m
    }

    #[inline]
    pub fn source(&self) -> u32 {
        self.origin[0]
    }

    #[inline]
    pub fn sink(&self) -> u32 {
        self.origin[1]
    }

    pub fn edge_count(&self) -> usize {
        self.live_edges
    }

    pub fn vertex_count(&self) -> usize {
        self.live_verts
    }

    pub fn face_count(&self) -> usize {
        self.live_faces
    }

    pub fn edge_capacity(&self) -> usize {
        self.next.len() / 2
    }

    pub fn vertex_capacity(&self) -> usize {
        self.v_half.len()
    }

    pub fn face_capacity(&self) -> usize {
        self.f_half.len()
    }

    #[inline]
    pub fn edge_alive(&self, e: u32) -> bool {
        self.origin[(2 * e) as usize] != NONE
    }

    #[inline]
    pub fn vertex_alive(&self, v: u32) -> bool {
        self.v_half[v as usize] != NONE
    }

    #[inline]
    pub fn face_alive(&self, f: u32) -> bool {
        self.f_half[f as usize] != NONE
    }

    #[inline]
    pub fn next(&self, h: u32) -> u32 {
        self.next[h as usize]
    }

    #[inline]
    pub fn prev(&self, h: u32) -> u32 {
        self.prev[h as usize]
    }

    #[inline]
    pub fn origin(&self, h: u32) -> u32 {
        self.origin[h as usize]
    }

    #[inline]
    pub fn dest(&self, h: u32) -> u32 {
        self.origin[twin(h) as usize]
    }

    #[inline]
    pub fn face(&self, h: u32) -> u32 {
        self.face[h as usize]
    }

    #[inline]
    pub fn rot(&self, h: u32) -> u32 {
        self.next[twin(h) as usize]
    }

    #[inline]
    pub fn rot_inv(&self, h: u32) -> u32 {
        twin(self.prev[h as usize])
    }

    #[inline]
    pub fn degree(&self, v: u32) -> u32 {
        self.v_deg[v as usize]
    }

    #[inline]
    pub fn face_size(&self, f: u32) -> u32 {
        self.f_size[f as usize]
    }

    #[inline]
    pub fn any_half_at(&self, v: u32) -> u32 {
        self.v_half[v as usize]
    }

    #[inline]
    pub fn any_half_on(&self, f: u32) -> u32 {
        self.f_half[f as usize]
    }

    /// Outgoing half-edges of `v` in clockwise order, without allocating.
    pub fn star_iter(&self, v: u32) -> impl Iterator<Item = u32> + '_ {
        let start = self.v_half[v as usize];
        let mut h = start;
        let mut first = true;
        std::iter::from_fn(move || {
            if !first && h == start {
                return None;
            }
            first = false;
            let out = h;
            h = self.rot(h);
            Some(out)
        })
    }

    /// Outgoing half-edges of `v` in clockwise order.
    pub fn star(&self, v: u32) -> Vec<u32> {
        let start = self.v_half[v as usize];
        let mut out = Vec::with_capacity(self.v_deg[v as usize] as usize);
        let mut h = start;
        loop {
            out.push(h);
            h = self.rot(h);
            if h == start {
                break;
            }
        }
        out
    }

    /// Half-edges around face `f`, following `next`.
    pub fn cycle(&self, f: u32) -> Vec<u32> {
        let start = self.f_half[f as usize];
        let mut out = Vec::with_capacity(self.f_size[f as usize] as usize);
        let mut h = start;
        loop {
            out.push(h);
            h = self.next[h as usize];
            if h == start {
                break;
            }
        }
        out
    }

    pub fn live_edge_ids(&self) -> Vec<u32> {
        (0..self.edge_capacity() as u32)
            .filter(|&e| self.edge_alive(e))
            .collect()
    }

    pub fn live_vertex_ids(&self) -> Vec<u32> {
        (0..self.vertex_capacity() as u32)
            .filter(|&v| self.vertex_alive(v))
            .collect()
    }

    pub fn live_face_ids(&self) -> Vec<u32> {
        (0..self.face_capacity() as u32)
            .filter(|&f| self.face_alive(f))
            .collect()
    }

    fn alloc_edge(&mut self) -> u32 {
        self.live_edges += 1;
        if let Some(e) = self.free_edges.pop() {
            return e;
        }
        let e = self.edge_capacity() as u32;
        for _ in 0..2 {
            self.next.push(NONE);
            self.prev.push(NONE);
            self.origin.push(NONE);
            self.face.push(NONE);
        }
        e
    }

    fn free_edge(&mut self, e: u32) {
        for h in [2 * e, 2 * e + 1] {
            self.next[h as usize] = NONE;
            self.prev[h as usize] = NONE;
            self.origin[h as usize] = NONE;
            self.face[h as usize] = NONE;
        }
        self.free_edges.push(e);
        self.live_edges -= 1;
    }

    fn alloc_vertex(&mut self) -> u32 {
        self.live_verts += 1;
        if let Some(v) = self.free_verts.pop() {
            return v;
        }
        self.v_half.push(NONE);
        self.v_deg.push(0);
        (self.v_half.len() - 1) as u32
    }

    fn free_vertex(&mut self, v: u32) {
        self.v_half[v as usize] = NONE;
        self.v_deg[v as usize] = 0;
        self.free_verts.push(v);
        self.live_verts -= 1;
    }

    fn alloc_face(&mut self) -> u32 {
        self.live_faces += 1;
        if let Some(f) = self.free_faces.pop() {
            return f;
        }
        self.f_half.push(NONE);
        self.f_size.push(0);
        (self.f_half.len() - 1) as u32
    }

    fn free_face(&mut self, f: u32) {
        self.f_half[f as usize] = NONE;
        self.f_size[f as usize] = 0;
        self.free_faces.push(f);
        self.live_faces -= 1;
    }

    fn assign_face(&mut self, start: u32, f: u32) {
        let mut h = start;
        let mut n = 0;
        loop {
            self.face[h as usize] = f;
            n += 1;
            h = self.next[h as usize];
            if h == start {
                break;
            }
        }
        self.f_half[f as usize] = start;
        self.f_size[f as usize] = n;
    }

    /// Add an edge from the origin of `ha` to the origin of `hb`, both on the
    /// same face and not consecutive on it. Returns the new edge id; its
    /// half-edge `2e` leaves the origin of `ha`.
    pub fn insert_edge(&mut self, ha: u32, hb: u32) -> u32 {
        debug_assert_eq!(self.face[ha as usize], self.face[hb as usize]);
        debug_assert!(ha != hb && self.next[ha as usize] != hb && self.next[hb as usize] != ha);
        let f = self.face[ha as usize];
        let u = self.origin[ha as usize];
        let v = self.origin[hb as usize];
        let pa = self.prev[ha as usize];
        let pb = self.prev[hb as usize];
        let e = self.alloc_edge();
        let g = 2 * e;
        let g2 = g + 1;
        self.origin[g as usize] = u;
        self.origin[g2 as usize] = v;
        self.next[pa as usize] = g;
        self.prev[g as usize] = pa;
        self.next[g as usize] = hb;
        self.prev[hb as usize] = g;
        self.next[pb as usize] = g2;
        self.prev[g2 as usize] = pb;
        self.next[g2 as usize] = ha;
        self.prev[ha as usize] = g2;
        self.assign_face(g, f);
        let f2 = self.alloc_face();
        self.assign_face(g2, f2);
        self.v_deg[u as usize] += 1;
        self.v_deg[v as usize] += 1;
        e
    }

    /// Remove edge `e`, merging the faces on its two sides. The caller checks
    /// that the result stays 3-connected.
    pub fn delete_edge(&mut self, e: u32) -> u32 {
        debug_assert!(e != POLE);
        let h = 2 * e;
        let t = h + 1;
        let u = self.origin[h as usize];
        let v = self.origin[t as usize];
        let f1 = self.face[h as usize];
        let f2 = self.face[t as usize];
        debug_assert_ne!(f1, f2);
        let ph = self.prev[h as usize];
        let nh = self.next[h as usize];
        let pt = self.prev[t as usize];
        let nt = self.next[t as usize];
        self.v_half[u as usize] = nt;
        self.v_half[v as usize] = nh;
        self.next[ph as usize] = nt;
        self.prev[nt as usize] = ph;
        self.next[pt as usize] = nh;
        self.prev[nh as usize] = pt;
        self.free_face(f2);
        self.assign_face(nh, f1);
        self.v_deg[u as usize] -= 1;
        self.v_deg[v as usize] -= 1;
        self.free_edge(e);
        f1
    }

    /// Split the origin `v` of `h_start`: the `k` outgoing half-edges from
    /// `h_start` clockwise move to a new vertex joined to `v` by a new edge.
    /// Returns (new edge, new vertex); half-edge `2e` leaves `v`.
    pub fn split_vertex(&mut self, h_start: u32, k: u32) -> (u32, u32) {
        let v = self.origin[h_start as usize];
        let d = self.v_deg[v as usize];
        debug_assert!(k >= 2 && d - k >= 2);
        let a = self.rot_inv(h_start);
        let mut b = h_start;
        let mut run = Vec::with_capacity(k as usize);
        for i in 0..k {
            run.push(b);
            if i + 1 < k {
                b = self.rot(b);
            }
        }
        let c = self.rot(b);
        let ta = twin(a);
        let tb = twin(b);
        let v2 = self.alloc_vertex();
        let e = self.alloc_edge();
        let g = 2 * e;
        let g2 = g + 1;
        self.origin[g as usize] = v;
        self.origin[g2 as usize] = v2;
        let fa = self.face[ta as usize];
        let fb = self.face[tb as usize];
        self.next[ta as usize] = g;
        self.prev[g as usize] = ta;
        self.next[g as usize] = h_start;
        self.prev[h_start as usize] = g;
        self.face[g as usize] = fa;
        self.f_size[fa as usize] += 1;
        self.next[tb as usize] = g2;
        self.prev[g2 as usize] = tb;
        self.next[g2 as usize] = c;
        self.prev[c as usize] = g2;
        self.face[g2 as usize] = fb;
        self.f_size[fb as usize] += 1;
        for &r in &run {
            self.origin[r as usize] = v2;
        }
        self.v_half[v as usize] = g;
        self.v_half[v2 as usize] = g2;
        self.v_deg[v as usize] = d - k + 1;
        self.v_deg[v2 as usize] = k + 1;
        (e, v2)
    }

    /// Contract edge `e`, merging the destination of `2e` into its origin.
    /// Both faces beside `e` must have at least four edges. The caller checks
    /// that the result stays 3-connected.
    pub fn contract_edge(&mut self, e: u32) -> u32 {
        debug_assert!(e != POLE);
        let h = 2 * e;
        let t = h + 1;
        let u = self.origin[h as usize];
        let w = self.origin[t as usize];
        let fh = self.face[h as usize];
        let ft = self.face[t as usize];
        debug_assert!(self.f_size[fh as usize] >= 4 && self.f_size[ft as usize] >= 4);
        let ta = self.prev[h as usize];
        let tb = self.prev[t as usize];
        let h_start = self.next[h as usize];
        let c = self.next[t as usize];
        let star_w = self.star(w);
        for &r in &star_w {
            if r != t {
                self.origin[r as usize] = u;
            }
        }
        self.next[ta as usize] = h_start;
        self.prev[h_start as usize] = ta;
        self.next[tb as usize] = c;
        self.prev[c as usize] = tb;
        self.f_size[fh as usize] -= 1;
        self.f_size[ft as usize] -= 1;
        if self.f_half[fh as usize] == h {
            self.f_half[fh as usize] = h_start;
        }
        if self.f_half[ft as usize] == t {
            self.f_half[ft as usize] = c;
        }
        self.v_half[u as usize] = c;
        self.v_deg[u as usize] += self.v_deg[w as usize] - 2;
        self.free_vertex(w);
        self.free_edge(e);
        u
    }

    fn next_stamp(&mut self) -> u32 {
        let cap = self.vertex_capacity().max(self.face_capacity());
        if self.stamp.len() < cap {
            self.stamp.resize(cap, 0);
        }
        self.stamp_gen = self.stamp_gen.wrapping_add(1);
        if self.stamp_gen == 0 {
            self.stamp.iter_mut().for_each(|s| *s = 0);
            self.stamp_gen = 1;
        }
        self.stamp_gen
    }

    /// The face is a simple cycle: no vertex repeats.
    pub fn face_is_simple(&mut self, f: u32) -> bool {
        let g = self.next_stamp();
        let cyc = self.cycle(f);
        for h in cyc {
            let v = self.origin[h as usize] as usize;
            if self.stamp[v] == g {
                return false;
            }
            self.stamp[v] = g;
        }
        true
    }

    /// Two faces may share nothing, one vertex, or one edge with its two ends.
    fn faces_meet_ok(&mut self, f: u32, g: u32) -> bool {
        let s = self.next_stamp();
        for h in self.cycle(f) {
            self.stamp[self.origin[h as usize] as usize] = s;
        }
        let mut shared_v = 0;
        let mut shared_e = 0;
        for h in self.cycle(g) {
            if self.stamp[self.origin[h as usize] as usize] == s {
                shared_v += 1;
            }
            if self.face[twin(h) as usize] == f {
                shared_e += 1;
            }
        }
        shared_v <= 1 || (shared_v == 2 && shared_e == 1)
    }

    /// After a deletion that produced face `f`: `f` is simple and meets every
    /// neighboring face properly.
    pub fn face_is_polyhedral(&mut self, f: u32) -> bool {
        if !self.face_is_simple(f) {
            return false;
        }
        let mut seen: Vec<u32> = Vec::new();
        for h in self.cycle(f) {
            let v = self.origin[h as usize];
            for s in self.star(v) {
                let g = self.face[s as usize];
                if g != f && !seen.contains(&g) {
                    seen.push(g);
                    if !self.faces_meet_ok(f, g) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// After a contraction that produced vertex `v`: every face around it is
    /// simple and every pair of them meets properly.
    pub fn vertex_is_polyhedral(&mut self, v: u32) -> bool {
        let faces: Vec<u32> = self
            .star(v)
            .iter()
            .map(|&h| self.face[h as usize])
            .collect();
        for &f in &faces {
            if !self.face_is_simple(f) {
                return false;
            }
        }
        for i in 0..faces.len() {
            for j in (i + 1)..faces.len() {
                if faces[i] != faces[j] && !self.faces_meet_ok(faces[i], faces[j]) {
                    return false;
                }
            }
        }
        true
    }

    /// Whole-map check used by tests: Euler's formula, a simple graph with
    /// minimum degree three, and every pair of faces meeting properly.
    pub fn is_polyhedral(&mut self) -> Result<(), String> {
        let v = self.live_verts as i64;
        let e = self.live_edges as i64;
        let f = self.live_faces as i64;
        if v - e + f != 2 {
            return Err(format!("euler: v={} e={} f={}", v, e, f));
        }
        for vid in self.live_vertex_ids() {
            if self.degree(vid) < 3 {
                return Err(format!("vertex {} has degree {}", vid, self.degree(vid)));
            }
            let star = self.star(vid);
            if star.len() != self.degree(vid) as usize {
                return Err(format!(
                    "vertex {} star length {} vs degree {}",
                    vid,
                    star.len(),
                    self.degree(vid)
                ));
            }
            let mut nbrs: Vec<u32> = star.iter().map(|&h| self.dest(h)).collect();
            nbrs.sort_unstable();
            let n = nbrs.len();
            nbrs.dedup();
            if nbrs.len() != n {
                return Err(format!("vertex {} has a parallel edge", vid));
            }
        }
        let faces = self.live_face_ids();
        for &fid in &faces {
            if self.cycle(fid).len() != self.face_size(fid) as usize {
                return Err(format!("face {} size mismatch", fid));
            }
            if !self.face_is_simple(fid) {
                return Err(format!("face {} is not a simple cycle", fid));
            }
        }
        for i in 0..faces.len() {
            for j in (i + 1)..faces.len() {
                if !self.faces_meet_ok(faces[i], faces[j]) {
                    return Err(format!("faces {} and {} meet badly", faces[i], faces[j]));
                }
            }
        }
        Ok(())
    }

    /// Brute-force 3-connectivity used by tests.
    pub fn is_three_connected(&self) -> bool {
        let verts = self.live_vertex_ids();
        if verts.len() < 4 {
            return false;
        }
        for i in 0..verts.len() {
            for j in (i + 1)..verts.len() {
                if !self.connected_without(&verts, &[verts[i], verts[j]]) {
                    return false;
                }
            }
        }
        true
    }

    fn connected_without(&self, verts: &[u32], removed: &[u32]) -> bool {
        let start = match verts.iter().find(|v| !removed.contains(v)) {
            Some(&s) => s,
            None => return true,
        };
        let mut seen = vec![false; self.vertex_capacity()];
        let mut stack = vec![start];
        seen[start as usize] = true;
        let mut count = 0;
        while let Some(v) = stack.pop() {
            count += 1;
            for h in self.star(v) {
                let w = self.dest(h);
                if !removed.contains(&w) && !seen[w as usize] {
                    seen[w as usize] = true;
                    stack.push(w);
                }
            }
        }
        count == verts.len() - removed.len()
    }
}
