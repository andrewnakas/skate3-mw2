//! MW2 with a skateboard: the game boots into a Modern Warfare 2 match on
//! the map, and J (or both sticks) drops the player onto a Skate 3 board.
//! The match keeps running while skating: bots hunt the board, and getting
//! killed puts the player back on foot.
//!
//! On foot the player moves with IW4L's own IW4 movement (`movement_iw4::pmove`:
//! walk, sprint, jump, crouch, prone, mantle, slide-move and step-up), traced
//! against the map's skate collision triangles through [`clip`]. Guns use
//! published MW2 numbers; every model is built from primitives and every
//! sound is synthesised, so nothing here comes from a retail game.
//!

pub(crate) mod audio;
mod bots;
pub(crate) mod class;
pub(crate) mod menu;
mod grenades;
pub(crate) mod clip;
mod hud;
mod look;
mod weapons;

use bevy::input::gamepad::{Gamepad, GamepadAxis, GamepadButton};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use movement_iw4::{
    ANGLE2SHORT, AirMoveContext, CmdScaleWalkContext, CreateAnimsMantleRootDelta,
    FlatMantleAnimLength, JumpLaunchContext, MoveBounds, PmoveSingleContext, SprintContext,
    ViewAngleClamp, WalkMoveContext, get_max_sprint_time,
};
use playerstate_iw4::{PlayerState, UserCmd, buttons, other_flags};
use std::sync::Arc;

use audio::{Sfx, Sounds};
use clip::{ClipWorld, dir_from_iw, from_iw, to_iw};

/// MW2's `cg_fov` 65 is horizontal at 4:3; Bevy wants the vertical angle.
pub(crate) fn vertical_fov(horizontal_4x3_deg: f32) -> f32 {
    2.0 * ((horizontal_4x3_deg.to_radians() * 0.5).tan() * 0.75).atan()
}

/// Shared with the camera, physics and input: whether MW2 owns the player.
#[derive(Resource, Default)]
pub(crate) struct Mw2 {
    pub(crate) active: bool,
    /// Eye pose and vertical field of view, written before `camera::present`.
    pub(crate) camera: Option<(Transform, f32)>,
    /// The match has started (the first spawn happened).
    pub(crate) booted: bool,
    boot_frames: u32,
    /// The pause menu asked to drop onto the board.
    pub(crate) board_request: bool,
}

/// UI that belongs to skating (the original scoring HUD, mod overlays):
/// hidden while on foot.
#[derive(Component)]
pub(crate) struct SkateOnly;

/// UI with no place in a match (the bike mod's hint): hidden once it starts.
#[derive(Component)]
pub(crate) struct HideInMatch;

/// A match: first to the score limit, or the most when time runs out.
pub(crate) const MATCH_SECONDS: f32 = 600.0;
pub(crate) const SCORE_LIMIT: u32 = 3000;

#[derive(Default, Clone, Copy)]
pub(crate) struct Controls {
    pub forward: f32,
    pub right: f32,
    /// Degrees this frame: yaw left positive, pitch down positive (IW4).
    pub look: Vec2,
    pub fire: bool,
    pub fire_pressed: bool,
    pub ads: bool,
    pub jump: bool,
    pub crouch: bool,
    pub prone: bool,
    pub sprint: bool,
    pub reload: bool,
    pub next_weapon: bool,
    pub weapon_slot: Option<usize>,
    pub knife: bool,
    pub streak_key: bool,
    pub frag: bool,
    pub smoke: bool,
    pub scores: bool,
}

#[derive(Resource)]
pub(crate) struct Session {
    pub clip: Arc<ClipWorld>,
    pub ps: PlayerState,
    pub old_buttons: u32,
    pub yaw: f32,
    pub pitch: f32,
    ms_carry: f32,
    hidden: Vec<(Entity, Visibility)>,
    pub health: f32,
    pub last_hurt: f32,
    pub dead_until: Option<f32>,
    pub kills: u32,
    pub deaths: u32,
    pub streak: u32,
    pub best_streak: u32,
    pub feed: Vec<(String, f32)>,
    pub hurt_flash: f32,
    /// Bots hold fire until then: entering MW2 and every respawn.
    pub protected_until: f32,
    pub spots: Vec<[f32; 3]>,
    pub rng: u32,
    footstep: f32,
    pub weapons: weapons::Loadout,
    /// The class to spawn with (Create-a-Class changes apply on the next spawn).
    pub class: class::Class,
    pub bots: Vec<bots::Bot>,
    pub controls: Controls,
    pub now: f32,
    pub score: u32,
    /// Seconds left in the match (counts only while the game runs).
    pub match_left: f32,
    pub match_over_until: Option<f32>,
    /// Centre-screen points popups ("+100").
    pub popups: Vec<(String, f32)>,
    pub uav_ready: bool,
    pub uav_until: f32,
    pub grenades: grenades::Grenades,
    /// Pad stance (B): tap toggles crouch, hold goes prone. 0 stand, 1 crouch, 2 prone.
    pad_stance: u8,
    pad_b_down: Option<f32>,
}

