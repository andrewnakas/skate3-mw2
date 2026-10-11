//! MW2-style front end: the main menu over a slow shot of the map, Create-a-
//! Class, options, map select and the pause menu. Layout and flow follow MW2's
//! menus; the type is Bebas Neue (SIL OFL), nothing is taken from the game.

use super::class::{Class, PERKS, PRIMARIES, SECONDARIES, Settings};
use super::weapons::guns;
use super::{Mw2, Session};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

/// The clean-room Skate 3 parks published with skate3-cleanroom.
const MAPS: [(&str, &str); 9] = [
    ("IndustrialSkatePark", "Industrial"),
    ("MegaPark", "Mega Park"),
    ("DownTownSkatePark", "Downtown"),
    ("SkateSchool", "Skate School"),
    ("StartPark", "Start Park"),
    ("BlackBoxPark", "Black Box"),
    ("MaloofMoneyCup", "Money Cup"),
    ("dmjumpline", "Jump Line"),
    ("KenneyPark", "Kenney Park"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Screen {
    Main,
    Class,
    Options,
    Maps,
    Pause,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Action {
    Play,
    Resume,
    Open(Screen),
    Back,
    Board,
    EndMatch,
    Primary,
    Secondary,
    Perk(usize),
    Sensitivity,
    Fov,
    Invert,
    PadLayout,
    EngineMenu,
    Map(usize),
}

struct Row {
    label: String,
    value: Option<String>,
    hint: String,
    action: Action,
}

#[derive(Resource)]
pub(crate) struct Menu {
    pub screen: Option<Screen>,
    selected: usize,
    stack: Vec<Screen>,
    /// PLAY was chosen: the match may start.
    pub start: bool,
    font: Handle<Font>,
    built: Option<u64>,
    attract: f32,
    /// Options > Graphics opened the skate engine's menu on purpose.
    pub allow_engine: bool,
    /// Escape was used by our menus this frame (the guard must not reuse it).
    esc_used: bool,
}

impl Menu {
    pub fn open(&self) -> bool {
        self.screen.is_some()
    }
}

#[derive(Component)]
struct MenuRoot;

#[derive(Component)]
struct RowButton(usize);

pub(crate) fn install(app: &mut App) {
    app.add_systems(PostUpdate, guard).add_systems(Startup, setup).add_systems(
        PreUpdate,
        input.after(crate::customiser::navigation).before(crate::graphics_menu::MenuInput),
    )
    .add_systems(Update, (attract_camera, draw).chain().before(crate::camera::present));
}

fn setup(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let font = Font::try_from_bytes(include_bytes!("fonts/BebasNeue-Regular.ttf").to_vec())
        .map(|f| fonts.add(f))
        .unwrap_or_default();
    commands.insert_resource(Menu { screen: Some(Screen::Main), selected: 0, stack: Vec::new(), start: false, font, built: None, attract: 0.0, allow_engine: false, esc_used: false });
}

fn gun_line(i: usize) -> String {
    let g = &guns()[i];
    format!(
        "Damage {:.0}-{:.0}   {:.0} RPM   Magazine {}   Reload {:.1}s",
        g.damage.0 * g.pellets as f32,
        g.damage.1 * g.pellets as f32,
        g.rpm,
        g.mag,
        g.reload_s
    )
}

fn rows(screen: Screen, class: &Class, settings: &Settings) -> Vec<Row> {
    let row = |label: &str, value: Option<String>, hint: &str, action| Row { label: label.into(), value, hint: hint.into(), action };
    match screen {
        Screen::Main => vec![
            row("Play", None, "Team up with nobody: you against six bots, first to 3000.", Action::Play),
            row("Create a Class", None, "Primary, secondary and three perks.", Action::Open(Screen::Class)),
            row("Map", None, "Pick the park the match is played on.", Action::Open(Screen::Maps)),
            row("Options", None, "Sensitivity, field of view, controller layout.", Action::Open(Screen::Options)),
        ],
        Screen::Pause => vec![
            row("Resume Game", None, "", Action::Resume),
            row("Create a Class", None, "Changes apply on your next spawn.", Action::Open(Screen::Class)),
            row("Options", None, "", Action::Open(Screen::Options)),
            row("Drop onto a Board", None, "J does this in game. The bots keep hunting you.", Action::Board),
            row("End Game", None, "Back to the main menu.", Action::EndMatch),
        ],
        Screen::Class => {
            let mut v = vec![
                row("Primary", Some(guns()[class.primary].name.into()), &gun_line(class.primary), Action::Primary),
                row("Secondary", Some(guns()[class.secondary].name.into()), &gun_line(class.secondary), Action::Secondary),
            ];
            for tier in 0..3 {
                let (name, desc) = PERKS[tier][class.perks[tier]];
                v.push(row(&format!("Perk {}", tier + 1), Some(name.into()), desc, Action::Perk(tier)));
            }
            v.push(row("Equipment", Some("Frag + Smoke".into()), "F frag, G smoke (pad RB / LB).", Action::Back));
            v.push(row("Back", None, "", Action::Back));
            v
        }
        Screen::Options => vec![
            row("Mouse Sensitivity", Some(format!("{:.1}", settings.sensitivity)), "MW2's slider: 0.022 degrees per count times this.", Action::Sensitivity),
            row("Field of View", Some(format!("{:.0}", settings.fov)), "MW2 cg_fov, 65 to 90.", Action::Fov),
            row("Invert Look", Some(if settings.invert { "Yes" } else { "No" }.into()), "", Action::Invert),
            row("Controller Layout", Some(["Default", "Tactical"][settings.pad_layout as usize % 2].into()), "Tactical: B melee, R3 crouch.", Action::PadLayout),
            row("Graphics", None, "The engine's graphics and park settings.", Action::EngineMenu),
            row("Back", None, "", Action::Back),
        ],
        Screen::Maps => {
            let mut v: Vec<Row> = MAPS
                .iter()
                .enumerate()
                .map(|(i, (_, title))| row(title, None, "Reloads the page on that park.", Action::Map(i)))
                .collect();
            v.push(row("Back", None, "", Action::Back));
            v
        }
    }
}

fn cycle(value: usize, len: usize, step: i32) -> usize {
    ((value as i32 + step).rem_euclid(len as i32)) as usize
}

#[allow(clippy::too_many_arguments)]
fn input(
    mut menu: ResMut<Menu>,
    mut mw2: ResMut<Mw2>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut nav: ResMut<crate::customiser::Navigation>,
    pads: Query<&Gamepad>,
    clicks: Query<(&Interaction, &RowButton), Changed<Interaction>>,
    mut class: ResMut<Class>,
    mut settings: ResMut<Settings>,
    mut session: Option<ResMut<Session>>,
    mut engine_menu: ResMut<crate::graphics_menu::Menu>,
    mut time: ResMut<Time<Virtual>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let pad = |b: GamepadButton| pads.iter().any(|p| p.just_pressed(b));
    let escape = keys.just_pressed(KeyCode::Escape) || nav.pressed & 0x10 != 0;
    // On foot (or in our menus) Escape / Start belong to MW2, not the skate menu.
    if escape && (menu.open() || mw2.active) {
        keys.clear_just_pressed(KeyCode::Escape);
        nav.pressed &= !0x10;
    }
    // Opening the pause menu is the guard's job (it runs after every other menu).
    let Some(screen) = menu.screen else { return };
    if escape {
        menu.esc_used = true;
    }
    if menu.allow_engine && engine_menu.open {
        // The engine's graphics menu is up over Options: it owns the keys.
        return;
    }
    let list = rows(screen, &class, &settings);
    let up = keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW) || pad(GamepadButton::DPadUp);
    let down = keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS) || pad(GamepadButton::DPadDown);
    let left = keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA) || pad(GamepadButton::DPadLeft);
    let right = keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD) || pad(GamepadButton::DPadRight);
    let mut accept = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) || pad(GamepadButton::South);
    let back = escape || pad(GamepadButton::East);
    if up {
        menu.selected = cycle(menu.selected, list.len(), -1);
    }
    if down {
        menu.selected = cycle(menu.selected, list.len(), 1);
    }
    for (interaction, button) in &clicks {
        match interaction {
            Interaction::Hovered => menu.selected = button.0,
            Interaction::Pressed => {
                menu.selected = button.0;
                accept = true;
            }
            Interaction::None => {}
        }
    }
    menu.selected = menu.selected.min(list.len().saturating_sub(1));
    let step = if left { -1 } else if right || accept { 1 } else { 0 };
    let action = list[menu.selected].action;
    // Values change with left / right (and Enter steps them forward).
    if step != 0 {
        match action {
            Action::Primary => {
                let at = PRIMARIES.iter().position(|g| *g == class.primary).unwrap_or(0);
                class.primary = PRIMARIES[cycle(at, PRIMARIES.len(), step)];
            }
            Action::Secondary => {
                let at = SECONDARIES.iter().position(|g| *g == class.secondary).unwrap_or(0);
                class.secondary = SECONDARIES[cycle(at, SECONDARIES.len(), step)];
            }
            Action::Perk(t) => class.perks[t] = cycle(class.perks[t], 3, step),
            Action::Sensitivity => settings.sensitivity = (settings.sensitivity + step as f32 * 0.5).clamp(1.0, 20.0),
            Action::Fov => settings.fov = (settings.fov + step as f32 * 5.0).clamp(65.0, 90.0),
            Action::Invert => settings.invert = !settings.invert,
            Action::PadLayout => settings.pad_layout = (settings.pad_layout + 1) % 2,
            _ => {}
        }
        if let Some(s) = session.as_mut() {
            s.class = *class;
        }
    }
    let close = |menu: &mut Menu, time: &mut Time<Virtual>, cursor: &mut Query<&mut CursorOptions, With<PrimaryWindow>>| {
        menu.screen = None;
        menu.stack.clear();
        time.unpause();
        for mut c in cursor.iter_mut() {
            c.grab_mode = CursorGrabMode::Locked;
            c.visible = false;
        }
    };
    if accept {
        match action {
            Action::Play => {
                menu.start = true;
                close(&mut menu, &mut time, &mut cursor);
            }
            Action::Resume => close(&mut menu, &mut time, &mut cursor),
            Action::Open(next) => {
                menu.stack.push(screen);
                menu.screen = Some(next);
                menu.selected = 0;
            }
            Action::Back => go_back(&mut menu, &mut time, &mut cursor),
            Action::Board => {
                close(&mut menu, &mut time, &mut cursor);
                mw2.board_request = true;
            }
            Action::EndMatch => {
                // Back to the main menu; the page reload restarts cleanly.
                reload(None);
            }
            Action::EngineMenu => {
                menu.allow_engine = true;
                engine_menu.open = true;
            }
            Action::Map(i) => reload(Some(MAPS[i].0)),
            _ => {}
        }
    } else if back && !(screen == Screen::Main) {
        if screen == Screen::Pause {
            close(&mut menu, &mut time, &mut cursor);
        } else {
            go_back(&mut menu, &mut time, &mut cursor);
        }
    }
}

