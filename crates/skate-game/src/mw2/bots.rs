//! Enemy bots. They move with the same IW4 pmove as the player, so they step,
//! jump and collide with the skate map the same way; bodies are primitives.

use super::audio::{Sfx, Sounds};
use super::clip::from_iw;
use super::{Session, pmove_context, spawn_player_state, weapons};
use bevy::prelude::*;
use movement_iw4::{ANGLE2SHORT, CreateAnimsMantleRootDelta, FlatMantleAnimLength};
use playerstate_iw4::{PlayerState, UserCmd, buttons, pm_flags};

const COUNT: usize = 6;
/// Half-angle of what a bot can see, in degrees.
const VIEW_HALF_ANGLE: f32 = 65.0;
/// How many bots may fight the player at the same time.
const MAX_ENGAGED: usize = 2;
const NAMES: [&str; COUNT] = ["Viper", "Nomad", "Hex", "Rook", "Jinx", "Bishop"];
const RADIUS: f32 = 15.0;
const HEAD_RADIUS: f32 = 7.0;

pub(crate) struct Bot {
    pub ps: PlayerState,
    pub alive: bool,
    pub name: &'static str,
    /// When this bot last fired (red dot on the minimap, as in MW2).
    pub last_shot: f32,
    pub kills: u32,
    pub deaths: u32,
    pub yaw: f32,
    health: f32,
    respawn_at: f32,
    died_at: f32,
    entity: Option<Entity>,
    seen_since: Option<f32>,
    next_shot: f32,
    burst: u32,
    strafe: f32,
    strafe_until: f32,
    stuck: f32,
    jump: bool,
    old_buttons: u32,
    ms_carry: f32,
    /// Patrol target when the bot does not know where the player is.
    goal: [f32; 3],
    goal_until: f32,
    /// Heard a shot or got hit: hunts the player until then.
    alert_until: f32,
    /// Fighting the player; at most `MAX_ENGAGED` at once.
    engaged: bool,
}

#[derive(Resource)]
struct BotAssets {
    box_mesh: Handle<Mesh>,
    head_mesh: Handle<Mesh>,
    uniform: Handle<StandardMaterial>,
    vest: Handle<StandardMaterial>,
    skin: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    band: Handle<StandardMaterial>,
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(Startup, assets);
}

fn assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mut mat = |r, g, b| materials.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: 0.9, ..default() });
    commands.insert_resource(BotAssets {
        box_mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        head_mesh: meshes.add(Sphere::new(1.0)),
        uniform: mat(0.48, 0.42, 0.3),
        vest: mat(0.25, 0.24, 0.18),
        skin: mat(0.62, 0.47, 0.36),
        dark: mat(0.08, 0.08, 0.08),
        band: mat(0.75, 0.08, 0.06),
    });
}

/// A soldier from boxes, facing -Z, feet at the origin, in metres.
fn spawn_body(world: &mut World) -> Entity {
    let a = world.resource::<BotAssets>();
    let (cube, ball) = (a.box_mesh.clone(), a.head_mesh.clone());
    let parts = [
        (Vec3::new(0.14, 0.82, 0.16), Vec3::new(-0.11, 0.41, 0.0), a.uniform.clone(), false),
        (Vec3::new(0.14, 0.82, 0.16), Vec3::new(0.11, 0.41, 0.0), a.uniform.clone(), false),
        (Vec3::new(0.42, 0.58, 0.24), Vec3::new(0.0, 1.12, 0.0), a.vest.clone(), false),
        (Vec3::new(0.11, 0.5, 0.11), Vec3::new(-0.27, 1.12, -0.12), a.uniform.clone(), false),
        (Vec3::new(0.11, 0.5, 0.11), Vec3::new(0.27, 1.12, -0.12), a.uniform.clone(), false),
        (Vec3::new(0.12, 0.06, 0.12), Vec3::new(0.27, 1.32, -0.12), a.band.clone(), false),
        (Vec3::new(0.06, 0.1, 0.75), Vec3::new(0.16, 1.2, -0.42), a.dark.clone(), false),
        (Vec3::splat(0.12), Vec3::new(0.0, 1.62, 0.0), a.skin.clone(), true),
        (Vec3::new(0.27, 0.12, 0.28), Vec3::new(0.0, 1.71, 0.0), a.vest.clone(), false),
    ];
    let root = world.spawn((Transform::default(), Visibility::Hidden)).id();
    for (size, offset, material, round) in parts {
        let mesh = if round { ball.clone() } else { cube.clone() };
        let child = world
            .spawn((Mesh3d(mesh), MeshMaterial3d(material), Transform::from_translation(offset).with_scale(size)))
            .id();
        world.entity_mut(root).add_child(child);
    }
    root
}