impl Session {
    pub fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng as f32 / u32::MAX as f32
    }

    pub fn eye(&self) -> [f32; 3] {
        let o = self.ps.origin;
        [o[0], o[1], o[2] + self.ps.view_height_current]
    }

    /// View direction in IW4 space, including the recoil offset.
    pub fn view_dir(&self) -> [f32; 3] {
        let pitch = (self.pitch + self.weapons.recoil.x).to_radians();
        let yaw = (self.yaw + self.weapons.recoil.y).to_radians();
        [pitch.cos() * yaw.cos(), pitch.cos() * yaw.sin(), -pitch.sin()]
    }

    pub fn popup(&mut self, text: String) {
        self.popups.push((text, self.now));
        if self.popups.len() > 3 {
            self.popups.remove(0);
        }
    }

    pub fn feed(&mut self, line: String) {
        self.feed.push((line, self.now));
        if self.feed.len() > 5 {
            self.feed.remove(0);
        }
    }

    /// A walkable spot `min`..`max` units from `from`, preferring ones out of sight.
    pub fn spawn_spot(&mut self, from: [f32; 3], min: f32, max: f32) -> Option<[f32; 3]> {
        if self.spots.is_empty() {
            return None;
        }
        let mut fallback = None;
        for attempt in 0..48 {
            let i = (self.random() * self.spots.len() as f32) as usize % self.spots.len();
            let p = self.spots[i];
            let d = ((p[0] - from[0]).powi(2) + (p[1] - from[1]).powi(2)).sqrt();
            if d < min || d > max || (p[2] - from[2]).abs() > 400.0 {
                continue;
            }
            let start = [p[0], p[1], p[2] + 48.0];
            let ground = self.clip.sweep(start, [p[0], p[1], p[2] - 64.0], [-15.0, -15.0, 0.0], [15.0, 15.0, 70.0]);
            if ground.startsolid != 0 || ground.fraction >= 1.0 {
                continue;
            }
            let spot = ground.endpos;
            let eye = [spot[0], spot[1], spot[2] + 60.0];
            let seen = self.clip.ray(eye, [from[0], from[1], from[2] + 60.0]).is_none();
            if !seen || attempt > 40 {
                return Some(spot);
            }
            fallback.get_or_insert(spot);
        }
        fallback
    }
}

pub(crate) struct Mw2Plugin;
impl Plugin for Mw2Plugin {
    fn build(&self, app: &mut App) {
        audio::install(app);
        class::install(app);
        menu::install(app);
        app.init_resource::<Mw2>()
            .add_systems(Startup, hud::spawn)
            .add_systems(
                Update,
                (
                    toggle,
                    run.run_if(|m: Res<Mw2>| m.active),
                    ride.run_if(|m: Res<Mw2>| m.booted && !m.active),
                    hud::update,
                )
                    .chain()
                    .before(crate::camera::present),
            );
        weapons::install(app);
        grenades::install(app);
        look::install(app);
        bots::install(app);
    }
}

fn spawn_player_state(origin: [f32; 3], yaw: f32) -> PlayerState {
    // Same defaults IW4L's sim gives a freshly spawned player.
    let mut ps = PlayerState::ZERO;
    ps.origin = origin;
    ps.viewangles = [0.0, yaw, 0.0];
    ps.speed = 190;
    ps.health = 100;
    ps.max_health = 100;
    ps.move_speed_scale_multiplier = 1.0;
    ps.view_height_target = 60;
    ps.view_height_current = 60.0;
    ps.gravity = 800;
    ps.ground_entity_num = 0x7fe;
    ps.other_flags |= other_flags::PLAYER;
    ps.corpse_index = -1;
    ps
}

