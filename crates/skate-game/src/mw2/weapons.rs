//! Guns: published MW2 numbers, hitscan against bots and the map, and a
//! viewmodel built from boxes.

use super::audio::{Sfx, Sounds};
use super::clip::{dir_from_iw, from_iw};
use super::{Session, bots, vertical_fov};
use bevy::prelude::*;

#[derive(Clone)]
pub(crate) struct Gun {
    pub name: &'static str,
    pub sfx: Sfx,
    /// Damage at the near and far range (falls off linearly between).
    pub damage: (f32, f32),
    /// Ranges in IW4 units (inches).
    pub range: (f32, f32),
    pub rpm: f32,
    pub mag: u32,
    pub reserve: u32,
    pub reload_s: f32,
    pub auto: bool,
    pub pellets: u32,
    /// Cone half-angles in degrees.
    pub hip_spread: f32,
    pub ads_spread: f32,
    /// ADS zoom as MW2's 4:3 horizontal field of view.
    pub ads_fov: f32,
    pub kick: f32,
    pub head_mult: f32,
    pub body_mult: f32,
    pub move_scale: f32,
    pub ads_move_scale: f32,
    pub sprint_scale: f32,
    pub ads_ms: f32,
    pub scope: bool,
    look: Look,
}

#[derive(Clone, Copy)]
struct Look {
    body: f32,
    barrel: f32,
    mag: f32,
    stock: f32,
    scope: bool,
    pump: bool,
    tint: [f32; 3],
}

/// Primaries: ACR, M4A1, UMP45, Intervention. Secondaries: USP .45, SPAS-12.
pub(crate) fn guns() -> [Gun; 6] {
    [
        Gun {
            name: "ACR",
            sfx: Sfx::Rifle,
            damage: (40.0, 30.0),
            range: (1500.0, 2400.0),
            rpm: 750.0,
            mag: 30,
            reserve: 90,
            reload_s: 2.3,
            auto: true,
            pellets: 1,
            hip_spread: 3.2,
            ads_spread: 0.15,
            ads_fov: 50.0,
            kick: 0.55,
            head_mult: 1.4,
            body_mult: 1.0,
            move_scale: 0.95,
            ads_move_scale: 0.5,
            sprint_scale: 1.0,
            ads_ms: 250.0,
            scope: false,
            look: Look { body: 0.30, barrel: 0.16, mag: 0.11, stock: 0.14, scope: false, pump: false, tint: [0.16, 0.17, 0.15] },
        },
        Gun {
            name: "M4A1",
            sfx: Sfx::Rifle,
            damage: (30.0, 20.0),
            range: (1500.0, 2400.0),
            rpm: 800.0,
            mag: 30,
            reserve: 90,
            reload_s: 2.2,
            auto: true,
            pellets: 1,
            hip_spread: 3.0,
            ads_spread: 0.12,
            ads_fov: 50.0,
            kick: 0.4,
            head_mult: 1.4,
            body_mult: 1.0,
            move_scale: 0.95,
            ads_move_scale: 0.5,
            sprint_scale: 1.0,
            ads_ms: 230.0,
            scope: false,
            look: Look { body: 0.30, barrel: 0.15, mag: 0.11, stock: 0.15, scope: false, pump: false, tint: [0.11, 0.11, 0.1] },
        },
        Gun {
            name: "UMP45",
            sfx: Sfx::Smg,
            damage: (40.0, 35.0),
            range: (400.0, 1000.0),
            rpm: 666.0,
            mag: 32,
            reserve: 96,
            reload_s: 2.0,
            auto: true,
            pellets: 1,
            hip_spread: 2.4,
            ads_spread: 0.3,
            ads_fov: 55.0,
            kick: 0.45,
            head_mult: 1.4,
            body_mult: 1.0,
            move_scale: 1.0,
            ads_move_scale: 0.6,
            sprint_scale: 1.0,
            ads_ms: 200.0,
            scope: false,
            look: Look { body: 0.24, barrel: 0.07, mag: 0.13, stock: 0.11, scope: false, pump: false, tint: [0.12, 0.12, 0.12] },
        },
        Gun {
            name: "Intervention",
            sfx: Sfx::Sniper,
            damage: (98.0, 98.0),
            range: (100_000.0, 100_000.0),
            rpm: 46.0,
            mag: 5,
            reserve: 25,
            reload_s: 3.3,
            auto: false,
            pellets: 1,
            hip_spread: 8.0,
            ads_spread: 0.0,
            ads_fov: 15.0,
            kick: 3.5,
            head_mult: 1.5,
            // Upper body takes x1.5 too: one shot kills, the quickscope classic.
            body_mult: 1.5,
            move_scale: 0.95,
            ads_move_scale: 0.4,
            sprint_scale: 1.0,
            ads_ms: 330.0,
            scope: true,
            look: Look { body: 0.34, barrel: 0.30, mag: 0.06, stock: 0.17, scope: true, pump: false, tint: [0.24, 0.22, 0.17] },
        },
        Gun {
            name: "USP .45",
            sfx: Sfx::Smg,
            damage: (40.0, 30.0),
            range: (300.0, 1000.0),
            rpm: 600.0,
            mag: 12,
            reserve: 36,
            reload_s: 1.7,
            auto: false,
            pellets: 1,
            hip_spread: 2.0,
            ads_spread: 0.5,
            ads_fov: 58.0,
            kick: 1.1,
            head_mult: 1.4,
            body_mult: 1.0,
            move_scale: 1.0,
            ads_move_scale: 0.75,
            sprint_scale: 1.0,
            ads_ms: 150.0,
            scope: false,
            look: Look { body: 0.17, barrel: 0.02, mag: 0.0, stock: 0.0, scope: false, pump: false, tint: [0.09, 0.09, 0.09] },
        },
        Gun {
            name: "SPAS-12",
            sfx: Sfx::Shotgun,
            damage: (40.0, 20.0),
            range: (300.0, 800.0),
            rpm: 70.0,
            mag: 8,
            reserve: 24,
            reload_s: 3.0,
            auto: false,
            pellets: 8,
            hip_spread: 4.5,
            ads_spread: 4.0,
            ads_fov: 55.0,
            kick: 2.8,
            head_mult: 1.0,
            body_mult: 1.0,
            move_scale: 1.0,
            ads_move_scale: 0.6,
            sprint_scale: 1.0,
            ads_ms: 250.0,
            scope: false,
            look: Look { body: 0.30, barrel: 0.24, mag: 0.0, stock: 0.15, scope: false, pump: true, tint: [0.10, 0.10, 0.11] },
        },
    ]
}

