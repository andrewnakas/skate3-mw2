//! The MW2 look on the gameplay camera, once the match starts: bloom on
//! highlights, muzzle flashes and explosions; SMAA instead of MSAA; and MW2's
//! aim-down-sights depth of field, which focuses past the gun onto whatever
//! the sights are on.

use super::{Mw2, Session, clip::UNIT};
use bevy::anti_alias::smaa::{Smaa, SmaaPreset};
use bevy::post_process::bloom::Bloom;
use bevy::post_process::dof::{DepthOfField, DepthOfFieldMode};
use bevy::prelude::*;

pub(crate) fn install(app: &mut App) {
    app.add_systems(Startup, spawn_sky)
        .add_systems(Update, (apply, follow_sky).after(crate::camera::present));
}

/// A hazy MW2-style sky dome behind everything (the parks' clear colour is a
/// flat blue). It sits at 90% of the camera's far plane, so the parks' own
/// backdrops still draw in front of it.
#[derive(Component)]
struct SkyDome;

fn spawn_sky(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mut mesh = Sphere::new(1.0).mesh().uv(48, 24);
    let colors: Vec<[f32; 4]> = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) => p
            .iter()
            .map(|v| {
                // Warm dusty horizon, pale haze, then a muted blue zenith.
                let h = v[1].max(0.0);
                let horizon = Vec3::new(0.86, 0.80, 0.70);
                let haze = Vec3::new(0.70, 0.74, 0.76);
                let zenith = Vec3::new(0.36, 0.49, 0.66);
                let c = if h < 0.15 { horizon.lerp(haze, h / 0.15) } else { haze.lerp(zenith, ((h - 0.15) / 0.85).powf(0.7)) };
                let c = if v[1] < 0.0 { horizon * 0.8 } else { c };
                [c.x * c.x, c.y * c.y, c.z * c.z, 1.0]
            })
            .collect(),
        _ => Vec::new(),
    };
    if !colors.is_empty() {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    }
    commands.spawn((
        SkyDome,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            cull_mode: None,
            double_sided: true,
            fog_enabled: false,
            ..default()
        })),
        Transform::from_scale(Vec3::splat(400.0)),
        bevy::light::NotShadowCaster,
        bevy::light::NotShadowReceiver,
    ));
}

fn follow_sky(
    cameras: Query<(&GlobalTransform, &Projection), With<crate::camera::GameplayCamera>>,
    mut sky: Query<&mut Transform, With<SkyDome>>,
) {
    let Ok((camera, projection)) = cameras.single() else { return };
    let far = match projection {
        Projection::Perspective(p) => p.far,
        _ => 1000.0,
    };
    for mut t in &mut sky {
        t.translation = camera.translation();
        t.scale = Vec3::splat(far * 0.9);
    }
}

/// f-stop that makes the depth of field invisible when not aiming.
const OPEN: f32 = 1000.0;

#[allow(clippy::type_complexity)]
fn apply(
    mut commands: Commands,
    mw2: Res<Mw2>,
    session: Option<Res<Session>>,
    mut done: Local<bool>,
    mut cameras: Query<(Entity, &mut Msaa, Option<&mut DepthOfField>), With<crate::camera::GameplayCamera>>,
) {
    if !mw2.booted {
        return;
    }
    for (entity, mut msaa, dof) in &mut cameras {
        if !*done {
            *msaa = Msaa::Off;
            commands.entity(entity).insert((
                Bloom { intensity: 0.12, low_frequency_boost: 0.6, ..Bloom::NATURAL },
                Smaa { preset: SmaaPreset::High },
                DepthOfField {
                    mode: DepthOfFieldMode::Gaussian,
                    focal_distance: 10.0,
                    aperture_f_stops: OPEN,
                    max_circle_of_confusion_diameter: 20.0,
                    ..default()
                },
            ));
            continue;
        }
        let Some(mut dof) = dof else { continue };
        let Some(s) = session.as_ref() else { continue };
        let ads = if mw2.active { s.weapons.ads } else { 0.0 };
        if ads < 0.05 || s.weapons.scoped() {
            if dof.aperture_f_stops != OPEN {
                dof.aperture_f_stops = OPEN;
            }
            continue;
        }
        // Focus on what the sights are on; the gun up close goes soft.
        let eye = s.eye();
        let dir = s.view_dir();
        let reach = 8000.0;
        let end = [eye[0] + dir[0] * reach, eye[1] + dir[1] * reach, eye[2] + dir[2] * reach];
        let hit = s.clip.ray(eye, end).map_or(reach, |(t, _)| t * reach);
        dof.focal_distance = (hit * UNIT).clamp(1.5, 150.0);
        dof.aperture_f_stops = 1.4 / ads.max(0.05);
    }
    if !cameras.is_empty() {
        *done = true;
    }
}