pub(crate) fn show(world: &mut World, on: bool) {
    let Some(s) = world.get_resource::<Session>() else { return };
    let entities: Vec<_> = s.bots.iter().filter(|b| b.alive || on).filter_map(|b| b.entity).collect();
    for e in entities {
        if let Some(mut v) = world.get_mut::<Visibility>(e) {
            *v = if on { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
}

fn place(bot: &mut Bot, spot: [f32; 3], toward: [f32; 3], now: f32) {
    let yaw = (toward[1] - spot[1]).atan2(toward[0] - spot[0]).to_degrees();
    bot.ps = spawn_player_state(spot, yaw);
    bot.yaw = yaw;
    bot.health = 100.0;
    bot.alive = true;
    bot.seen_since = None;
    bot.next_shot = now + 1.0;
    bot.stuck = 0.0;
    bot.engaged = false;
    bot.alert_until = 0.0;
    bot.goal_until = 0.0;
}

pub(crate) fn tick(world: &mut World, s: &mut Session, dt: f32) {
    let now = s.now;
    // Small parks get fewer bots: 3 below 60 m across, 6 from 120 m.
    let count = {
        let (lo, hi) = s.spots.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), p| {
            ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])
        });
        let across = ((hi[0] - lo[0]).max(hi[1] - lo[1]) * super::clip::UNIT).max(0.0);
        (3 + ((across - 60.0) / 20.0).clamp(0.0, 3.0) as usize).min(COUNT)
    };
    if s.bots.len() < count && !s.spots.is_empty() {
        for name in NAMES.iter().take(count).skip(s.bots.len()) {
            let entity = spawn_body(world);
            s.bots.push(Bot {
                ps: PlayerState::ZERO,
                alive: false,
                name: *name,
                last_shot: -100.0,
                kills: 0,
                deaths: 0,
                yaw: 0.0,
                health: 0.0,
                respawn_at: now + 0.5 + s.bots.len() as f32 * 0.7,
                died_at: -100.0,
                entity: Some(entity),
                seen_since: None,
                next_shot: 0.0,
                burst: 0,
                strafe: 0.0,
                strafe_until: 0.0,
                stuck: 0.0,
                jump: false,
                old_buttons: 0,
                ms_carry: 0.0,
                goal: [0.0; 3],
                goal_until: 0.0,
                alert_until: 0.0,
                engaged: false,
            });
        }
    }
    let gun = weapons::bot_gun();
    let player_alive = s.dead_until.is_none();
    let protected = now < s.protected_until || s.match_over_until.is_some();
    let player_eye = s.eye();
    let player_speed = (s.ps.velocity[0].powi(2) + s.ps.velocity[1].powi(2)).sqrt();
    let ninja = s.weapons.class.ninja();
    let clip = s.clip.clone();
    for i in 0..s.bots.len() {
        if !s.bots[i].alive {
            if now >= s.bots[i].respawn_at {
                let origin = s.ps.origin;
                if let Some(spot) = s.spawn_spot(origin, 900.0, 3500.0) {
                    place(&mut s.bots[i], spot, origin, now);
                    if let Some(e) = s.bots[i].entity {
                        if let Some(mut v) = world.get_mut::<Visibility>(e) {
                            *v = Visibility::Inherited;
                        }
                    }
                } else {
                    s.bots[i].respawn_at = now + 1.0;
                }
            }
            update_body(world, &s.bots[i], now);
            continue;
        }
        let r1 = s.random();
        let r2 = s.random();
        let pick = ((s.random() * s.spots.len() as f32) as usize).min(s.spots.len() - 1);
        let patrol = s.spots[pick];
        let engaged_others = s.bots.iter().enumerate().filter(|(j, b)| *j != i && b.alive && b.engaged).count();
        let bot = &mut s.bots[i];
        let eye = [bot.ps.origin[0], bot.ps.origin[1], bot.ps.origin[2] + bot.ps.view_height_current];
        let to = [player_eye[0] - eye[0], player_eye[1] - eye[1], player_eye[2] - eye[2]];
        let flat = (to[0] * to[0] + to[1] * to[1]).sqrt();
        let dist = (flat * flat + to[2] * to[2]).sqrt();
        let facing = {
            let want = to[1].atan2(to[0]).to_degrees();
            ((want - bot.yaw + 540.0).rem_euclid(360.0) - 180.0).abs() < VIEW_HALF_ANGLE
        };
        // Footsteps: a moving player is heard nearby, unless they run Ninja.
        if !ninja && player_alive && player_speed > 120.0 && dist < 600.0 {
            bot.alert_until = bot.alert_until.max(now + 2.0);
        }
        let alerted = now < bot.alert_until;
        let visible = player_alive && dist < 6000.0 && (facing || alerted || bot.engaged)
            && clip.ray(eye, player_eye).is_none()
            && !super::grenades::smoke_blocks(&s.grenades, now, eye, player_eye);
        bot.engaged = visible && (bot.engaged || engaged_others < MAX_ENGAGED);
        if bot.engaged {
            bot.seen_since.get_or_insert(now);
        } else {
            bot.seen_since = None;
        }
        let hunting = bot.engaged || (alerted && player_alive);
        if !hunting && (now >= bot.goal_until || {
            let g = [bot.goal[0] - bot.ps.origin[0], bot.goal[1] - bot.ps.origin[1]];
            g[0] * g[0] + g[1] * g[1] < 80.0 * 80.0
        }) {
            bot.goal = patrol;
            bot.goal_until = now + 6.0 + r2 * 6.0;
        }

        // Face the player when hunting, else the patrol goal, at a human turn rate.
        let want = if hunting {
            to[1].atan2(to[0]).to_degrees()
        } else {
            (bot.goal[1] - bot.ps.origin[1]).atan2(bot.goal[0] - bot.ps.origin[0]).to_degrees()
        };
        let mut delta = (want - bot.yaw + 540.0).rem_euclid(360.0) - 180.0;
        delta = delta.clamp(-160.0 * dt, 160.0 * dt);
        bot.yaw += delta;
        let pitch = -(to[2].atan2(flat.max(1.0))).to_degrees();

        if now >= bot.strafe_until {
            bot.strafe = (if r1 < 0.5 { -1.0 } else { 1.0 }) * (0.5 + r2 * 0.5);
            bot.strafe_until = now + 0.8 + r1 * 1.4;
        }
        let (forward, right) = if !hunting {
            (0.55, 0.0)
        } else if !bot.engaged {
            (1.0, bot.strafe * 0.3)
        } else if dist > 1400.0 {
            (0.6, bot.strafe)
        } else if dist < 500.0 {
            (-0.6, bot.strafe)
        } else {
            (0.0, bot.strafe)
        };

        let mut b = 0;
        if bot.jump {
            b |= buttons::JUMP;
            bot.jump = false;
        }
        bot.ms_carry += dt * 1000.0;
        let msec = bot.ms_carry.floor() as i32;
        if msec >= 1 {
            bot.ms_carry -= msec as f32;
            let mut cmd = UserCmd {
                server_time: bot.ps.command_time.wrapping_add(msec),
                buttons: b,
                angles: [(pitch * ANGLE2SHORT) as i32, (bot.yaw * ANGLE2SHORT) as i32, 0],
                forwardmove: (forward * 127.0) as i8,
                rightmove: (right * 127.0) as i8,
                ..Default::default()
            };
            let backend = super::clip::Backend { world: &clip };
            let before = bot.ps.origin;
            movement_iw4::pmove(
                &mut bot.ps,
                &mut cmd,
                pmove_context(bot.old_buttons, &gun, false, &super::class::Class { perks: [2, 2, 2], ..Default::default() }),
                &backend,
                &FlatMantleAnimLength::default(),
                &CreateAnimsMantleRootDelta,
            );
            bot.old_buttons = cmd.buttons;
            let moved = ((bot.ps.origin[0] - before[0]).powi(2) + (bot.ps.origin[1] - before[1]).powi(2)).sqrt();
            let trying = forward != 0.0 || right != 0.0;
            if trying && moved < 0.5 * msec as f32 * 0.19 * 0.3 {
                bot.stuck += dt;
            } else {
                bot.stuck = 0.0;
            }
            if bot.stuck > 0.4 {
                bot.jump = true;
                bot.strafe = -bot.strafe;
                bot.stuck = 0.0;
            }
            if bot.ps.origin[2] < -20_000.0 {
                bot.alive = false;
                bot.respawn_at = now + 1.0;
            }
        }

        // Shoot: reaction time, short bursts, accuracy falls with range and target speed.
        let mut shot = None;
        if bot.engaged && !protected && bot.seen_since.is_some_and(|t| now - t > 1.1) && now >= bot.next_shot {
            bot.burst += 1;
            bot.next_shot = if bot.burst >= 4 {
                bot.burst = 0;
                now + 1.0 + r1 * 0.8
            } else {
                now + 0.13
            };
            let accuracy = ((0.16 - dist / 15000.0) * (1.0 - (player_speed / 380.0).min(0.6))).clamp(0.03, 0.16);
            let damage = if dist < 1500.0 { 14.0 } else { 10.0 };
            bot.last_shot = now;
            shot = Some((eye, r2 < accuracy, damage, dist, bot.name));
        }
        if let Some((from, hit, damage, dist, name)) = shot {
            let gain = 0.9 / (1.0 + dist / 700.0);
            world.resource_scope(|world, sounds: Mut<Sounds>| sounds.play(&mut world.commands(), Sfx::EnemyShot, gain));
            let miss = if hit { [0.0; 3] } else { [(r1 - 0.5) * 60.0, (r2 - 0.5) * 60.0, (r1 - r2) * 30.0] };
            let target = [player_eye[0] + miss[0], player_eye[1] + miss[1], player_eye[2] - 10.0 + miss[2]];
            spawn_tracer(world, from_iw(from), from_iw(target), now);
            if hit {
                world.resource_scope(|world, sounds: Mut<Sounds>| sounds.play(&mut world.commands(), Sfx::Hurt, 0.6));
                if super::hurt(s, damage, name) {
                    s.bots[i].kills += 1;
                }
            }
        }
        update_body(world, &s.bots[i], now);
    }
}