pub(crate) struct Loadout {
    guns: [Gun; 6],
    ammo: [(u32, u32); 6],
    /// Primary and secondary gun indices; `current` is the one in hand.
    slots: [usize; 2],
    pub slot: usize,
    pub current: usize,
    pub class: super::class::Class,
    ready_at: f32,
    reload_until: Option<f32>,
    switch_until: f32,
    /// Degrees of view kick: x pitch (negative is up), y yaw.
    pub recoil: Vec2,
    kickback: f32,
    pub ads: f32,
    bloom: f32,
    pub hitmarker_until: f32,
    flash_until: f32,
    knife_ready: f32,
    knife_until: f32,
    pub throw_until: f32,
    /// The options' field of view (MW2 cg_fov).
    pub base_fov: f32,
}

impl Loadout {
    pub fn new(class: &super::class::Class) -> Self {
        let guns = guns();
        let ammo = std::array::from_fn(|i| (guns[i].mag, guns[i].reserve));
        let slots = [class.primary.min(5), class.secondary.min(5)];
        Self {
            guns,
            ammo,
            slots,
            slot: 0,
            current: slots[0],
            class: *class,
            ready_at: 0.0,
            reload_until: None,
            switch_until: 0.0,
            recoil: Vec2::ZERO,
            kickback: 0.0,
            ads: 0.0,
            bloom: 0.0,
            hitmarker_until: 0.0,
            flash_until: 0.0,
            knife_ready: 0.0,
            knife_until: 0.0,
            throw_until: 0.0,
            base_fov: 65.0,
        }
    }
    pub fn gun(&self) -> &Gun {
        &self.guns[self.current]
    }
    pub fn ammo(&self) -> (u32, u32) {
        self.ammo[self.current]
    }
    pub fn reloading(&self) -> bool {
        self.reload_until.is_some()
    }
    pub fn refill(&mut self) {
        for (i, g) in self.guns.iter().enumerate() {
            self.ammo[i] = (g.mag, g.reserve);
        }
        self.reload_until = None;
    }
    /// Vertical field of view in radians, between hip (65) and the gun's ADS zoom.
    pub fn fov(&self) -> f32 {
        let hip = vertical_fov(self.base_fov);
        let ads = vertical_fov(self.gun().ads_fov * self.base_fov / 65.0);
        hip + (ads - hip) * self.ads
    }
    /// Look sensitivity follows the zoom.
    pub fn zoom_scale(&self) -> f32 {
        (self.fov() / vertical_fov(self.base_fov)).clamp(0.2, 1.0)
    }
    pub fn crosshair_gap(&self) -> f32 {
        (self.hip_spread() + self.bloom) * (1.0 - self.ads)
    }
    fn hip_spread(&self) -> f32 {
        self.gun().hip_spread * if self.class.steady_aim() { 0.65 } else { 1.0 }
    }
    fn reload_time(&self) -> f32 {
        self.gun().reload_s * if self.class.sleight_of_hand() { 0.5 } else { 1.0 }
    }
    /// Scavenger: a fallen enemy's ammo tops up both guns.
    pub fn scavenge(&mut self) {
        for slot in self.slots {
            let g = &self.guns[slot];
            self.ammo[slot].1 = (self.ammo[slot].1 + g.mag).min(g.reserve * 2);
        }
    }
    pub fn scoped(&self) -> bool {
        self.gun().scope && self.ads > 0.9
    }
}

