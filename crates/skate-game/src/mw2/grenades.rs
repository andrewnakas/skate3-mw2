//! Frag and smoke grenades: one of each per life, as in MW2's default class.
//! Projectiles fly in IW4 space and bounce off the map's collision.

use super::audio::Sfx;
use super::clip::from_iw;
use super::{Session, bots, play};
use bevy::prelude::*;

const GRAVITY: f32 = 800.0;
const FRAG_FUSE: f32 = 3.5;
const FRAG_RADIUS: f32 = 256.0;
const FRAG_DAMAGE: f32 = 130.0;
const SMOKE_DELAY: f32 = 1.2;
const SMOKE_TIME: f32 = 15.0;
pub(crate) const SMOKE_RADIUS: f32 = 190.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Frag,
    Smoke,
}

pub(crate) struct Grenade {
    kind: Kind,
    pos: [f32; 3],
    vel: [f32; 3],
    thrown: f32,
    entity: Option<Entity>,
}

/// A smoke cloud: blocks sight lines until `until`.
pub(crate) struct Cloud {
    pub centre: [f32; 3],
    pub until: f32,
    entity: Option<Entity>,
}

#[derive(Default)]
pub(crate) struct Grenades {
    pub frags: u32,
    pub smokes: u32,
    flying: Vec<Grenade>,
    pub clouds: Vec<Cloud>,
}

impl Grenades {
    pub fn refill(&mut self) {
        self.frags = 1;
        self.smokes = 1;
    }
}

#[derive(Resource)]
struct GrenadeAssets {
    ball: Handle<Mesh>,
    body: Handle<StandardMaterial>,
    flash: Handle<StandardMaterial>,
    smoke: Handle<StandardMaterial>,
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(Startup, assets);
}

fn assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(GrenadeAssets {
        ball: meshes.add(Sphere::new(1.0)),
        body: materials.add(StandardMaterial { base_color: Color::srgb(0.2, 0.24, 0.17), perceptual_roughness: 0.7, ..default() }),
        flash: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.7, 0.3),
            emissive: LinearRgba::rgb(30.0, 16.0, 4.0),
            unlit: true,
            ..default()
        }),
        smoke: materials.add(StandardMaterial {
            base_color: Color::srgba(0.72, 0.72, 0.7, 0.55),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 1.0,
            ..default()
        }),
    });
}

/// Does the segment pass through a live smoke cloud?
pub(crate) fn smoke_blocks(g: &Grenades, now: f32, a: [f32; 3], b: [f32; 3]) -> bool {
    g.clouds.iter().filter(|c| now < c.until).any(|c| {
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c.centre[0] - a[0], c.centre[1] - a[1], c.centre[2] - a[2]];
        let len2 = (ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2]).max(1e-3);
        let t = ((ac[0] * ab[0] + ac[1] * ab[1] + ac[2] * ab[2]) / len2).clamp(0.0, 1.0);
        let p = [a[0] + ab[0] * t - c.centre[0], a[1] + ab[1] * t - c.centre[1], a[2] + ab[2] * t - c.centre[2]];
        p[0] * p[0] + p[1] * p[1] + p[2] * p[2] < SMOKE_RADIUS * SMOKE_RADIUS
    })
}

pub(crate) fn throw(world: &mut World, s: &mut Session, kind: Kind) {
    let have = match kind {
        Kind::Frag => &mut s.grenades.frags,
        Kind::Smoke => &mut s.grenades.smokes,
    };
    if *have == 0 || s.dead_until.is_some() {
        return;
    }
    *have -= 1;
    let eye = s.eye();
    let d = s.view_dir();
    let speed = 900.0;
    let vel = [d[0] * speed, d[1] * speed, d[2] * speed + 160.0];
    let pos = [eye[0] + d[0] * 16.0, eye[1] + d[1] * 16.0, eye[2] - 6.0 + d[2] * 16.0];
    let entity = world.get_resource::<GrenadeAssets>().map(|a| (a.ball.clone(), a.body.clone())).map(|(mesh, mat)| {
        world.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_translation(from_iw(pos)).with_scale(Vec3::splat(0.05)))).id()
    });
    s.grenades.flying.push(Grenade { kind, pos, vel, thrown: s.now, entity });
    s.weapons.throw_until = s.now + 0.5;
    play(world, Sfx::Knife, 0.4);
}

