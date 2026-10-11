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
    app.add_systems(Update, apply.after(crate::camera::present));
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
