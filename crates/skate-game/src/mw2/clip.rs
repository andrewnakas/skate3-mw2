//! IW4 traces against the skate map's collision triangles.
//!
//! `movement_iw4::pmove` only asks for box sweeps (`CollisionBackend::trace`).
//! The skate map has no brushes, so every sweep here is an exact swept
//! separating-axis test of the player box against the collision triangles,
//! bucketed in a uniform XY grid. Everything is in IW4 space: inches, Z up.

use movement_iw4::{CollisionBackend, GroundTraceInput};
use trace_iw4::{ENTITYNUM_NONE, ENTITYNUM_WORLD, HITTYPE_ENTITY, Trace};

/// Metres per IW4 unit (one inch).
pub(crate) const UNIT: f32 = 0.0254;
/// Sweeps stop this far short of the surface, as `SURFACE_CLIP_EPSILON` does.
const CLIP_EPSILON: f32 = 0.125;
const CELL: f32 = 128.0;
const CONTENTS_SOLID: u32 = 1;
/// Surface type "concrete" in bits 20..25, so footsteps and jumps have a surface.
const SURFACE_CONCRETE: u32 = 6 << 20;

/// Bevy (Y up, metres) to IW4 (Z up, inches); a proper rotation, no mirror.
pub(crate) fn to_iw(v: bevy::math::Vec3) -> [f32; 3] {
    [v.x / UNIT, -v.z / UNIT, v.y / UNIT]
}

pub(crate) fn from_iw(a: [f32; 3]) -> bevy::math::Vec3 {
    bevy::math::Vec3::new(a[0], a[2], -a[1]) * UNIT
}

/// A direction (no scaling) from IW4 to Bevy.
pub(crate) fn dir_from_iw(a: [f32; 3]) -> bevy::math::Vec3 {
    bevy::math::Vec3::new(a[0], a[2], -a[1])
}

type V = [f32; 3];
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len(a: V) -> f32 {
    dot(a, a).sqrt()
}

pub(crate) struct ClipWorld {
    tris: Vec<[V; 3]>,
    min: [f32; 2],
    dims: [usize; 2],
    cells: Vec<Vec<u32>>,
    pub(crate) source_len: usize,
}

impl ClipWorld {
    pub(crate) fn new(triangles: impl Iterator<Item = [bevy::math::Vec3; 3]>, source_len: usize) -> Self {
        let tris: Vec<[V; 3]> = triangles
            .map(|t| t.map(to_iw))
            .filter(|t| len(cross(sub(t[1], t[0]), sub(t[2], t[0]))) > 1e-6)
            .collect();
        let mut lo = [f32::INFINITY; 2];
        let mut hi = [f32::NEG_INFINITY; 2];
        for t in &tris {
            for v in t {
                for a in 0..2 {
                    lo[a] = lo[a].min(v[a]);
                    hi[a] = hi[a].max(v[a]);
                }
            }
        }
        if tris.is_empty() {
            lo = [0.0; 2];
            hi = [0.0; 2];
        }
        let dims = [0, 1].map(|a| (((hi[a] - lo[a]) / CELL).floor() as usize + 1).min(4096));
        let mut cells = vec![Vec::new(); dims[0] * dims[1]];
        for (i, t) in tris.iter().enumerate() {
            let (c0, c1) = Self::cell_range_of(lo, dims, t.iter().copied());
            for y in c0[1]..=c1[1] {
                for x in c0[0]..=c1[0] {
                    cells[y * dims[0] + x].push(i as u32);
                }
            }
        }
        Self { tris, min: lo, dims, cells, source_len }
    }

    fn cell_range_of(min: [f32; 2], dims: [usize; 2], points: impl Iterator<Item = V>) -> ([usize; 2], [usize; 2]) {
        let mut lo = [f32::INFINITY; 2];
        let mut hi = [f32::NEG_INFINITY; 2];
        for p in points {
            for a in 0..2 {
                lo[a] = lo[a].min(p[a]);
                hi[a] = hi[a].max(p[a]);
            }
        }
        let cell = |v: f32, a: usize| (((v - min[a]) / CELL).floor().max(0.0) as usize).min(dims[a] - 1);
        ([cell(lo[0], 0), cell(lo[1], 1)], [cell(hi[0], 0), cell(hi[1], 1)])
    }