pub(crate) fn tick(world: &mut World, s: &mut Session, dt: f32) {
    let now = s.now;
    let clip = s.clip.clone();
    let mut explode = Vec::new();
    for g in &mut s.grenades.flying {
        g.vel[2] -= GRAVITY * dt;
        let end = [g.pos[0] + g.vel[0] * dt, g.pos[1] + g.vel[1] * dt, g.pos[2] + g.vel[2] * dt];
        let t = clip.sweep(g.pos, end, [-2.0; 3], [2.0; 3]);
        if t.fraction < 1.0 && t.startsolid == 0 {
            g.pos = t.endpos;
            // Bounce: reflect, lose most of the speed.
            let n = t.normal;
            let vn = g.vel[0] * n[0] + g.vel[1] * n[1] + g.vel[2] * n[2];
            g.vel = [0, 1, 2].map(|a| (g.vel[a] - 2.0 * vn * n[a]) * 0.35);
        } else if t.startsolid == 0 {
            g.pos = end;
        }
        if let Some(e) = g.entity {
            if let Some(mut tr) = world.get_mut::<Transform>(e) {
                tr.translation = from_iw(g.pos);
            }
        }
        let fuse = match g.kind {
            Kind::Frag => FRAG_FUSE,
            Kind::Smoke => SMOKE_DELAY,
        };
        if now - g.thrown >= fuse {
            explode.push((g.kind, g.pos, g.entity));
        }
    }
    s.grenades.flying.retain(|g| now - g.thrown < match g.kind { Kind::Frag => FRAG_FUSE, Kind::Smoke => SMOKE_DELAY });
    for (kind, pos, entity) in explode {
        if let Some(e) = entity {
            world.entity_mut(e).despawn();
        }
        match kind {
            Kind::Frag => detonate(world, s, pos),
            Kind::Smoke => pop_smoke(world, s, pos),
        }
    }
    s.grenades.clouds.retain(|c| {
        if now < c.until {
            return true;
        }
        if let Some(e) = c.entity {
            world.entity_mut(e).despawn();
        }
        false
    });
}

fn detonate(world: &mut World, s: &mut Session, pos: [f32; 3]) {
    play(world, Sfx::Explosion, 1.0);
    if let Some(a) = world.get_resource::<GrenadeAssets>() {
        let (mesh, mat) = (a.ball.clone(), a.flash.clone());
        world.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::from_translation(from_iw(pos)).with_scale(Vec3::splat(1.6)),
            PointLight { intensity: 2_000_000.0, range: 18.0, color: Color::srgb(1.0, 0.7, 0.35), ..default() },
            super::weapons::fade_at(s.now + 0.12),
        ));
    }
    let clip = s.clip.clone();
    // Bots in the blast, if the blast can reach them.
    for i in 0..s.bots.len() {
        let b = &s.bots[i];
        if !b.alive {
            continue;
        }
        let c = [b.ps.origin[0], b.ps.origin[1], b.ps.origin[2] + 36.0];
        let d = ((c[0] - pos[0]).powi(2) + (c[1] - pos[1]).powi(2) + (c[2] - pos[2]).powi(2)).sqrt();
        if d < FRAG_RADIUS && clip.ray([pos[0], pos[1], pos[2] + 4.0], c).is_none() {
            let damage = FRAG_DAMAGE * (1.0 - d / FRAG_RADIUS) + 20.0;
            if bots::damage(s, i, damage, "Frag", false) {
                play(world, Sfx::Kill, 0.6);
            }
            s.weapons.hitmarker_until = s.now + 0.18;
        }
    }
    // The player too, as in MW2.
    if s.dead_until.is_none() {
        let c = [s.ps.origin[0], s.ps.origin[1], s.ps.origin[2] + 36.0];
        let d = ((c[0] - pos[0]).powi(2) + (c[1] - pos[1]).powi(2) + (c[2] - pos[2]).powi(2)).sqrt();
        if d < FRAG_RADIUS && clip.ray([pos[0], pos[1], pos[2] + 4.0], c).is_none() {
            let protected = std::mem::replace(&mut s.protected_until, 0.0);
            super::hurt(s, FRAG_DAMAGE * (1.0 - d / FRAG_RADIUS), "Your own grenade");
            s.protected_until = protected;
        }
    }
}

fn pop_smoke(world: &mut World, s: &mut Session, pos: [f32; 3]) {
    play(world, Sfx::Smoke, 0.7);
    let entity = world.get_resource::<GrenadeAssets>().map(|a| (a.ball.clone(), a.smoke.clone())).map(|(mesh, mat)| {
        let root = world.spawn((Transform::from_translation(from_iw(pos)), Visibility::Inherited)).id();
        let r = SMOKE_RADIUS * super::clip::UNIT;
        for (i, off) in [[0.0, 0.5, 0.0], [0.6, 0.35, 0.2], [-0.5, 0.4, -0.3], [0.2, 0.9, -0.5], [-0.3, 1.0, 0.5], [0.1, 1.4, 0.0]].iter().enumerate() {
            let scale = r * (0.75 - i as f32 * 0.05);
            let child = world
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_translation(Vec3::from_array(*off) * r).with_scale(Vec3::splat(scale)),
                ))
                .id();
            world.entity_mut(root).add_child(child);
        }
        root
    });
    s.grenades.clouds.push(Cloud { centre: [pos[0], pos[1], pos[2] + 40.0], until: s.now + SMOKE_TIME, entity });
}
