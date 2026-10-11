//! MW2-style HUD: minimap, match clock and score, crosshair, hitmarker, ammo,
//! kill feed, points popups, damage and scope. On the board only the match
//! parts stay; the skate HUD (`SkateOnly`) is shown instead of the gun parts.

use super::{HideInMatch, MATCH_SECONDS, Mw2, SCORE_LIMIT, Session, SkateOnly};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

const MINIMAP: f32 = 170.0;
/// IW4 units from the centre to the minimap edge.
const MINIMAP_RANGE: f32 = 2200.0;
const DOTS: usize = 6;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Part {
    Root,
    Cross(u8),
    Hitmarker,
    Scope,
    Vignette,
    Ammo,
    Score,
    Feed,
    Center,
    Popup,
    Hint,
    Minimap,
    Dot(u8),
    Scores,
}

impl Part {
    /// Parts drawn only on foot (the gun's).
    fn on_foot_only(self) -> bool {
        matches!(self, Part::Cross(_) | Part::Hitmarker | Part::Scope | Part::Ammo)
    }
}

fn text(part: Part, size: f32, color: Color, node: Node) -> impl Bundle {
    (part, Text::new(""), TextFont { font_size: size, ..default() }, TextColor(color), node)
}

fn abs() -> Node {
    Node { position_type: PositionType::Absolute, ..default() }
}

pub(crate) fn spawn(mut commands: Commands) {
    let white = Color::srgba(1.0, 1.0, 1.0, 0.92);
    commands
        .spawn((
            Part::Root,
            Node { width: Val::Percent(100.0), height: Val::Percent(100.0), position_type: PositionType::Absolute, ..default() },
            GlobalZIndex(20),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn((
                Part::Vignette,
                Node { width: Val::Percent(100.0), height: Val::Percent(100.0), position_type: PositionType::Absolute, ..default() },
                BackgroundColor(Color::NONE),
            ));
            // Scope: black sides around a square, with fine cross lines.
            root.spawn((
                Part::Scope,
                Node { width: Val::Percent(100.0), height: Val::Percent(100.0), position_type: PositionType::Absolute, flex_direction: FlexDirection::Row, ..default() },
                Visibility::Hidden,
            ))
            .with_children(|scope| {
                let black = BackgroundColor(Color::BLACK);
                scope.spawn((Node { flex_grow: 1.0, height: Val::Percent(100.0), ..default() }, black));
                scope
                    .spawn(Node { width: Val::Vh(100.0), height: Val::Percent(100.0), flex_direction: FlexDirection::Column, ..default() })
                    .with_children(|mid| {
                        mid.spawn((Node { width: Val::Percent(100.0), height: Val::Vh(4.0), ..default() }, black));
                        mid.spawn(Node { width: Val::Percent(100.0), height: Val::Vh(92.0), ..default() })
                            .with_children(|glass| {
                                let line = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.9));
                                glass.spawn((Node { position_type: PositionType::Absolute, top: Val::Percent(50.0), width: Val::Percent(100.0), height: Val::Px(2.0), ..default() }, line));
                                glass.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), width: Val::Px(2.0), height: Val::Percent(100.0), ..default() }, line));
                            });
                        mid.spawn((Node { width: Val::Percent(100.0), height: Val::Vh(4.0), ..default() }, black));
                    });
                scope.spawn((Node { flex_grow: 1.0, height: Val::Percent(100.0), ..default() }, black));
            });
            for i in 0..4 {
                let (w, h) = if i < 2 { (10.0, 2.0) } else { (2.0, 10.0) };
                root.spawn((
                    Part::Cross(i),
                    Node { position_type: PositionType::Absolute, width: Val::Px(w), height: Val::Px(h), ..default() },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
                ));
            }
            // Minimap: rotates with the view; the player is the marker in the middle.
            root.spawn((
                Part::Minimap,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(16.0),
                    top: Val::Px(16.0),
                    width: Val::Px(MINIMAP),
                    height: Val::Px(MINIMAP),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.09, 0.06, 0.6)),
            ))
            .with_children(|map| {
                let ring = BackgroundColor(Color::srgba(0.6, 0.8, 0.6, 0.18));
                map.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), width: Val::Px(1.0), height: Val::Percent(100.0), ..default() }, ring));
                map.spawn((Node { position_type: PositionType::Absolute, top: Val::Percent(50.0), width: Val::Percent(100.0), height: Val::Px(1.0), ..default() }, ring));
                map.spawn((
                    Node { position_type: PositionType::Absolute, left: Val::Px(MINIMAP * 0.5 - 4.0), top: Val::Px(MINIMAP * 0.5 - 6.0), width: Val::Px(8.0), height: Val::Px(10.0), ..default() },
                    BackgroundColor(Color::srgb(1.0, 0.92, 0.3)),
                ));
                for i in 0..DOTS as u8 {
                    map.spawn((
                        Part::Dot(i),
                        Node { position_type: PositionType::Absolute, width: Val::Px(8.0), height: Val::Px(8.0), ..default() },
                        BackgroundColor(Color::srgb(0.95, 0.12, 0.08)),
                        Visibility::Hidden,
                    ));
                }
            });
            root.spawn(text(Part::Hitmarker, 26.0, white, Node { left: Val::Percent(50.0), top: Val::Percent(50.0), margin: UiRect { left: Val::Px(-8.0), top: Val::Px(-16.0), ..default() }, ..abs() }));
            root.spawn(text(Part::Popup, 22.0, Color::srgb(1.0, 0.85, 0.3), Node { left: Val::Percent(50.0), top: Val::Percent(56.0), margin: UiRect { left: Val::Px(-150.0), ..default() }, width: Val::Px(300.0), justify_content: JustifyContent::Center, ..abs() }));
            root.spawn(text(Part::Ammo, 22.0, white, Node { right: Val::Px(28.0), bottom: Val::Px(64.0), ..abs() }));
            root.spawn(text(Part::Score, 18.0, white, Node { left: Val::Percent(50.0), top: Val::Px(14.0), margin: UiRect { left: Val::Px(-200.0), ..default() }, width: Val::Px(400.0), justify_content: JustifyContent::Center, ..abs() }));
            root.spawn(text(Part::Feed, 15.0, white, Node { left: Val::Px(20.0), bottom: Val::Px(140.0), ..abs() }));
            root.spawn(text(Part::Center, 30.0, white, Node { left: Val::Percent(50.0), top: Val::Percent(38.0), margin: UiRect { left: Val::Px(-150.0), ..default() }, width: Val::Px(300.0), ..abs() }));
            root.spawn((
                text(Part::Scores, 17.0, white, Node {
                    left: Val::Percent(50.0),
                    top: Val::Percent(22.0),
                    margin: UiRect { left: Val::Px(-210.0), ..default() },
                    width: Val::Px(420.0),
                    padding: UiRect::all(Val::Px(14.0)),
                    ..abs()
                }),
                BackgroundColor(Color::srgba(0.02, 0.03, 0.03, 0.82)),
                Visibility::Hidden,
            ));
            root.spawn(text(Part::Hint, 13.0, white, Node { left: Val::Px(20.0), bottom: Val::Px(46.0), ..abs() }));
        });
}