    pub(crate) fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    /// Centres of floor triangles big enough to stand on, for spawns.
    pub(crate) fn walkable_spots(&self) -> Vec<[f32; 3]> {
        let mut spots: Vec<[f32; 3]> = self
            .tris
            .iter()
            .filter_map(|t| {
                let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
                let area = len(n) * 0.5;
                let up = n[2].abs() / len(n).max(1e-6);
                (up >= 0.85 && area >= 1200.0).then(|| [0, 1, 2].map(|a| (t[0][a] + t[1][a] + t[2][a]) / 3.0))
            })
            .collect();
        if spots.len() > 6000 {
            let step = spots.len() / 6000 + 1;
            spots = spots.into_iter().step_by(step).collect();
        }
        spots
    }

    /// Swept box (`mins`/`maxs` around the moving origin) from `start` to `end`.
    pub(crate) fn sweep(&self, start: V, end: V, mins: V, maxs: V) -> Trace {
        let mut out = Trace {
            fraction: 1.0,
            endpos: end,
            hit_type: 0,
            hit_id: ENTITYNUM_NONE,
            ..Default::default()
        };
        if self.tris.is_empty() {
            return out;
        }
        // Centre the box: offset the path by the box centre, use half extents.
        let centre = [0, 1, 2].map(|a| (mins[a] + maxs[a]) * 0.5);
        let half = [0, 1, 2].map(|a| ((maxs[a] - mins[a]) * 0.5).max(0.0));
        let s = [0, 1, 2].map(|a| start[a] + centre[a]);
        let e = [0, 1, 2].map(|a| end[a] + centre[a]);
        let d = sub(e, s);
        let dist = len(d);
        let pad = [half[0] + 1.0, half[1] + 1.0];
        let corners = [
            [s[0] - pad[0], s[1] - pad[1], 0.0],
            [s[0] + pad[0], s[1] + pad[1], 0.0],
            [e[0] - pad[0], e[1] - pad[1], 0.0],
            [e[0] + pad[0], e[1] + pad[1], 0.0],
        ];
        let (c0, c1) = Self::cell_range_of(self.min, self.dims, corners.into_iter());
        let zlo = s[2].min(e[2]) - half[2] - 1.0;
        let zhi = s[2].max(e[2]) + half[2] + 1.0;

        // A triangle spanning several cells is tested once.
        let mut candidates: Vec<u32> = Vec::new();
        for y in c0[1]..=c1[1] {
            for x in c0[0]..=c1[0] {
                candidates.extend_from_slice(&self.cells[y * self.dims[0] + x]);
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        let mut best_enter = f32::INFINITY;
        let mut best_normal = [0.0; 3];
        let mut start_solid = false;
        let mut all_solid = false;
        for &i in &candidates {
            let t = &self.tris[i as usize];
            let tz_lo = t[0][2].min(t[1][2]).min(t[2][2]);
            let tz_hi = t[0][2].max(t[1][2]).max(t[2][2]);
            if tz_hi < zlo || tz_lo > zhi {
                continue;
            }
            match sweep_triangle(s, d, half, t) {
                Hit::None => {}
                Hit::Solid { all } => {
                    start_solid = true;
                    all_solid |= all;
                }
                Hit::Enter { t: enter, normal } => {
                    if enter < best_enter {
                        best_enter = enter;
                        best_normal = normal;
                    }
                }
            }
        }
        if start_solid {
            out.startsolid = 1;
            out.allsolid = u8::from(all_solid);
            if all_solid {
                out.fraction = 0.0;
                out.endpos = start;
                out.contents = CONTENTS_SOLID;
                out.hit_type = HITTYPE_ENTITY;
                out.hit_id = ENTITYNUM_WORLD;
                return out;
            }
        }
        if best_enter <= 1.0 {
            let fraction = if dist > 0.0 { (best_enter - CLIP_EPSILON / dist).max(0.0) } else { 0.0 };
            out.fraction = fraction;
            out.normal = best_normal;
            out.endpos = [0, 1, 2].map(|a| start[a] + (end[a] - start[a]) * fraction);
            out.contents = CONTENTS_SOLID;
            out.surface_flags = SURFACE_CONCRETE;
            out.hit_type = HITTYPE_ENTITY;
            out.hit_id = ENTITYNUM_WORLD;
            out.walkable = u8::from(best_normal[2] >= 0.7);
        }
        out
    }

    /// Ray from `start` to `end`: hit fraction and normal, for bullets and sight lines.
    pub(crate) fn ray(&self, start: V, end: V) -> Option<(f32, V)> {
        let t = self.sweep(start, end, [0.0; 3], [0.0; 3]);
        (t.fraction < 1.0).then_some((t.fraction, t.normal))
    }
}

enum Hit {
    None,
    /// Already overlapping at the start; `all` if it never leaves.
    Solid { all: bool },
    Enter { t: f32, normal: V },
}

/// Swept SAT of a box (centre `s`, half extents `h`, motion `d`) against a triangle.
fn sweep_triangle(s: V, d: V, h: V, tri: &[V; 3]) -> Hit {
    let p = [sub(tri[0], s), sub(tri[1], s), sub(tri[2], s)];
    let edges = [sub(p[1], p[0]), sub(p[2], p[1]), sub(p[0], p[2])];
    let normal = cross(edges[0], edges[1]);
    let mut axes: [V; 13] = [[0.0; 3]; 13];
    axes[0] = [1.0, 0.0, 0.0];
    axes[1] = [0.0, 1.0, 0.0];
    axes[2] = [0.0, 0.0, 1.0];
    axes[3] = normal;
    let mut n = 4;
    for e in edges {
        for b in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
            axes[n] = cross(e, b);
            n += 1;
        }
    }
    let mut enter = f32::NEG_INFINITY;
    let mut exit = f32::INFINITY;
    let mut enter_axis = [0.0; 3];
    // Overlap tolerance: touching (resting on a floor) is not "inside".
    const TOUCH: f32 = 0.01;
    for axis in &axes[..n] {
        let l = len(*axis);
        if l < 1e-6 {
            continue;
        }
        let a = [axis[0] / l, axis[1] / l, axis[2] / l];
        let q = [dot(p[0], a), dot(p[1], a), dot(p[2], a)];
        let tmin = q[0].min(q[1]).min(q[2]);
        let tmax = q[0].max(q[1]).max(q[2]);
        let r = h[0] * a[0].abs() + h[1] * a[1].abs() + h[2] * a[2].abs();
        let v = dot(d, a);
        // Box interval at time t: [-r + t v, r + t v]; triangle interval [tmin, tmax].
        if v.abs() < 1e-9 {
            if tmin - r >= -TOUCH || -r - tmax >= -TOUCH {
                return Hit::None;
            }
            continue;
        }
        let t_a = (tmin - r) / v;
        let t_b = (tmax + r) / v;
        let (t0, t1) = if t_a < t_b { (t_a, t_b) } else { (t_b, t_a) };
        if t0 > enter {
            enter = t0;
            // Face against the motion.
            enter_axis = if v > 0.0 { [-a[0], -a[1], -a[2]] } else { a };
        }
        exit = exit.min(t1);
        if enter > exit {
            return Hit::None;
        }
    }
    if exit < 0.0 || enter > 1.0 {
        return Hit::None;
    }
    let dist = len(d).max(1e-6);
    if enter * dist < -TOUCH {
        return Hit::Solid { all: exit > 1.0 };
    }
    // Only stop on surfaces we move into.
    if dot(enter_axis, d) >= 0.0 {
        return Hit::None;
    }
    Hit::Enter { t: enter.max(0.0), normal: enter_axis }
}

/// The collision a single pmove call sees.
pub(crate) struct Backend<'a> {
    pub world: &'a ClipWorld,
}