fn spawn_tracer(world: &mut World, from: Vec3, to: Vec3, now: f32) {
    let Some(a) = world.get_resource::<BotAssets>() else { return };
    let (mesh, material) = (a.box_mesh.clone(), a.band.clone());
    let length = from.distance(to);
    if length < 0.5 {
        return;
    }
    world.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(from.lerp(to, 0.5)).looking_at(to, Vec3::Y).with_scale(Vec3::new(0.01, 0.01, length.min(40.0))),
        weapons::fade_at(now + 0.05),
    ));
}

fn update_body(world: &mut World, bot: &Bot, now: f32) {
    let Some(e) = bot.entity else { return };
    let yaw = bot.yaw.to_radians();
    // IW4 forward (cos y, sin y, 0) is Bevy (cos y, 0, -sin y).
    let forward = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let mut t = Transform::from_translation(from_iw(bot.ps.origin)).looking_to(forward, Vec3::Y);
    if bot.ps.pm_flags & pm_flags::CROUCH != 0 {
        t.scale = Vec3::new(1.0, 0.72, 1.0);
    }
    if !bot.alive {
        // Topple backwards, then vanish until the respawn.
        let k = ((now - bot.died_at) / 0.35).clamp(0.0, 1.0);
        t.rotate_local_x(k * std::f32::consts::FRAC_PI_2);
        if now - bot.died_at > 2.5 {
            if let Some(mut v) = world.get_mut::<Visibility>(e) {
                *v = Visibility::Hidden;
            }
        }
    }
    if let Some(mut current) = world.get_mut::<Transform>(e) {
        *current = t;
    }
}