fn go_back(menu: &mut Menu, time: &mut Time<Virtual>, cursor: &mut Query<&mut CursorOptions, With<PrimaryWindow>>) {
    match menu.stack.pop() {
        Some(prev) => {
            menu.screen = Some(prev);
            menu.selected = 0;
        }
        None => {
            menu.screen = None;
            time.unpause();
            for mut c in cursor.iter_mut() {
                c.grab_mode = CursorGrabMode::Locked;
                c.visible = false;
            }
        }
    }
}

/// Map changes reload the page (the engine loads one map at startup).
fn reload(map: Option<&str>) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let global = js_sys::global();
        let f = if map.is_some() { "skateSelectMap" } else { "location" };
        if let Some(name) = map {
            if let Some(select) = js_sys::Reflect::get(&global, &f.into()).ok().and_then(|v| v.dyn_into::<js_sys::Function>().ok()) {
                let _ = select.call1(&global, &name.into());
            }
        } else if let Ok(location) = js_sys::Reflect::get(&global, &"location".into()) {
            if let Some(reload) = js_sys::Reflect::get(&location, &"reload".into()).ok().and_then(|v| v.dyn_into::<js_sys::Function>().ok()) {
                let _ = reload.call0(&location);
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = map;
}

/// Before the match: a slow orbit around the skater, as MW2's menus show the map.
fn attract_camera(
    mut menu: ResMut<Menu>,
    mut mw2: ResMut<Mw2>,
    time: Res<Time<Real>>,
    skater: Option<Res<crate::physics::SkaterRuntime>>,
) {
    if mw2.booted {
        return;
    }
    let Some(skater) = skater else { return };
    menu.attract += time.delta_secs();
    let m = skater.animated_skeleton.roots.animation_to_world;
    let target = Vec3::new(m[3][0], m[3][1] + 1.2, m[3][2]);
    let a = menu.attract * 0.06;
    let eye = target + Vec3::new(a.cos() * 9.0, 3.5, a.sin() * 9.0);
    mw2.camera = Some((Transform::from_translation(eye).looking_at(target, Vec3::Y), super::vertical_fov(65.0)));
}

fn draw(
    mut commands: Commands,
    mut menu: ResMut<Menu>,
    class: Res<Class>,
    settings: Res<Settings>,
    roots: Query<Entity, With<MenuRoot>>,
) {
    let Some(screen) = menu.screen else {
        if menu.built.take().is_some() {
            for e in &roots {
                commands.entity(e).despawn();
            }
        }
        return;
    };
    let list = rows(screen, &class, &settings);
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!("{screen:?}{}{:?}{:?}", menu.selected, *class, *settings).hash(&mut h);
        h.finish()
    };
    if menu.built == Some(key) {
        return;
    }
    menu.built = Some(key);
    for e in &roots {
        commands.entity(e).despawn();
    }
    let font = menu.font.clone();
    let title = match screen {
        Screen::Main => "MW2 x SKATE",
        Screen::Class => "CREATE A CLASS",
        Screen::Options => "OPTIONS",
        Screen::Maps => "MAP",
        Screen::Pause => "PAUSED",
    };
    let selected = menu.selected;
    let hint = list.get(selected).map(|r| r.hint.clone()).unwrap_or_default();
    let tf = |size: f32| TextFont { font: font.clone(), font_size: size, ..default() };
    commands
        .spawn((
            MenuRoot,
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            BackgroundColor(Color::srgba(0.0, 0.02, 0.0, if screen == Screen::Main { 0.15 } else { 0.45 })),
            GlobalZIndex(40),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(42.0),
                    height: Val::Percent(100.0),
                    padding: UiRect { left: Val::Percent(7.0), top: Val::Percent(12.0), ..default() },
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.01, 0.02, 0.01, 0.62)),
            ))
            .with_children(|panel| {
                panel.spawn((Text::new(title), tf(64.0), TextColor(Color::srgb(0.93, 0.95, 0.9))));
                if screen == Screen::Main {
                    panel.spawn((Text::new("CLEAN-ROOM MASHUP  ·  NO GAME FILES"), tf(18.0), TextColor(Color::srgb(0.62, 0.7, 0.55))));
                }
                panel.spawn(Node { height: Val::Px(28.0), ..default() });
                for (i, row) in list.iter().enumerate() {
                    let on = i == selected;
                    panel
                        .spawn((
                            Button,
                            RowButton(i),
                            Node {
                                width: Val::Px(440.0),
                                padding: UiRect::axes(Val::Px(14.0), Val::Px(3.0)),
                                margin: UiRect::bottom(Val::Px(4.0)),
                                justify_content: JustifyContent::SpaceBetween,
                                ..default()
                            },
                            BackgroundColor(if on { Color::srgba(0.55, 0.72, 0.28, 0.45) } else { Color::NONE }),
                        ))
                        .with_children(|b| {
                            let color = if on { Color::srgb(0.95, 1.0, 0.75) } else { Color::srgb(0.78, 0.8, 0.76) };
                            b.spawn((Text::new(row.label.to_uppercase()), tf(34.0), TextColor(color)));
                            if let Some(v) = &row.value {
                                let shown = if on { format!("< {} >", v.to_uppercase()) } else { v.to_uppercase() };
                                b.spawn((Text::new(shown), tf(34.0), TextColor(Color::srgb(0.98, 0.84, 0.38))));
                            }
                        });
                }
                panel.spawn(Node { height: Val::Px(24.0), ..default() });
                panel.spawn((
                    Text::new(hint),
                    TextFont { font_size: 15.0, ..default() },
                    TextColor(Color::srgb(0.72, 0.76, 0.7)),
                    Node { width: Val::Px(440.0), ..default() },
                ));
            });
            root.spawn((
                Text::new("ENTER select   ESC back   ARROWS change   ·   PAD: A select  B back  D-PAD change"),
                TextFont { font_size: 13.0, ..default() },
                TextColor(Color::srgb(0.6, 0.65, 0.6)),
                Node { position_type: PositionType::Absolute, left: Val::Percent(3.0), bottom: Val::Px(54.0), ..default() },
            ));
        });
}

/// Runs after every other menu: during a match the skate engine's menu and
/// the mod panel stay shut (unless Options > Graphics asked for it), and
/// Escape / Start on foot opens MW2's pause menu.
fn guard(
    mut menu: ResMut<Menu>,
    mw2: Res<Mw2>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut engine_menu: ResMut<crate::graphics_menu::Menu>,
    mut mods: ResMut<crate::modding::menu::ModMenu>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let escape = keys.just_pressed(KeyCode::Escape) || pads.iter().any(|p| p.just_pressed(GamepadButton::Start));
    let ours = mw2.booted || menu.open();
    if menu.allow_engine && !engine_menu.open {
        menu.allow_engine = false;
    }
    if ours && !menu.allow_engine {
        engine_menu.open = false;
        mods.open = false;
    }
    if escape && !std::mem::take(&mut menu.esc_used) && menu.screen.is_none() && mw2.active {
        menu.screen = Some(Screen::Pause);
        menu.selected = 0;
        for mut c in &mut cursor {
            c.grab_mode = CursorGrabMode::None;
            c.visible = true;
        }
    }
    menu.esc_used = false;
}