/// IW4L's `pmove_context` with MW2's default dvars and the held gun's scales.
fn pmove_context(old_buttons: u32, gun: &weapons::Gun, ads: bool, class: &class::Class) -> PmoveSingleContext {
    let light = if class.lightweight() { 1.07 } else { 1.0 };
    let air = AirMoveContext {
        player_spectate_speed_scale: 1.0,
        shellshock_gravity_scale: 1.0,
        shellshock_gravity_bias: 0.0,
    };
    PmoveSingleContext {
        walk: WalkMoveContext {
            cmd_scale: CmdScaleWalkContext {
                player_back_speed_scale: 0.7,
                player_strafe_speed_scale: 0.8,
                player_sprint_speed_scale: 1.5,
                player_last_stand_crawl_speed_scale: 0.15,
                weapon_move_speed_scale: gun.move_scale * light,
                weapon_ads_move_speed_scale: gun.ads_move_scale * light,
                shellshock_affects_movement: false,
            },
            weapon_move_scale: 1.0,
            old_buttons,
            jump: JumpLaunchContext {
                jump_height: 39.0,
                dive: false,
                crouch_jump_scale: 1.0,
                jump_ladder_push_vel: 128.0,
            },
            air,
        },
        air,
        bounds: MoveBounds {
            mins: [-15.0, -15.0, 0.0],
            maxs: [15.0, 15.0, 70.0],
            tracemask: 0x0281_0011,
        },
        view_angles: ViewAngleClamp { pitch_up: 85.0, pitch_down: 85.0, unclamped_pitch_bit: false },
        sprint: SprintContext {
            weapon_max_sprint_time: get_max_sprint_time(gun.sprint_scale, 4.0),
            sprint_forever: class.marathon(),
            min_sprint_time_seconds: 1.0,
            sprint_delay_seconds: 0.0,
            sprint_forward_minimum: 105,
            stand_up_clear: true,
            sprint_recharge_pause_seconds: 0.0,
        },
        ads_intent: movement_iw4::AdsIntentContext {
            ads_allowed: true,
            weapon_def_scope: gun.scope,
            sprint_hold_ads: false,
        },
        ads_frac: movement_iw4::AdsFracContext {
            aim_down_sight: ads,
            ads_in_rate: 1.0 / gun.ads_ms,
            ads_out_rate: 1.0 / (gun.ads_ms * 0.8),
            ..Default::default()
        },
        melee_charge: movement_iw4::MeleeChargeWeaponDelays { melee_delay_ms: 0, melee_charge_delay_ms: 0 },
        player_melee_range: movement_iw4::MELEE_CHARGE_PLAYER_MELEE_RANGE_DEFAULT,
        old_buttons,
        weapon_blocks_prone: false,
        can_hold_breath: false,
    }
}