/// Nearest bot hit by a ray before `max` units: (index, distance, headshot).
pub(crate) fn raycast(s: &Session, from: [f32; 3], dir: [f32; 3], max: f32) -> Option<(usize, f32, bool)> {
    let mut best: Option<(usize, f32, bool)> = None;
    for (i, bot) in s.bots.iter().enumerate().filter(|(_, b)| b.alive) {
        let o = bot.ps.origin;
        let crouched = bot.ps.pm_flags & pm_flags::CROUCH != 0;
        let (body_top, head_z) = if crouched { (40.0, 46.0) } else { (57.0, 64.0) };
        let head = ray_sphere(from, dir, [o[0], o[1], o[2] + head_z], HEAD_RADIUS).map(|t| (t, true));
        let body = ray_cylinder(from, dir, o, RADIUS, body_top).map(|t| (t, false));
        let hit = match (head, body) {
            (Some(h), Some(b)) => Some(if h.0 <= b.0 { h } else { b }),
            (h, b) => h.or(b),
        };
        if let Some((t, is_head)) = hit {
            if t < max && best.is_none_or(|(_, d, _)| t < d) {
                best = Some((i, t, is_head));
            }
        }
    }
    best
}

fn ray_sphere(o: [f32; 3], d: [f32; 3], c: [f32; 3], r: f32) -> Option<f32> {
    let m = [o[0] - c[0], o[1] - c[1], o[2] - c[2]];
    let b = m[0] * d[0] + m[1] * d[1] + m[2] * d[2];
    let cc = m[0] * m[0] + m[1] * m[1] + m[2] * m[2] - r * r;
    let disc = b * b - cc;
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    (t > 0.0).then_some(t)
}