impl CollisionBackend for Backend<'_> {
    fn trace(&self, input: GroundTraceInput) -> Trace {
        self.world.sweep(input.start, input.end, input.mins, input.maxs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor() -> ClipWorld {
        use bevy::math::Vec3;
        let a = Vec3::new(-10.0, 0.0, -10.0);
        let b = Vec3::new(10.0, 0.0, -10.0);
        let c = Vec3::new(10.0, 0.0, 10.0);
        let d = Vec3::new(-10.0, 0.0, 10.0);
        ClipWorld::new([[a, c, b], [a, d, c]].into_iter(), 2)
    }

    #[test]
    fn falling_box_lands_on_the_floor() {
        let w = floor();
        let t = w.sweep([0.0, 0.0, 100.0], [0.0, 0.0, -100.0], [-15.0, -15.0, 0.0], [15.0, 15.0, 70.0]);
        assert!(t.fraction < 1.0 && t.startsolid == 0);
        assert!((t.endpos[2] - 0.125).abs() < 0.05, "{:?}", t.endpos);
        assert!(t.normal[2] > 0.99 && t.walkable == 1);
    }

    #[test]
    fn resting_on_the_floor_is_not_solid() {
        let w = floor();
        let t = w.sweep([0.0, 0.0, 0.125], [50.0, 0.0, 0.125], [-15.0, -15.0, 0.0], [15.0, 15.0, 70.0]);
        assert_eq!(t.startsolid, 0);
        assert_eq!(t.fraction, 1.0);
    }

    #[test]
    fn units_round_trip() {
        let v = bevy::math::Vec3::new(1.0, 2.0, 3.0);
        assert!((from_iw(to_iw(v)) - v).length() < 1e-5);
    }
}
