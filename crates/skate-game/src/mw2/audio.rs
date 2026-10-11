//! Every MW2-mode sound is synthesised here at startup: no recorded samples.

use bevy::audio::{AddAudioSource, Decodable, Source, Volume};
use bevy::prelude::*;
use std::sync::Arc;
use std::time::Duration;

const RATE: u32 = 44_100;

/// A mono clip rendered once in memory.
#[derive(Asset, TypePath, Clone)]
pub(crate) struct SynthClip {
    samples: Arc<[f32]>,
}

pub(crate) struct SynthPlayback {
    samples: Arc<[f32]>,
    at: usize,
}

impl Iterator for SynthPlayback {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let v = self.samples.get(self.at).copied();
        self.at += 1;
        v
    }
}

impl Source for SynthPlayback {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        RATE
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f32(self.samples.len() as f32 / RATE as f32))
    }
}

impl Decodable for SynthClip {
    type DecoderItem = f32;
    type Decoder = SynthPlayback;
    fn decoder(&self) -> SynthPlayback {
        SynthPlayback { samples: self.samples.clone(), at: 0 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Sfx {
    Rifle,
    Smg,
    Sniper,
    Shotgun,
    EnemyShot,
    HitMarker,
    Kill,
    Reload,
    Empty,
    Footstep,
    Hurt,
    Toggle,
    Knife,
}

#[derive(Resource)]
pub(crate) struct Sounds(Vec<(Sfx, Handle<SynthClip>)>);

impl Sounds {
    pub(crate) fn play(&self, commands: &mut Commands, sfx: Sfx, gain: f32) {
        if gain <= 0.001 {
            return;
        }
        if let Some((_, handle)) = self.0.iter().find(|(s, _)| *s == sfx) {
            commands.spawn((
                AudioPlayer(handle.clone()),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(gain)),
            ));
        }
    }
}

pub(crate) fn install(app: &mut App) {
    app.add_audio_source::<SynthClip>().add_systems(Startup, build);
}

fn build(mut commands: Commands, mut clips: ResMut<Assets<SynthClip>>) {
    let mut add = |sfx, samples: Vec<f32>| (sfx, clips.add(SynthClip { samples: samples.into() }));
    let sounds = vec![
        add(Sfx::Rifle, gunshot(0x1234, 0.16, 95.0, 0.85, 2200.0)),
        add(Sfx::Smg, gunshot(0x2345, 0.12, 120.0, 0.7, 2800.0)),
        add(Sfx::Sniper, gunshot(0x3456, 0.55, 60.0, 1.0, 1500.0)),
        add(Sfx::Shotgun, gunshot(0x4567, 0.32, 70.0, 1.0, 1700.0)),
        add(Sfx::EnemyShot, gunshot(0x5678, 0.14, 110.0, 0.45, 1300.0)),
        add(Sfx::HitMarker, tick(2600.0, 0.035, 0.5)),
        add(Sfx::Kill, kill_chime()),
        add(Sfx::Reload, reload()),
        add(Sfx::Empty, tick(900.0, 0.03, 0.4)),
        add(Sfx::Footstep, footstep()),
        add(Sfx::Hurt, hurt()),
        add(Sfx::Toggle, tick(1400.0, 0.06, 0.35)),
        add(Sfx::Knife, whoosh()),
    ];
    commands.insert_resource(Sounds(sounds));
}

struct Noise(u32);
impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn samples(seconds: f32) -> usize {
    (seconds * RATE as f32) as usize
}

/// Crack (filtered noise) plus a body thump plus a decaying tail.
fn gunshot(seed: u32, length: f32, thump_hz: f32, gain: f32, cutoff: f32) -> Vec<f32> {
    let mut noise = Noise(seed | 1);
    let n = samples(length);
    let mut out = Vec::with_capacity(n);
    let alpha = 1.0 - (-2.0 * std::f32::consts::PI * cutoff / RATE as f32).exp();
    let mut lp = 0.0;
    let mut tail = 0.0;
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let x = noise.next();
        lp += alpha * (x - lp);
        let crack = (x - lp) * (-t * 90.0).exp();
        tail += 0.02 * (x - tail);
        let body = (2.0 * std::f32::consts::PI * thump_hz * t * (1.0 - t * 2.0).max(0.4)).sin()
            * (-t * 22.0).exp();
        let rumble = tail * 6.0 * (-t * 9.0 / length.max(0.1)).exp();
        let v = (crack * 0.9 + body * 0.8 + rumble * 0.6) * gain;
        out.push((v * 1.4).tanh() * 0.8);
    }
    out
}

fn tick(hz: f32, length: f32, gain: f32) -> Vec<f32> {
    (0..samples(length))
        .map(|i| {
            let t = i as f32 / RATE as f32;
            (2.0 * std::f32::consts::PI * hz * t).sin() * (-t * 120.0).exp() * gain
        })
        .collect()
}

fn kill_chime() -> Vec<f32> {
    let mut out = tick(1800.0, 0.05, 0.45);
    out.extend(tick(2400.0, 0.08, 0.4));
    out
}

fn reload() -> Vec<f32> {
    let mut noise = Noise(0x9e37);
    let mut out = vec![0.0; samples(0.5)];
    for (start, hz) in [(0.0, 700.0), (0.22, 1100.0), (0.36, 500.0)] {
        let s = samples(start);
        for i in 0..samples(0.07) {
            let t = i as f32 / RATE as f32;
            let v = ((2.0 * std::f32::consts::PI * hz * t).sin() * 0.5 + noise.next() * 0.5)
                * (-t * 70.0).exp()
                * 0.45;
            if let Some(o) = out.get_mut(s + i) {
                *o += v;
            }
        }
    }
    out
}

fn footstep() -> Vec<f32> {
    let mut noise = Noise(0x51ed);
    let mut lp = 0.0;
    (0..samples(0.09))
        .map(|i| {
            let t = i as f32 / RATE as f32;
            lp += 0.08 * (noise.next() - lp);
            lp * 3.0 * (-t * 45.0).exp() * 0.5
        })
        .collect()
}

fn hurt() -> Vec<f32> {
    let mut noise = Noise(0x7f4a);
    let mut lp = 0.0;
    (0..samples(0.25))
        .map(|i| {
            let t = i as f32 / RATE as f32;
            lp += 0.05 * (noise.next() - lp);
            (lp * 4.0 + (2.0 * std::f32::consts::PI * 80.0 * t).sin() * 0.5) * (-t * 14.0).exp() * 0.5
        })
        .collect()
}

fn whoosh() -> Vec<f32> {
    let mut noise = Noise(0x2b1d);
    let mut lp = 0.0;
    let n = samples(0.22);
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32;
            lp += (0.05 + 0.25 * t) * (noise.next() - lp);
            lp * (t * std::f32::consts::PI).sin() * 0.9
        })
        .collect()
}