/// Vertical cylinder from `base` up `height`.
fn ray_cylinder(o: [f32; 3], d: [f32; 3], base: [f32; 3], r: f32, height: f32) -> Option<f32> {
    let (mx, my) = (o[0] - base[0], o[1] - base[1]);
    let a = d[0] * d[0] + d[1] * d[1];
    if a < 1e-9 {
        return None;
    }
    let b = mx * d[0] + my * d[1];
    let c = mx * mx + my * my - r * r;
    let disc = b * b - a * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / a;
    if t <= 0.0 {
        return None;
    }
    let z = o[2] + d[2] * t - base[2];
    (0.0..=height).contains(&z).then_some(t)
}

/// The player fired: bots within earshot come looking.
pub(crate) fn hear_shot(s: &mut Session) {
    let (now, p) = (s.now, s.ps.origin);
    for bot in s.bots.iter_mut().filter(|b| b.alive) {
        let d = ((bot.ps.origin[0] - p[0]).powi(2) + (bot.ps.origin[1] - p[1]).powi(2)).sqrt();
        if d < 1600.0 {
            bot.alert_until = bot.alert_until.max(now + 5.0);
        }
    }
}

/// Apply damage; returns true on a kill.
pub(crate) fn damage(s: &mut Session, i: usize, amount: f32, weapon: &str, head: bool) -> bool {
    let now = s.now;
    let bot = &mut s.bots[i];
    if !bot.alive {
        return false;
    }
    bot.health -= amount;
    // Getting shot makes them turn and fight.
    bot.alert_until = now + 8.0;
    if bot.health > 0.0 {
        return false;
    }
    bot.alive = false;
    bot.deaths += 1;
    bot.died_at = now;
    bot.respawn_at = now + 4.0;
    let name = bot.name;
    s.kills += 1;
    s.streak += 1;
    s.best_streak = s.best_streak.max(s.streak);
    let how = if head { " (headshot)" } else { "" };
    s.feed(format!("You [{weapon}] {name}{how}"));
    if s.match_over_until.is_none() {
        let points = if head { 150 } else { 100 };
        s.score += points;
        s.popup(if head { format!("Headshot! +{points}") } else { format!("+{points}") });
    }
    if s.weapons.class.scavenger() {
        s.weapons.scavenge();
    }
    if s.streak == if s.weapons.class.hardline() { 2 } else { 3 } {
        s.uav_ready = true;
        s.feed("UAV ready: press 4".into());
    } else if matches!(s.streak, 5 | 7 | 11 | 25) {
        s.feed(format!("{} kill streak!", s.streak));
    }
    true
}