#[derive(Component)]
pub(crate) struct Fade(f32);

/// Despawn marker for short-lived effects.
pub(crate) fn fade_at(at: f32) -> Fade {
    Fade(at)
}

/// What the bots carry: the ACR's movement scales.
pub(crate) fn bot_gun() -> Gun {
    guns()[0].clone()
}

#[derive(Component)]
struct ViewmodelRoot;

#[derive(Resource, Default)]
struct Viewmodel {
    root: Option<Entity>,
    model: Option<Entity>,
    flash: Option<Entity>,
    built_for: Option<usize>,
}

#[derive(Resource)]
struct EffectAssets {
    cube: Handle<Mesh>,
    ball: Handle<Mesh>,
    tracer: Handle<StandardMaterial>,
    spark: Handle<StandardMaterial>,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Viewmodel>()
        .add_systems(Startup, effect_assets)
        .add_systems(Update, fade);
}

fn effect_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(EffectAssets {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        ball: meshes.add(Sphere::new(1.0)),
        tracer: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.85, 0.5),
            emissive: LinearRgba::rgb(6.0, 4.5, 1.8),
            unlit: true,
            ..default()
        }),
        spark: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.7, 0.3),
            emissive: LinearRgba::rgb(8.0, 5.0, 1.5),
            unlit: true,
            ..default()
        }),
    });
}

fn fade(mut commands: Commands, time: Res<Time>, q: Query<(Entity, &Fade)>) {
    let now = time.elapsed_secs();
    for (e, f) in &q {
        if now >= f.0 {
            commands.entity(e).despawn();
        }
    }
}