fn read_controls(world: &mut World, previous_fire: bool) -> (Controls, bool) {
    let keys = world.resource::<ButtonInput<KeyCode>>();
    let mouse = world.resource::<ButtonInput<MouseButton>>();
    let settings = *world.resource::<class::Settings>();
    let motion = world.resource::<AccumulatedMouseMotion>().delta;
    let wheel = world.resource::<bevy::input::mouse::AccumulatedMouseScroll>().delta.y;
    let axis = |neg: KeyCode, pos: KeyCode| f32::from(i8::from(keys.pressed(pos)) - i8::from(keys.pressed(neg)));
    let mut c = Controls {
        forward: axis(KeyCode::KeyS, KeyCode::KeyW),
        right: axis(KeyCode::KeyA, KeyCode::KeyD),
        // MW2: m_yaw 0.022 degrees per count x the sensitivity slider.
        look: Vec2::new(-motion.x, motion.y * if settings.invert { -1.0 } else { 1.0 }) * 0.022 * settings.sensitivity,
        fire: mouse.pressed(MouseButton::Left),
        ads: mouse.pressed(MouseButton::Right),
        jump: keys.pressed(KeyCode::Space),
        crouch: keys.pressed(KeyCode::KeyC),
        prone: keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight),
        sprint: keys.pressed(KeyCode::ShiftLeft),
        reload: keys.just_pressed(KeyCode::KeyR),
        next_weapon: mouse.just_pressed(MouseButton::Middle) || wheel.abs() > 0.0,
        weapon_slot: [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3]
            .iter()
            .position(|k| keys.just_pressed(*k)),
        knife: keys.just_pressed(KeyCode::KeyV),
        streak_key: keys.just_pressed(KeyCode::Digit4),
        frag: keys.just_pressed(KeyCode::KeyF),
        smoke: keys.just_pressed(KeyCode::KeyG),
        scores: keys.pressed(KeyCode::Tab),
        ..default()
    };
    let toggle = keys.just_pressed(KeyCode::KeyJ);
    let dt = world.resource::<Time>().delta_secs();
    let mut pad_toggle = false;
    let mut pad_b: Option<bool> = None;
    let mut pads = world.query::<&Gamepad>();
    for pad in pads.iter(world) {
        let get = |a: GamepadAxis| pad.get(a).unwrap_or(0.0);
        let dead = |v: f32| if v.abs() < 0.15 { 0.0 } else { (v.abs() - 0.15) / 0.85 * v.signum() };
        let (lx, ly) = (dead(get(GamepadAxis::LeftStickX)), dead(get(GamepadAxis::LeftStickY)));
        if lx != 0.0 || ly != 0.0 {
            c.forward = ly;
            c.right = lx;
        }
        let (rx, ry) = (dead(get(GamepadAxis::RightStickX)), dead(get(GamepadAxis::RightStickY)));
        // Console MW2 turns at roughly 180 deg/s at full deflection, with a curve.
        let invert = if settings.invert { -1.0 } else { 1.0 };
        c.look += Vec2::new(-rx * rx.abs(), -ry * ry.abs() * invert) * 180.0 * dt * settings.sensitivity / 5.0;
        let analog = |b: GamepadButton| pad.get(b).unwrap_or(if pad.pressed(b) { 1.0 } else { 0.0 });
        c.fire |= analog(GamepadButton::RightTrigger2) > 0.3;
        c.ads |= analog(GamepadButton::LeftTrigger2) > 0.3;
        c.jump |= pad.pressed(GamepadButton::South);
        // Tactical swaps stance (B) and melee (R3).
        let (stance_button, melee_button) = if settings.pad_layout == 1 {
            (GamepadButton::RightThumb, GamepadButton::East)
        } else {
            (GamepadButton::East, GamepadButton::RightThumb)
        };
        pad_b = pad_b.or(Some(pad.pressed(stance_button)).filter(|b| *b));
        c.knife |= pad.just_pressed(melee_button) && !pad.pressed(GamepadButton::LeftThumb);
        c.frag |= pad.just_pressed(GamepadButton::RightTrigger);
        c.smoke |= pad.just_pressed(GamepadButton::LeftTrigger);
        c.scores |= pad.pressed(GamepadButton::Select);
        c.sprint |= pad.pressed(GamepadButton::LeftThumb);
        c.reload |= pad.just_pressed(GamepadButton::West);
        c.next_weapon |= pad.just_pressed(GamepadButton::North);
        c.streak_key |= pad.just_pressed(GamepadButton::DPadRight);
        let both = pad.pressed(GamepadButton::LeftThumb) && pad.pressed(GamepadButton::RightThumb);
        if both && (pad.just_pressed(GamepadButton::LeftThumb) || pad.just_pressed(GamepadButton::RightThumb)) {
            pad_toggle = true;
        }
    }
    c.fire_pressed = c.fire && !previous_fire;
    // Pad B is MW2's +stance: tap toggles crouch, hold goes prone.
    let now = world.resource::<Time>().elapsed_secs();
    if let Some(mut s) = world.get_resource_mut::<Session>() {
        match (pad_b.is_some(), s.pad_b_down) {
            (true, None) => s.pad_b_down = Some(now),
            (true, Some(at)) if now - at > 0.4 && s.pad_stance != 2 => s.pad_stance = 2,
            (false, Some(at)) => {
                if now - at <= 0.4 {
                    s.pad_stance = if s.pad_stance == 0 { 1 } else { 0 };
                }
                s.pad_b_down = None;
            }
            _ => {}
        }
        c.crouch |= s.pad_stance == 1;
        c.prone |= s.pad_stance == 2;
    }
    (c, toggle || pad_toggle)
}

fn menu_open(world: &World) -> bool {
    world.get_resource::<crate::graphics_menu::Menu>().is_some_and(|m| m.open)
        || world.get_resource::<menu::Menu>().is_some_and(|m| m.open())
}

