//! What the app remembers per app, and saved groups of windows.

use serde::{Deserialize, Serialize};

use crate::mirroring::session::MirrorOptions;

const WHOLE_PHONE: &str = "phone";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum Quality {
    Smooth,
    Balanced,
    Sharp,
    Custom,
}

/// Longest side, bit rate and frame rate for each one-click choice.
const QUALITY_PRESETS: [(Quality, u32, u32, u32); 3] = [
    (Quality::Smooth, 1280, 4_000_000, 60),
    (Quality::Balanced, 1600, 8_000_000, 60),
    (Quality::Sharp, 0, 16_000_000, 60),
];

pub fn quality_of(options: &MirrorOptions) -> Quality {
    QUALITY_PRESETS
        .iter()
        .find(|(_, size, rate, fps)| (*size, *rate, *fps) == (options.max_size, options.bit_rate, options.max_fps))
        .map_or(Quality::Custom, |(quality, ..)| *quality)
}

pub fn apply_quality(options: &mut MirrorOptions, quality: Quality) {
    if let Some((_, size, rate, fps)) = QUALITY_PRESETS.iter().find(|(preset, ..)| *preset == quality) {
        (options.max_size, options.bit_rate, options.max_fps) = (*size, *rate, *fps);
    }
}

/// A key that taps one spot, stored as a fraction of the screen so it survives
/// a change of resolution.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct KeyBinding {
    pub key: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct AppProfile {
    pub quality: Option<Quality>,
    pub position: Option<[f32; 2]>,
    pub keys: Vec<KeyBinding>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkspaceWindow {
    pub serial: String,
    pub package: Option<String>,
    pub title: String,
    pub position: Option<[f32; 2]>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Workspace {
    pub name: String,
    pub windows: Vec<WorkspaceWindow>,
}

pub fn profile_key(package: Option<&str>) -> &str {
    package.unwrap_or(WHOLE_PHONE)
}

/// Adds a binding. A key can only tap one spot, so an older use of it is replaced.
pub fn set_binding(bindings: &mut Vec<KeyBinding>, key: &str, spot: [f32; 2]) {
    bindings.retain(|binding| binding.key != key);
    bindings.push(KeyBinding { key: key.to_string(), x: spot[0], y: spot[1] });
}

/// The binding within `radius` points of `spot`, on a screen `size` points large.
pub fn binding_at(bindings: &[KeyBinding], spot: [f32; 2], size: [f32; 2], radius: f32) -> Option<usize> {
    bindings.iter().position(|binding| {
        let (x, y) = ((binding.x - spot[0]) * size[0], (binding.y - spot[1]) * size[1]);
        x * x + y * y <= radius * radius
    })
}

/// Replaces a workspace of the same name, so saving again updates it.
pub fn store_workspace(workspaces: &mut Vec<Workspace>, workspace: Workspace) {
    match workspaces.iter_mut().find(|existing| existing.name == workspace.name) {
        Some(existing) => *existing = workspace,
        None => workspaces.push(workspace),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_recognized_and_applied() {
        let mut options = MirrorOptions::default();
        assert_eq!(quality_of(&options), Quality::Balanced);
        apply_quality(&mut options, Quality::Sharp);
        assert_eq!((options.max_size, options.bit_rate), (0, 16_000_000));
        assert_eq!(quality_of(&options), Quality::Sharp);
        options.max_fps = 120;
        assert_eq!(quality_of(&options), Quality::Custom);
    }

    #[test]
    fn a_key_only_taps_one_spot() {
        let mut bindings = Vec::new();
        set_binding(&mut bindings, "W", [0.1, 0.2]);
        set_binding(&mut bindings, "A", [0.3, 0.4]);
        set_binding(&mut bindings, "W", [0.5, 0.6]);
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[1], KeyBinding { key: "W".to_string(), x: 0.5, y: 0.6 });
    }

    #[test]
    fn finds_a_binding_by_distance_on_screen() {
        let bindings = [KeyBinding { key: "W".to_string(), x: 0.5, y: 0.5 }];
        assert_eq!(binding_at(&bindings, [0.52, 0.5], [400.0, 800.0], 16.0), Some(0));
        assert_eq!(binding_at(&bindings, [0.6, 0.5], [400.0, 800.0], 16.0), None);
    }

    #[test]
    fn saving_a_workspace_again_updates_it() {
        let workspace = |count: usize| Workspace {
            name: "Work".to_string(),
            windows: vec![WorkspaceWindow { serial: "a".to_string(), package: None, title: "A".to_string(), position: None }; count],
        };
        let mut workspaces = Vec::new();
        store_workspace(&mut workspaces, workspace(1));
        store_workspace(&mut workspaces, workspace(3));
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0].windows.len(), 3);
    }

    #[test]
    fn the_whole_phone_has_its_own_profile() {
        assert_eq!(profile_key(None), "phone");
        assert_eq!(profile_key(Some("com.a.b")), "com.a.b");
    }
}