/// Show or hide the viewmodel, building it under the gameplay camera first.
pub(crate) fn show(world: &mut World, on: bool) {
    if on && world.resource::<Viewmodel>().root.is_none() {
        let camera = world
            .query_filtered::<Entity, With<crate::camera::GameplayCamera>>()
            .iter(world)
            .next();
        let Some(camera) = camera else { return };
        let root = world.spawn((ViewmodelRoot, Transform::default(), Visibility::Hidden)).id();
        world.entity_mut(camera).add_child(root);
        world.resource_mut::<Viewmodel>().root = Some(root);
    }
    if let Some(root) = world.resource::<Viewmodel>().root {
        if let Some(mut v) = world.get_mut::<Visibility>(root) {
            *v = if on { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
}

fn build_model(world: &mut World, root: Entity, look: Look) -> (Entity, Entity) {
    let mut meshes = world.resource_mut::<Assets<Mesh>>();
    let parts = [
        // (size, offset) in metres; the gun points down -Z.
        (Vec3::new(0.045, 0.07, look.body), Vec3::new(0.0, 0.0, -look.body * 0.5), 0),
        (Vec3::new(0.022, 0.022, look.barrel), Vec3::new(0.0, 0.012, -look.body - look.barrel * 0.5), 0),
        (Vec3::new(0.035, look.mag.max(0.001), 0.04), Vec3::new(0.0, -0.035 - look.mag * 0.5, -look.body * 0.35), 1),
        (Vec3::new(0.035, 0.06, look.stock), Vec3::new(0.0, -0.012, look.stock * 0.5), 1),
        (Vec3::new(0.03, 0.09, 0.035), Vec3::new(0.0, -0.07, -0.03), 1),
        (Vec3::new(0.04, 0.045, 0.12), Vec3::new(0.0, -0.035, -look.body - 0.02), if look.pump { 2 } else { 1 }),
        (Vec3::new(0.05, 0.05, 0.16), Vec3::new(0.0, 0.065, -look.body * 0.55), if look.scope { 0 } else { 3 }),
        (Vec3::new(0.012, 0.025, 0.012), Vec3::new(0.0, 0.045, -look.body * 0.9), if look.scope { 3 } else { 1 }),
        // Sleeves and gloves.
        (Vec3::new(0.08, 0.08, 0.32), Vec3::new(0.09, -0.11, 0.12), 4),
        (Vec3::new(0.06, 0.06, 0.08), Vec3::new(0.02, -0.07, -0.02), 5),
        (Vec3::new(0.08, 0.08, 0.36), Vec3::new(-0.09, -0.1, -look.body * 0.5), 4),
        (Vec3::new(0.06, 0.06, 0.08), Vec3::new(-0.02, -0.05, -look.body - 0.02), 5),
    ];
    let handles: Vec<_> = parts.iter().map(|(size, _, _)| meshes.add(Cuboid::from_size(*size))).collect();
    let flash_mesh = meshes.add(Sphere::new(0.035));
    let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
    let tint = Color::srgb(look.tint[0], look.tint[1], look.tint[2]);
    let palette = [
        materials.add(StandardMaterial { base_color: tint, perceptual_roughness: 0.55, metallic: 0.4, ..default() }),
        materials.add(StandardMaterial { base_color: Color::srgb(0.09, 0.09, 0.09), perceptual_roughness: 0.8, ..default() }),
        materials.add(StandardMaterial { base_color: Color::srgb(0.33, 0.24, 0.15), perceptual_roughness: 0.7, ..default() }),
        Handle::default(),
        materials.add(StandardMaterial { base_color: Color::srgb(0.42, 0.38, 0.28), perceptual_roughness: 0.95, ..default() }),
        materials.add(StandardMaterial { base_color: Color::srgb(0.12, 0.11, 0.1), perceptual_roughness: 0.9, ..default() }),
    ];
    let flash_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.8, 0.4),
        emissive: LinearRgba::rgb(20.0, 12.0, 4.0),
        unlit: true,
        ..default()
    });
    let model = world.spawn((Transform::default(), Visibility::Inherited)).id();
    for ((size, offset, mat), mesh) in parts.iter().zip(handles) {
        // Slot 3 marks a part this gun does not have.
        if size.y <= 0.002 || *mat == 3 {
            continue;
        }
        let child = world
            .spawn((Mesh3d(mesh), MeshMaterial3d(palette[*mat].clone()), Transform::from_translation(*offset)))
            .id();
        world.entity_mut(model).add_child(child);
    }
    let flash = world
        .spawn((
            Mesh3d(flash_mesh),
            MeshMaterial3d(flash_material),
            Transform::from_xyz(0.0, 0.012, -look.body - look.barrel - 0.03),
            Visibility::Hidden,
            PointLight { intensity: 60_000.0, range: 6.0, color: Color::srgb(1.0, 0.75, 0.4), ..default() },
        ))
        .id();
    world.entity_mut(model).add_child(flash);
    world.entity_mut(root).add_child(model);
    (model, flash)
}

