//! MW2-style HUD: crosshair, hitmarker, ammo, kill feed, damage and scope.

use super::{Mw2, Session};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

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
    Hint,
}

fn text(part: Part, size: f32, node: Node) -> impl Bundle {
    (part, Text::new(""), TextFont { font_size: size, ..default() }, TextColor(Color::srgba(1.0, 1.0, 1.0, 0.92)), node)
}

fn abs() -> Node {
    Node { position_type: PositionType::Absolute, ..default() }
}

pub(crate) fn spawn(mut commands: Commands) {
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
                        mid.spawn(Node { width: Val::Percent(100.0), height: Val::Vh(92.0), border: UiRect::all(Val::Px(3.0)), ..default() })
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
            root.spawn(text(Part::Hitmarker, 26.0, Node { left: Val::Percent(50.0), top: Val::Percent(50.0), margin: UiRect { left: Val::Px(-8.0), top: Val::Px(-16.0), ..default() }, ..abs() }));
            root.spawn(text(Part::Ammo, 22.0, Node { right: Val::Px(28.0), bottom: Val::Px(64.0), ..abs() }));
            root.spawn(text(Part::Score, 18.0, Node { left: Val::Percent(50.0), top: Val::Px(14.0), margin: UiRect { left: Val::Px(-190.0), ..default() }, width: Val::Px(380.0), ..abs() }));
            root.spawn(text(Part::Feed, 15.0, Node { left: Val::Px(20.0), bottom: Val::Px(120.0), ..abs() }));
            root.spawn(text(Part::Center, 30.0, Node { left: Val::Percent(50.0), top: Val::Percent(38.0), margin: UiRect { left: Val::Px(-150.0), ..default() }, width: Val::Px(300.0), ..abs() }));
            root.spawn(text(Part::Hint, 14.0, Node { left: Val::Px(20.0), bottom: Val::Px(20.0), ..abs() }));
        });
}

pub(crate) fn update(
    mw2: Res<Mw2>,
    session: Option<Res<Session>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut parts: Query<(&Part, &mut Visibility, &mut Node, Option<&mut Text>, Option<&mut BackgroundColor>)>,
) {
    let height = windows.single().map(|w| w.height()).unwrap_or(720.0);
    let Some(s) = session.filter(|_| mw2.active) else {
        for (part, mut vis, ..) in &mut parts {
            if *part == Part::Root {
                *vis = Visibility::Hidden;
            }
        }
        return;
    };
    let w = &s.weapons;
    let gun = w.gun();
    let dead = s.dead_until.is_some();
    let fov = w.fov();
    // Spread cone half-angle to pixels at the current field of view.
    let gap = (w.crosshair_gap().to_radians().tan() / (fov * 0.5).tan() * height * 0.5).clamp(4.0, height * 0.3);
    let show_cross = w.ads < 0.5 && !dead;
    let (ammo, reserve) = w.ammo();
    for (part, mut vis, mut node, text, background) in &mut parts {
        match *part {
            Part::Root => *vis = Visibility::Inherited,
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
                if let Some(mut t) = text {
                    t.0 = if s.now < w.hitmarker_until { "X".into() } else { String::new() };
                }
            }
            Part::Ammo => {
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
                    t.0 = format!("Kills {}   Deaths {}   Streak {} (best {})", s.kills, s.deaths, s.streak, s.best_streak);
                }
            }
            Part::Feed => {
                if let Some(mut t) = text {
                    t.0 = s.feed.iter().map(|(line, _)| line.as_str()).collect::<Vec<_>>().join("\n");
                }
            }
            Part::Center => {
                if let Some(mut t) = text {
                    t.0 = if dead { "KILLED\nrespawning...".into() } else { String::new() };
                }
            }
            Part::Hint => {
                if let Some(mut t) = text {
                    t.0 = "J skate  |  WASD move  Space jump  C crouch  Z prone  Shift sprint  |  1-4 / Q guns  R reload".into();
                }
            }
        }
    }
}