fn set_cursor(world: &mut World, locked: bool) {
    let mut q = world.query_filtered::<&mut CursorOptions, With<PrimaryWindow>>();
    for mut cursor in q.iter_mut(world) {
        cursor.grab_mode = if locked { CursorGrabMode::Locked } else { CursorGrabMode::None };
        cursor.visible = !locked;
    }
}

fn toggle(world: &mut World) {
    let previous_fire = world.get_resource::<Session>().is_some_and(|s| s.controls.fire);
    let (controls, toggle) = read_controls(world, previous_fire);
    publish_on_foot(world);
    if !world.resource::<Mw2>().booted {
        if world.resource::<menu::Menu>().start {
            boot(world);
        }
        return;
    }
    if std::mem::take(&mut world.resource_mut::<Mw2>().board_request) && world.resource::<Mw2>().active {
        exit(world);
        return;
    }
    let active = world.resource::<Mw2>().active;
    if active {
        if let Some(mut s) = world.get_resource_mut::<Session>() {
            s.controls = controls;
        }
        // The browser drops pointer lock on Escape; a click takes it back.
        if world.resource::<ButtonInput<MouseButton>>().just_pressed(MouseButton::Left) && !menu_open(world) {
            set_cursor(world, true);
        }
    }
    if !toggle || menu_open(world) {
        return;
    }
    if active {
        exit(world);
    } else {
        enter(world, false);
    }
}