fn spawn_fx(world: &mut World, from: Vec3, to: Vec3, impact: bool, now: f32) {
    let Some(fx) = world.get_resource::<EffectAssets>() else { return };
    let (cube, ball, tracer, spark) = (fx.cube.clone(), fx.ball.clone(), fx.tracer.clone(), fx.spark.clone());
    let length = from.distance(to);
    if length > 0.5 {
        let mid = from.lerp(to, 0.5);
        world.spawn((
            Mesh3d(cube),
            MeshMaterial3d(tracer),
            Transform::from_translation(mid).looking_at(to, Vec3::Y).with_scale(Vec3::new(0.01, 0.01, length.min(60.0))),
            Fade(now + 0.04),
        ));
    }
    if impact {
        world.spawn((
            Mesh3d(ball),
            MeshMaterial3d(spark),
            Transform::from_translation(to).with_scale(Vec3::splat(0.05)),
            Fade(now + 0.08),
        ));
    }
}

pub(crate) fn tick(world: &mut World, s: &mut Session, dt: f32) {
    let now = s.now;
    let c = s.controls;
    let dead = s.dead_until.is_some();

    // Weapon switch: 1-4, Q or Y.
    // Primary / secondary: wheel, middle click or Y swap; 1 and 2 pick.
    let wanted = c.weapon_slot.filter(|w| *w < 2).or(c.next_weapon.then(|| 1 - s.weapons.slot));
    if let Some(slot) = wanted.filter(|w| *w != s.weapons.slot && !dead) {
        s.weapons.slot = slot;
        s.weapons.current = s.weapons.slots[slot];
        s.weapons.reload_until = None;
        s.weapons.switch_until = now + 0.45;
    }

    // ADS comes from pmove's own weapon position fraction.
    s.weapons.ads = s.ps.f_weapon_pos_frac.clamp(0.0, 1.0);
    let moving = (s.ps.velocity[0].powi(2) + s.ps.velocity[1].powi(2)).sqrt();
    let target_bloom = (moving / 190.0).min(1.5) * 2.0 + if s.ps.ground_entity_num == 0x7ff { 3.0 } else { 0.0 };
    s.weapons.bloom += (target_bloom - s.weapons.bloom) * (dt * 8.0).min(1.0);
    let recover = (dt * 7.0).min(1.0);
    s.weapons.recoil -= s.weapons.recoil * recover;
    s.weapons.kickback -= s.weapons.kickback * (dt * 14.0).min(1.0);

    let gun = s.weapons.gun().clone();
    let i = s.weapons.current;
    if let Some(until) = s.weapons.reload_until {
        if now >= until {
            let (mag, reserve) = s.weapons.ammo[i];
            let take = (gun.mag - mag).min(reserve);
            s.weapons.ammo[i] = (mag + take, reserve - take);
            s.weapons.reload_until = None;
        }
    }
    let (mag, reserve) = s.weapons.ammo[i];
    let busy = s.weapons.reloading() || now < s.weapons.switch_until || dead;
    if c.reload && !busy && mag < gun.mag && reserve > 0 {
        s.weapons.reload_until = Some(now + s.weapons.reload_time());
        play(world, Sfx::Reload, 0.7);
    }
    // Knife: MW2's melee kills in one hit at lunge range.
    if c.knife && !dead && now >= s.weapons.knife_ready {
        s.weapons.knife_ready = now + 0.9;
        s.weapons.knife_until = now + 0.35;
        s.weapons.reload_until = None;
        play(world, Sfx::Knife, 0.7);
        let eye = s.eye();
        let f = s.view_dir();
        let target = s.bots.iter().enumerate().filter(|(_, b)| b.alive).find_map(|(i, b)| {
            let d = [b.ps.origin[0] - eye[0], b.ps.origin[1] - eye[1]];
            let dist = (d[0] * d[0] + d[1] * d[1]).sqrt();
            let ahead = (d[0] * f[0] + d[1] * f[1]) / dist.max(1.0);
            (dist < if s.weapons.class.commando() { 180.0 } else { 90.0 } && ahead > 0.5 && (b.ps.origin[2] - s.ps.origin[2]).abs() < 60.0).then_some(i)
        });
        if let Some(i) = target {
            s.weapons.hitmarker_until = now + 0.18;
            play(world, Sfx::HitMarker, 0.7);
            if bots::damage(s, i, 200.0, "Knife", false) {
                play(world, Sfx::Kill, 0.6);
            }
        }
    }
    let busy = busy || now < s.weapons.knife_until || now < s.weapons.throw_until;
    let sprinting = s.ps.pm_flags & playerstate_iw4::pm_flags::SPRINTING != 0;
    let trigger = if gun.auto { c.fire } else { c.fire_pressed };
    if trigger && !busy && !sprinting && now >= s.weapons.ready_at {
        if mag == 0 {
            if c.fire_pressed {
                play(world, Sfx::Empty, 0.6);
                if reserve > 0 {
                    s.weapons.reload_until = Some(now + s.weapons.reload_time());
                    play(world, Sfx::Reload, 0.7);
                }
            }
        } else {
            s.weapons.ammo[i].0 -= 1;
            s.weapons.ready_at = now + 60.0 / gun.rpm;
            fire(world, s, &gun);
        }
    }
    animate_viewmodel(world, s, &gun, dt);
}

