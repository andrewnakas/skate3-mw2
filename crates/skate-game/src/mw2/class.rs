//! Create-a-Class and options: the player's class (primary, secondary, three
//! perks) and settings, kept in the browser's localStorage between visits.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Guns by index into `weapons::guns()`.
pub(crate) const PRIMARIES: [usize; 4] = [0, 1, 2, 3];
pub(crate) const SECONDARIES: [usize; 2] = [4, 5];

/// MW2's perk tiers, as far as they mean something against bots.
pub(crate) const PERKS: [[(&str, &str); 3]; 3] = [
    [
        ("Marathon", "Unlimited sprint."),
        ("Sleight of Hand", "Faster reloading."),
        ("Scavenger", "Resupply ammo from fallen enemies."),
    ],
    [
        ("Stopping Power", "Increased bullet damage."),
        ("Lightweight", "Move faster."),
        ("Hardline", "Killstreaks require 1 less kill."),
    ],
    [
        ("Commando", "Increased melee distance."),
        ("Steady Aim", "Increased hip fire accuracy."),
        ("Ninja", "Silent footsteps: enemies can't hear you coming."),
    ],
];

#[derive(Resource, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Class {
    pub primary: usize,
    pub secondary: usize,
    pub perks: [usize; 3],
}

impl Default for Class {
    fn default() -> Self {
        // MW2's first default class, roughly: assault rifle, pistol, Marathon / Stopping Power / Commando.
        Self { primary: 0, secondary: 4, perks: [0, 0, 0] }
    }
}

impl Class {
    pub fn has(&self, tier: usize, perk: usize) -> bool {
        self.perks[tier] == perk
    }
    pub fn marathon(&self) -> bool {
        self.has(0, 0)
    }
    pub fn sleight_of_hand(&self) -> bool {
        self.has(0, 1)
    }
    pub fn scavenger(&self) -> bool {
        self.has(0, 2)
    }
    pub fn stopping_power(&self) -> bool {
        self.has(1, 0)
    }
    pub fn lightweight(&self) -> bool {
        self.has(1, 1)
    }
    pub fn hardline(&self) -> bool {
        self.has(1, 2)
    }
    pub fn commando(&self) -> bool {
        self.has(2, 0)
    }
    pub fn steady_aim(&self) -> bool {
        self.has(2, 1)
    }
    pub fn ninja(&self) -> bool {
        self.has(2, 2)
    }
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Settings {
    /// MW2's sensitivity slider: degrees per mouse count = 0.022 x this.
    pub sensitivity: f32,
    /// MW2 `cg_fov` (horizontal at 4:3).
    pub fov: f32,
    pub invert: bool,
    /// Pad layout: 0 Default, 1 Tactical (B melee, R3 stance).
    pub pad_layout: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self { sensitivity: 5.0, fov: 65.0, invert: false, pad_layout: 0 }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    class: Option<Class>,
    settings: Option<Settings>,
}

const KEY: &str = "mw2skate.v1";

pub(crate) fn install(app: &mut App) {
    let saved: Saved = load().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    app.insert_resource(saved.class.unwrap_or_default())
        .insert_resource(saved.settings.unwrap_or_default())
        .add_systems(Update, save.run_if(|c: Res<Class>, s: Res<Settings>| c.is_changed() || s.is_changed()));
}

fn save(class: Res<Class>, settings: Res<Settings>) {
    let saved = Saved { class: Some(*class), settings: Some(*settings) };
    if let Ok(text) = serde_json::to_string(&saved) {
        store(&text);
    }
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<wasm_bindgen::JsValue> {
    js_sys::Reflect::get(&js_sys::global(), &"localStorage".into()).ok().filter(|v| !v.is_undefined() && !v.is_null())
}

#[cfg(target_arch = "wasm32")]
fn load() -> Option<String> {
    use wasm_bindgen::JsCast;
    let ls = storage()?;
    let get: js_sys::Function = js_sys::Reflect::get(&ls, &"getItem".into()).ok()?.dyn_into().ok()?;
    get.call1(&ls, &KEY.into()).ok()?.as_string()
}

#[cfg(target_arch = "wasm32")]
fn store(text: &str) {
    use wasm_bindgen::JsCast;
    let Some(ls) = storage() else { return };
    if let Some(set) = js_sys::Reflect::get(&ls, &"setItem".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok()) {
        let _ = set.call2(&ls, &KEY.into(), &text.into());
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load() -> Option<String> {
    std::fs::read_to_string(std::env::temp_dir().join(KEY)).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn store(text: &str) {
    let _ = std::fs::write(std::env::temp_dir().join(KEY), text);
}