pub(crate) fn update(
    mw2: Res<Mw2>,
    session: Option<Res<Session>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut parts: Query<(&Part, &mut Visibility, &mut Node, Option<&mut Text>, Option<&mut BackgroundColor>)>,
    mut skate_only: Query<&mut Visibility, (With<SkateOnly>, Without<Part>, Without<HideInMatch>)>,
    mut in_match: Query<&mut Visibility, (With<HideInMatch>, Without<Part>, Without<SkateOnly>)>,
    menu: Res<super::menu::Menu>,
) {
    // The skate HUD shows only on the board, never under our menus.
    let skating = mw2.booted && !mw2.active && !menu.open();
    for mut vis in &mut skate_only {
        *vis = if skating { Visibility::Inherited } else { Visibility::Hidden };
    }
    // The bike mod's hint has no place in this game.
    for mut vis in &mut in_match {
        *vis = Visibility::Hidden;
    }
    let height = windows.single().map(|w| w.height()).unwrap_or(720.0);
    let Some(s) = session.filter(|_| mw2.booted) else {
        for (part, mut vis, ..) in &mut parts {
            if *part == Part::Root {
                *vis = Visibility::Hidden;
            }
        }
        return;
    };
    let on_foot = mw2.active;
    let w = &s.weapons;
    let gun = w.gun();
    let dead = s.dead_until.is_some();
    let fov = w.fov();
    // Spread cone half-angle to pixels at the current field of view.
    let gap = (w.crosshair_gap().to_radians().tan() / (fov * 0.5).tan() * height * 0.5).clamp(4.0, height * 0.3);
    let show_cross = w.ads < 0.5 && !dead;
    let (ammo, reserve) = w.ammo();
    let uav = s.now < s.uav_until;
    // Minimap: rotate so the view direction points up. IW4 is Z up, so the
    // right of forward (cos y, sin y) is (sin y, -cos y).
    let yaw = s.yaw.to_radians();
    let (fwd, right) = ([yaw.cos(), yaw.sin()], [yaw.sin(), -yaw.cos()]);
    let dots: Vec<Option<(f32, f32)>> = s
        .bots
        .iter()
        .take(DOTS)
        .map(|b| {
            // MW2: an enemy shows when firing, or always under a UAV.
            if !(b.alive && (uav || s.now - b.last_shot < 2.0)) {
                return None;
            }
            let d = [b.ps.origin[0] - s.ps.origin[0], b.ps.origin[1] - s.ps.origin[1]];
            let scale = MINIMAP * 0.5 / MINIMAP_RANGE;
            let x = (d[0] * right[0] + d[1] * right[1]) * scale;
            let y = -(d[0] * fwd[0] + d[1] * fwd[1]) * scale;
            let edge = (MINIMAP * 0.5 - 6.0) / (x * x + y * y).sqrt().max(1.0);
            Some(if edge < 1.0 { (x * edge, y * edge) } else { (x, y) })
        })
        .collect();
    let left = s.match_left.clamp(0.0, MATCH_SECONDS);
    for (part, mut vis, mut node, text, background) in &mut parts {
        if part.on_foot_only() && !on_foot {
            *vis = Visibility::Hidden;
            continue;
        }
        match *part {
            Part::Root | Part::Minimap => *vis = Visibility::Inherited,
            Part::Dot(i) => match dots.get(i as usize).copied().flatten() {
                Some((x, y)) => {
                    *vis = Visibility::Inherited;
                    node.left = Val::Px(MINIMAP * 0.5 + x - 4.0);
                    node.top = Val::Px(MINIMAP * 0.5 + y - 4.0);
                }
                None => *vis = Visibility::Hidden,
            },
            Part::Cross(i) => {
                *vis = if show_cross { Visibility::Inherited } else { Visibility::Hidden };
                let (x, y) = match i {
                    0 => (-gap - 10.0, -1.0),
                    1 => (gap, -1.0),
                    2 => (-1.0, -gap - 10.0),
                    _ => (-1.0, gap),
                };
                node.left = Val::Percent(50.0);
                node.top = Val::Percent(50.0);
                node.margin = UiRect { left: Val::Px(x), top: Val::Px(y), ..default() };
            }
            Part::Scope => *vis = if w.scoped() && !dead { Visibility::Inherited } else { Visibility::Hidden },
            Part::Vignette => {
                if let Some(mut bg) = background {
                    let a = (s.hurt_flash * 0.35 + (1.0 - s.health / 100.0) * 0.35).clamp(0.0, 0.7);
                    bg.0 = Color::srgba(0.6, 0.0, 0.0, if dead { 0.55 } else { a });
                }
            }
            Part::Hitmarker => {
                *vis = Visibility::Inherited;
                if let Some(mut t) = text {
                    t.0 = if s.now < w.hitmarker_until { "X".into() } else { String::new() };
                }
            }
            Part::Ammo => {
                *vis = Visibility::Inherited;
                if let Some(mut t) = text {
                    let state = if w.reloading() {
                        "  RELOADING"
                    } else if ammo == 0 {
                        "  PRESS R"
                    } else {
                        ""
                    };
                    t.0 = format!("{}{state}\n{ammo} / {reserve}", gun.name);
                }
            }
            Part::Score => {
                if let Some(mut t) = text {
                    let m = (left / 60.0).floor() as u32;
                    let sec = (left % 60.0).floor() as u32;
                    t.0 = format!(
                        "{m}:{sec:02}    {} / {SCORE_LIMIT}    K {}  D {}  Streak {}",
                        s.score, s.kills, s.deaths, s.streak
                    );
                }
            }
            Part::Feed => {
                if let Some(mut t) = text {
                    t.0 = s.feed.iter().map(|(line, _)| line.as_str()).collect::<Vec<_>>().join("\n");
                }
            }
            Part::Popup => {
                if let Some(mut t) = text {
                    t.0 = s.popups.iter().map(|(line, _)| line.as_str()).collect::<Vec<_>>().join("\n");
                }
            }
            Part::Center => {
                if let Some(mut t) = text {
                    t.0 = if dead { "KILLED\nrespawning...".into() } else { String::new() };
                }
            }
            Part::Scores => {
                let show = s.controls.scores || s.match_over_until.is_some();
                *vis = if show { Visibility::Inherited } else { Visibility::Hidden };
                if let Some(mut t) = text {
                    let mut rows = vec![("You".to_string(), s.kills, s.deaths, s.score)];
                    rows.extend(s.bots.iter().map(|b| (b.name.to_string(), b.kills, b.deaths, b.kills * 100)));
                    rows.sort_by(|a, b| b.3.cmp(&a.3));
                    let mut out = format!("{:<14}{:>8}{:>7}{:>7}\n", "", "Score", "Kills", "Deaths");
                    for (name, k, d, sc) in rows {
                        out += &format!("{name:<14}{sc:>8}{k:>7}{d:>7}\n");
                    }
                    t.0 = out;
                }
            }
            Part::Hint => {
                if let Some(mut t) = text {
                    let streak = if s.uav_ready { "  |  4: UAV ready" } else { "" };
                    t.0 = if on_foot {
                        format!("J board | wheel/1-3 guns | R reload | V knife | F frag G smoke | Tab scores{streak}")
                    } else {
                        format!("J back on foot | the bots are still hunting you{streak}")
                    };
                }
            }
        }
    }
}