fn play(world: &mut World, sfx: Sfx, gain: f32) {
    world.resource_scope(|world, sounds: Mut<Sounds>| sounds.play(&mut world.commands(), sfx, gain));
}

fn fire(world: &mut World, s: &mut Session, gun: &Gun) {
    let now = s.now;
    play(world, gun.sfx, 0.8);
    bots::hear_shot(s);
    s.weapons.flash_until = now + 0.05;
    s.weapons.kickback = 1.0;
    let ads = s.weapons.ads;
    let spread = (s.weapons.hip_spread() + s.weapons.bloom) * (1.0 - ads) + gun.ads_spread * ads;
    let eye = s.eye();
    let dir = s.view_dir();
    // A basis around the view for the spread cone.
    let f = Vec3::from_array(dir).normalize_or(Vec3::X);
    let r = f.cross(Vec3::Z).normalize_or(Vec3::Y);
    let u = r.cross(f);
    let mut hit_any = false;
    let mut killed = Vec::new();
    for _ in 0..gun.pellets.max(1) {
        let angle = s.random() * std::f32::consts::TAU;
        let radius = spread.to_radians().tan() * s.random().sqrt();
        let d = (f + (r * angle.cos() + u * angle.sin()) * radius).normalize();
        let reach = 12_000.0;
        let end = [eye[0] + d.x * reach, eye[1] + d.y * reach, eye[2] + d.z * reach];
        let world_hit = s.clip.ray(eye, end).map(|(t, _)| t * reach).unwrap_or(f32::INFINITY);
        let bot_hit = bots::raycast(s, eye, d.to_array(), world_hit);
        let (dist, impact) = match bot_hit {
            Some((idx, dist, head)) => {
                let falloff = ((dist - gun.range.0) / (gun.range.1 - gun.range.0).max(1.0)).clamp(0.0, 1.0);
                let base = (gun.damage.0 + (gun.damage.1 - gun.damage.0) * falloff)
                    * if s.weapons.class.stopping_power() { 1.4 } else { 1.0 };
                let amount = base * if head { gun.head_mult } else { gun.body_mult };
                hit_any = true;
                if bots::damage(s, idx, amount, gun.name, head) {
                    killed.push(idx);
                }
                (dist, false)
            }
            None => (world_hit.min(reach), world_hit.is_finite()),
        };
        let to = [eye[0] + d.x * dist, eye[1] + d.y * dist, eye[2] + d.z * dist];
        let muzzle = from_iw(eye) + dir_from_iw(dir).normalize() * 0.6 + Vec3::NEG_Y * 0.08;
        spawn_fx(world, muzzle, from_iw(to), impact, now);
    }
    if hit_any {
        s.weapons.hitmarker_until = now + 0.18;
        play(world, Sfx::HitMarker, 0.7);
    }
    if !killed.is_empty() {
        play(world, Sfx::Kill, 0.6);
    }
    // View kick: up, with a little random sideways drift.
    let side = (s.random() - 0.5) * gun.kick * 0.6;
    let up = -gun.kick * (0.7 + 0.3 * s.random());
    s.weapons.recoil += Vec2::new(up, side);
    // Some of the kick stays, as MW2's recoil does.
    s.pitch -= gun.kick * 0.25;
}