/// Tell the page whether the player is on foot (it shows "click to play"
/// while the pointer is not locked).
fn publish_on_foot(world: &World) {
    #[cfg(target_arch = "wasm32")]
    {
        let on_foot = world.resource::<Mw2>().active;
        let _ = js_sys::Reflect::set(&js_sys::global(), &"MW2_ON_FOOT".into(), &on_foot.into());
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = world;
}

/// Start the match on foot once the world, skater and physics are ready.
fn boot(world: &mut World) {
    let ready = world.get_resource::<crate::animation::AnimationStatus>().is_some_and(|a| a.ready)
        && world.get_resource::<crate::physics::GamePhysics>().is_some_and(|p| !p.world_triangles().is_empty());
    let mut mw2 = world.resource_mut::<Mw2>();
    if !ready {
        mw2.boot_frames = 0;
        return;
    }
    mw2.boot_frames += 1;
    // A few frames for the spawn teleport to settle.
    if mw2.boot_frames < 30 {
        return;
    }
    enter(world, true);
    if world.resource::<Mw2>().active {
        world.resource_mut::<Mw2>().booted = true;
    }
}

fn enter(world: &mut World, boot: bool) {
    if world.resource::<crate::modding::vehicles::Vehicles>().occupied()
        || world.resource::<crate::replay::Replay>().active
        || world.resource::<crate::map_transition::MapTransition>().busy()
        || world.resource::<crate::physics::GamePhysics>().board_wiping_out
    {
        return;
    }
    let skater = world.resource::<crate::physics::SkaterRuntime>();
    if skater.player_input.pending_teleport().is_some() {
        return;
    }
    let m = skater.animated_skeleton.roots.animation_to_world;
    let feet = Vec3::new(m[3][0], m[3][1], m[3][2]);
    let heading = m[2][0].atan2(m[2][2]);
    // Bevy forward (sin h, 0, cos h) is IW4 (sin h, -cos h, 0).
    let yaw = (-heading.cos()).atan2(heading.sin()).to_degrees();

    let triangles = world.resource::<crate::physics::GamePhysics>().world_triangles();
    let count = triangles.len();
    let reuse = world.get_resource::<Session>().filter(|s| s.clip.source_len == count).map(|s| s.clip.clone());
    let clip = reuse.unwrap_or_else(|| {
        let tris = triangles.iter().map(|t| {
            t.triangle.vertices.map(|v| Vec3::new(v.x, v.y, v.z))
        });
        let clip = ClipWorld::new(tris, count);
        info!("MW2 mode: {} collision triangles", clip.triangle_count());
        Arc::new(clip)
    });

    // Start a little above the skater and settle onto the ground.
    let mut origin = to_iw(feet);
    origin[2] += 4.0;
    for lift in [0.0, 24.0, 48.0, 96.0] {
        let probe = [origin[0], origin[1], origin[2] + lift];
        let t = clip.sweep(probe, probe, [-15.0, -15.0, 0.0], [15.0, 15.0, 70.0]);
        if t.startsolid == 0 {
            origin = probe;
            break;
        }
    }
    let now = world.resource::<Time>().elapsed_secs();
    let mut hidden = Vec::new();
    let mut roots = world.query_filtered::<(Entity, &mut Visibility), With<crate::world::PlayerRoot>>();
    for (entity, mut vis) in roots.iter_mut(world) {
        hidden.push((entity, *vis));
        *vis = Visibility::Hidden;
    }

    let class = *world.resource::<class::Class>();
    let fresh = !world.contains_resource::<Session>() || world.resource::<Session>().clip.source_len != count;
    if fresh {
        let spots = clip.walkable_spots();
        world.insert_resource(Session {
            clip,
            ps: spawn_player_state(origin, yaw),
            old_buttons: 0,
            yaw,
            pitch: 0.0,
            ms_carry: 0.0,
            hidden,
            health: 100.0,
            last_hurt: -100.0,
            dead_until: None,
            kills: 0,
            deaths: 0,
            streak: 0,
            best_streak: 0,
            feed: Vec::new(),
            hurt_flash: 0.0,
            protected_until: now + 3.0,
            spots,
            rng: 0x2545_F491 ^ (now.to_bits() | 1),
            footstep: 0.0,
            weapons: weapons::Loadout::new(&class),
            class,
            bots: Vec::new(),
            controls: Controls::default(),
            now,
            score: 0,
            match_left: MATCH_SECONDS,
            match_over_until: None,
            popups: Vec::new(),
            uav_ready: false,
            uav_until: 0.0,
            grenades: {
                let mut g = grenades::Grenades::default();
                g.refill();
                g
            },
            pad_stance: 0,
            pad_b_down: None,
        });
    } else {
        let mut s = world.resource_mut::<Session>();
        s.ps = spawn_player_state(origin, yaw);
        s.yaw = yaw;
        s.pitch = 0.0;
        s.hidden = hidden;
        s.now = now;
        s.protected_until = now + 3.0;
    }
    if boot {
        // Deploy at a spawn point, not where the skater happened to start.
        let mut s = world.resource_mut::<Session>();
        let from = s.ps.origin;
        if let Some(spot) = s.spawn_spot(from, 0.0, 1e9) {
            s.ps = spawn_player_state(spot, yaw);
        }
        s.match_left = MATCH_SECONDS;
        s.feed("Kill the bots. J drops you onto a board.".into());
    }
    world.resource_mut::<Mw2>().active = true;
    set_cursor(world, true);
    bots::show(world, true);
    weapons::show(world, true);
    world.resource_scope(|world, sounds: Mut<Sounds>| {
        sounds.play(&mut world.commands(), Sfx::Toggle, 0.6);
    });
}

/// Skating: the match goes on. The player is wherever the skater is; bots
/// hunt the board, and getting killed puts the player back on foot.
fn ride(world: &mut World) {
    if world.resource::<menu::Menu>().open() {
        return;
    }
    let dt = world.resource::<Time>().delta_secs().min(0.1);
    let now = world.resource::<Time>().elapsed_secs();
    let m = world.resource::<crate::physics::SkaterRuntime>().animated_skeleton.roots.animation_to_world;
    let feet = Vec3::new(m[3][0], m[3][1], m[3][2]);
    let killed = world.resource_scope(|world, mut s: Mut<Session>| {
        s.now = now;
        s.controls = Controls::default();
        s.ps.origin = to_iw(feet);
        common_tick(world, &mut s, dt);
        grenades::tick(world, &mut s, dt);
        bots::tick(world, &mut s, dt);
        s.dead_until.is_some()
    });
    if killed {
        enter(world, false);
    }
}

/// Health regen, timers, popups and the match clock, on foot or on the board.
fn common_tick(world: &mut World, s: &mut Session, dt: f32) {
    let now = s.now;
    if s.dead_until.is_none() && now - s.last_hurt > 4.0 {
        s.health = (s.health + 50.0 * dt).min(100.0);
    }
    s.hurt_flash = (s.hurt_flash - dt * 2.0).max(0.0);
    s.feed.retain(|(_, at)| now - at < 6.0);
    s.popups.retain(|(_, at)| now - at < 1.4);
    if s.match_over_until.is_none() {
        s.match_left -= dt;
    }
    match s.match_over_until {
        Some(until) if now >= until => {
            // Next match.
            s.score = 0;
            s.kills = 0;
            s.deaths = 0;
            s.streak = 0;
            s.uav_ready = false;
            s.match_left = MATCH_SECONDS;
            s.match_over_until = None;
            s.protected_until = now + 3.0;
            s.feed("New match".into());
        }
        None if s.score >= SCORE_LIMIT || s.match_left <= 0.0 => {
            let won = s.score >= SCORE_LIMIT;
            s.match_over_until = Some(now + 8.0);
            s.popup(if won { "VICTORY".into() } else { "TIME UP".into() });
            s.feed(format!("Match over: {} points, {} kills, {} deaths", s.score, s.kills, s.deaths));
            play(world, Sfx::Kill, 0.8);
        }
        _ => {}
    }
}

pub(crate) fn play(world: &mut World, sfx: Sfx, gain: f32) {
    world.resource_scope(|world, sounds: Mut<Sounds>| sounds.play(&mut world.commands(), sfx, gain));
}

fn exit(world: &mut World) {
    let (origin, yaw, hidden) = {
        let mut s = world.resource_mut::<Session>();
        (s.ps.origin, s.yaw, std::mem::take(&mut s.hidden))
    };
    for (entity, vis) in hidden {
        if let Some(mut current) = world.get_mut::<Visibility>(entity) {
            *current = vis;
        }
    }
    let feet = from_iw(origin) + Vec3::Y * 0.15;
    let yr = yaw.to_radians();
    // IW4 forward (cos y, sin y, 0) is Bevy (cos y, 0, -sin y).
    let heading = yr.cos().atan2(-yr.sin());
    let (sin, cos) = heading.sin_cos();
    let matrix = [[cos, 0., -sin, 0.], [0., 1., 0., 0.], [sin, 0., cos, 0.], [feet.x, feet.y, feet.z, 0.]];
    {
        let mut skater = world.resource_mut::<crate::physics::SkaterRuntime>();
        if skater.player_input.pending_teleport().is_none() && skater.player_input.request_teleport(matrix).is_ok() {
            skater.teleport_state.request_manual(matrix, false);
        }
    }
    let mut mw2 = world.resource_mut::<Mw2>();
    mw2.active = false;
    mw2.camera = None;
    set_cursor(world, false);
    bots::show(world, false);
    weapons::show(world, false);
    world.resource_scope(|world, sounds: Mut<Sounds>| {
        sounds.play(&mut world.commands(), Sfx::Toggle, 0.6);
    });
}

/// One frame of MW2: look, pmove, guns, bots, camera.
fn run(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs().min(0.1);
    let now = world.resource::<Time>().elapsed_secs();
    if world.resource::<menu::Menu>().open() {
        return;
    }
    let paused = menu_open(world);
    world.resource_scope(|world, mut s: Mut<Session>| {
        s.now = now;
        if paused {
            s.controls = Controls::default();
        }
        let dead = s.dead_until.is_some();
        if let Some(until) = s.dead_until {
            if now >= until {
                respawn(&mut s);
            }
        }
        let c = if dead { Controls::default() } else { s.controls };

        s.weapons.base_fov = world.resource::<class::Settings>().fov;
        // Look: the gun's ADS zoom slows the turn, as on console.
        let zoom = s.weapons.zoom_scale();
        s.yaw += c.look.x * zoom;
        s.pitch = (s.pitch + c.look.y * zoom).clamp(-85.0, 85.0);

        // pmove in whole milliseconds, as the client does.
        s.ms_carry += dt * 1000.0;
        let msec = s.ms_carry.floor() as i32;
        if msec >= 1 && !dead {
            s.ms_carry -= msec as f32;
            let gun = s.weapons.gun().clone();
            let mut b = 0;
            if c.jump {
                b |= buttons::JUMP;
            }
            if c.crouch {
                b |= buttons::CROUCH;
            }
            if c.prone {
                b |= buttons::PRONE;
            }
            if c.sprint && !c.ads && !c.fire {
                b |= buttons::SPRINT;
            }
            if c.ads {
                b |= buttons::ADS;
            }
            if c.fire {
                b |= buttons::ATTACK;
            }
            let mut cmd = UserCmd {
                server_time: s.ps.command_time.wrapping_add(msec),
                buttons: b,
                angles: [(s.pitch * ANGLE2SHORT) as i32, (s.yaw * ANGLE2SHORT) as i32, 0],
                forwardmove: (c.forward * 127.0).round().clamp(-127.0, 127.0) as i8,
                rightmove: (c.right * 127.0).round().clamp(-127.0, 127.0) as i8,
                ..Default::default()
            };
            let context = pmove_context(s.old_buttons, &gun, c.ads, &s.weapons.class);
            let world_clip = s.clip.clone();
            let backend = clip::Backend { world: &world_clip };
            let before = s.ps.origin;
            let mut ps = s.ps;
            movement_iw4::pmove(
                &mut ps,
                &mut cmd,
                context,
                &backend,
                &FlatMantleAnimLength::default(),
                &CreateAnimsMantleRootDelta,
            );
            s.ps = ps;
            s.old_buttons = cmd.buttons;
            // Fell out of the map: put the player back on solid ground.
            if s.ps.origin[2] < s.clip.min_z - 400.0 {
                respawn(&mut s);
            }
            let moved = ((s.ps.origin[0] - before[0]).powi(2) + (s.ps.origin[1] - before[1]).powi(2)).sqrt();
            let on_ground = s.ps.ground_entity_num != 0x7ff;
            if on_ground {
                s.footstep += moved;
            }
            let stride = if s.ps.pm_flags & playerstate_iw4::pm_flags::SPRINTING != 0 { 90.0 } else { 70.0 };
            if s.footstep > stride {
                s.footstep = 0.0;
                if !s.weapons.class.ninja() {
                    world.resource_scope(|world, sounds: Mut<Sounds>| {
                        sounds.play(&mut world.commands(), Sfx::Footstep, 0.35);
                    });
                }
            }
        }

        common_tick(world, &mut s, dt);
        if c.streak_key && s.uav_ready {
            s.uav_ready = false;
            s.uav_until = now + 30.0;
            s.feed("UAV online".into());
            play(world, Sfx::Toggle, 0.8);
        }

        if c.frag {
            grenades::throw(world, &mut s, grenades::Kind::Frag);
        }
        if c.smoke {
            grenades::throw(world, &mut s, grenades::Kind::Smoke);
        }
        weapons::tick(world, &mut s, dt);
        grenades::tick(world, &mut s, dt);
        bots::tick(world, &mut s, dt);

        if (now / 2.0).floor() != ((now - dt) / 2.0).floor() {
            let o = s.ps.origin;
            info!(
                "MW2_STATE pos=[{:.0},{:.0},{:.0}] health={:.0} kills={} deaths={} bots_alive={} gun={} ammo={:?}",
                o[0], o[1], o[2], s.health, s.kills, s.deaths,
                s.bots.iter().filter(|b| b.alive).count(), s.weapons.gun().name, s.weapons.ammo()
            );
        }

        // Camera at the eye, with the gun's ADS field of view.
        let eye = from_iw(s.eye());
        let forward = dir_from_iw(s.view_dir()).normalize_or(Vec3::NEG_Z);
        let pose = Transform::from_translation(eye).looking_to(forward, Vec3::Y);
        let fov = s.weapons.fov();
        world.resource_mut::<Mw2>().camera = Some((pose, fov));
    });
}

fn respawn(s: &mut Session) {
    let from = s.bots.iter().filter(|b| b.alive).map(|b| b.ps.origin).next().unwrap_or(s.ps.origin);
    let spot = s.spawn_spot(from, 800.0, 3000.0).or_else(|| s.spawn_spot(s.ps.origin, 0.0, 1e9));
    if let Some(spot) = spot {
        let yaw = s.yaw;
        s.ps = spawn_player_state(spot, yaw);
    }
    s.health = 100.0;
    s.dead_until = None;
    s.protected_until = s.now + 4.0;
    s.weapons = weapons::Loadout::new(&s.class);
    s.grenades.refill();
    s.pad_stance = 0;
}

/// Take damage from a bot; returns true if this killed the player.
pub(crate) fn hurt(s: &mut Session, amount: f32, by: &str) -> bool {
    if s.dead_until.is_some() || s.now < s.protected_until {
        return false;
    }
    s.health -= amount;
    s.last_hurt = s.now;
    s.hurt_flash = (s.hurt_flash + amount / 60.0).min(1.0);
    if s.health <= 0.0 {
        s.health = 0.0;
        s.deaths += 1;
        s.streak = 0;
        s.dead_until = Some(s.now + 3.0);
        s.feed(format!("{by} killed you"));
        return true;
    }
    false
}