fn animate_viewmodel(world: &mut World, s: &Session, gun: &Gun, _dt: f32) {
    let current = s.weapons.current;
    let vm = world.resource::<Viewmodel>();
    let (root, built_for, model, flash) = (vm.root, vm.built_for, vm.model, vm.flash);
    let Some(root) = root else { return };
    if built_for != Some(current) {
        if let Some(old) = model {
            world.entity_mut(old).despawn();
        }
        let (model, flash) = build_model(world, root, gun.look);
        let mut vm = world.resource_mut::<Viewmodel>();
        vm.model = Some(model);
        vm.flash = Some(flash);
        vm.built_for = Some(current);
    }
    let w = &s.weapons;
    let ads = w.ads;
    // Hip: low right. ADS: centred on the sight line (the scope hides it).
    let hip = Vec3::new(0.12, -0.13, -0.36);
    let aim = Vec3::new(0.0, -0.075 - if gun.scope { 0.03 } else { 0.0 }, -0.2);
    let mut pos = hip.lerp(aim, ads);
    let speed = (s.ps.velocity[0].powi(2) + s.ps.velocity[1].powi(2)).sqrt() / 190.0;
    let phase = s.now * 9.0;
    let bob = (1.0 - ads * 0.85) * speed.min(1.6);
    pos += Vec3::new(phase.sin() * 0.008, (phase * 2.0).sin().abs() * -0.01, 0.0) * bob;
    pos.z += w.kickback * 0.04;
    let mut rot = Quat::IDENTITY;
    if s.ps.pm_flags & playerstate_iw4::pm_flags::SPRINTING != 0 {
        rot = Quat::from_euler(EulerRot::YXZ, 0.6, -0.25, 0.3);
        pos += Vec3::new(-0.04, -0.03, 0.03);
    }
    if let Some(until) = w.reload_until {
        let left = (until - s.now) / w.reload_time().max(0.1);
        let dip = (left * std::f32::consts::PI).sin().max(0.0);
        rot *= Quat::from_rotation_x(-0.6 * dip) * Quat::from_rotation_z(0.4 * dip);
        pos.y -= 0.06 * dip;
    }
    if s.now < w.switch_until {
        pos.y -= (w.switch_until - s.now) * 0.5;
    }
    if s.now < w.throw_until {
        pos.y -= 0.15 * ((w.throw_until - s.now) / 0.5);
    }
    if s.now < w.knife_until {
        // Knife swing: the gun swipes across and forward.
        let k = 1.0 - (w.knife_until - s.now) / 0.35;
        let swing = (k * std::f32::consts::PI).sin();
        pos += Vec3::new(-0.12 * swing, 0.03 * swing, -0.1 * swing);
        rot *= Quat::from_rotation_z(0.9 * swing) * Quat::from_rotation_y(0.5 * swing);
    }
    if w.scoped() || s.dead_until.is_some() {
        pos.y -= 1.0;
    }
    if let Some(mut t) = world.get_mut::<Transform>(root) {
        *t = Transform::from_translation(pos)
            .with_rotation(rot * Quat::from_rotation_x(w.kickback * 0.05))
            .with_scale(Vec3::splat(0.7));
    }
    if let Some(flash) = world.resource::<Viewmodel>().flash {
        let on = s.now < w.flash_until;
        if let Some(mut v) = world.get_mut::<Visibility>(flash) {
            *v = if on { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
}
