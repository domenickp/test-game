//! What a level entity *looks like*.
//!
//! [`crate::level`] decides what things **are** — a solid, a coin, the player. This file
//! decides what they look like, and it is the only file in the codebase that knows the
//! game is drawn with 3D meshes at all.
//!
//! ## Why the renderer moved out of `level.rs`
//!
//! It used to be inline: `spawn_level` hung a `Sprite` on each entity and that was the
//! entire renderer. Extruding the level into 3D breaks that arrangement, because
//! building a cuboid needs `Assets<Mesh>` and `Assets<StandardMaterial>` — and the
//! moment `spawn_level` asks for those, the headless tests in `tests.rs` would have to
//! stand up Bevy's asset system just to check that the player lands on a floor.
//!
//! Splitting the two apart keeps `MinimalPlugins` genuinely minimal. The tests still
//! build the real level with the real `spawn_level`; they simply never register
//! [`add_block_visual`], so nothing ever asks for a mesh. That is the payoff for
//! separating "what exists" from "how it's drawn", and it's why this file exists.
//!
//! ## The level is still two-dimensional
//!
//! Look at [`Block`]: it carries a `Vec2`. Nothing in `level.rs` has a third dimension,
//! and [`crate::physics`] is untouched — still 2D AABBs, still `Vec2` velocities. The
//! renderer supplies the missing axis by extruding each box backwards along Z according
//! to its [`BlockKind`].
//!
//! That separation is the whole point of this checkpoint. A 3D *camera* looking at
//! extruded 2D *data* is indistinguishable from the sprite version it replaced — but
//! unlike the sprite version, it survives being looked at from an angle. Which is what
//! the eventual swing into the lighthouse needs.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::TILE;

/// Whether block materials ignore lighting.
///
/// While this is `true` an unlit `StandardMaterial` emits its `base_color` verbatim —
/// the shader skips lighting *and* camera exposure and hands the raw color straight to
/// the tonemapper, which is exactly what a `Sprite` did. That's deliberate: it makes
/// this checkpoint a strict no-op visually, so any difference you notice on screen is a
/// bug rather than a lighting change you talked yourself into accepting.
///
/// Flip it to `false` once there's a light in the scene and you want the extrusion to
/// actually read as depth. Nothing else has to change.
const UNLIT: bool = true;

// How deep each class of block is, along Z. Terrain is a full tile deep so a wall looks
// like a wall the moment the camera comes off-axis; the things sitting in front of it
// are shallower, because they're props rather than architecture.
const DEPTH_TERRAIN: f32 = TILE;
const DEPTH_PROP: f32 = TILE * 0.5;
const DEPTH_PLAYER: f32 = 20.0;

/// Breathing room between adjacent layers, in world units.
///
/// Without it, one layer's back face lands exactly on the next layer's front face. That
/// mostly survives — the back face is culled — but coplanar surfaces are the classic
/// setup for z-fighting, and a gap costs nothing.
const LAYER_GAP: f32 = 1.0;

/// Z of the level architecture: solids, one-ways, moving platforms.
///
/// The level plane is Z = 0 and everything else is stacked in front of it, which is
/// what makes [`crate::camera`]'s framing math simple — it only ever has to reason
/// about the distance from the camera to this one plane.
pub const LAYER_TERRAIN: f32 = 0.0;

/// Z of things that sit in front of the architecture: coins, spikes, the goal.
///
/// Half of each layer's depth, plus a gap: `(32 + 16) / 2 + 1 = 25`, so terrain occupies
/// `[-16, 16]` and props occupy `[17, 33]`.
pub const LAYER_PROP: f32 = (DEPTH_TERRAIN + DEPTH_PROP) * 0.5 + LAYER_GAP;

/// Z of the player, in front of everything else. `[34, 54]`, by the same arithmetic.
pub const LAYER_PLAYER: f32 = LAYER_PROP + (DEPTH_PROP + DEPTH_PLAYER) * 0.5 + LAYER_GAP;

/// Which kind of thing a [`Block`] is drawn as.
///
/// This exists so the palette lives *here* rather than in the level. `level.rs` shouldn't
/// have an opinion about what color a hazard is, and after this split it doesn't have
/// one — it says `BlockKind::Hazard` and the renderer decides the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockKind {
    Solid,
    OneWay,
    Moving,
    Coin,
    Hazard,
    Player,
    GoalPole,
    GoalFlag,
}

impl BlockKind {
    /// The palette. Flat colors, no textures — a rectangle in a readable color says
    /// "this kills you" perfectly well, and skipping art assets keeps the project to
    /// one crate with no `assets/` directory.
    fn color(self) -> Color {
        match self {
            Self::Solid => Color::srgb(0.30, 0.36, 0.50),
            Self::OneWay => Color::srgb(0.55, 0.44, 0.32),
            Self::Moving => Color::srgb(0.85, 0.62, 0.28),
            Self::Coin => Color::srgb(1.00, 0.84, 0.28),
            Self::Hazard => Color::srgb(0.88, 0.26, 0.36),
            Self::Player => Color::srgb(0.36, 0.78, 0.96),
            Self::GoalPole => Color::srgb(0.85, 0.87, 0.92),
            Self::GoalFlag => Color::srgb(0.34, 0.90, 0.52),
        }
    }

    /// How far this kind of block extends along Z.
    fn depth(self) -> f32 {
        match self {
            Self::Solid | Self::OneWay | Self::Moving => DEPTH_TERRAIN,
            Self::Coin | Self::Hazard | Self::GoalPole | Self::GoalFlag => DEPTH_PROP,
            Self::Player => DEPTH_PLAYER,
        }
    }
}

/// "Draw me as a box this big, in this role."
///
/// Attached by `spawn_level` to everything visible. The size is the same `Vec2` the
/// entity's `Collider` gets, so what you see really is what you collide with — the one
/// exception being coins and hazards, which deliberately lie in opposite directions
/// (see `spawn_level`).
#[derive(Component, Debug, Clone, Copy)]
pub struct Block {
    /// Width and height in world units. Depth is the renderer's business.
    pub size: Vec2,
    pub kind: BlockKind,
}

impl Block {
    pub fn new(kind: BlockKind, width: f32, height: f32) -> Self {
        Self {
            size: Vec2::new(width, height),
            kind,
        }
    }
}

/// Shared mesh and material handles.
///
/// The level is 60x18 tiles, so a fresh `Mesh` and `StandardMaterial` per entity would
/// mean roughly a thousand of each. Bevy batches draw calls by *(mesh, material)* pair,
/// and unique assets defeat that completely — a thousand identical blue cubes would
/// cost a thousand draw calls. Sharing handles collapses that back down to one draw
/// call per distinct box.
#[derive(Resource, Default)]
pub struct BlockAssets {
    /// Cuboid meshes, keyed by their exact size.
    ///
    /// A linear scan over a `Vec` rather than a `HashMap`, because `f32` is not `Hash`
    /// and the level uses under a dozen distinct box sizes. Sizes are computed from the
    /// same constants every time, so the bit patterns match exactly; a missed lookup
    /// would only ever cost a duplicate mesh, never correctness.
    meshes: Vec<(Vec3, Handle<Mesh>)>,
    /// One material per kind, built the first time that kind is seen.
    materials: HashMap<BlockKind, Handle<StandardMaterial>>,
}

impl BlockAssets {
    fn mesh(&mut self, meshes: &mut Assets<Mesh>, size: Vec3) -> Handle<Mesh> {
        if let Some((_, handle)) = self.meshes.iter().find(|(cached, _)| *cached == size) {
            return handle.clone();
        }

        let handle = meshes.add(Cuboid::from_size(size));
        self.meshes.push((size, handle.clone()));
        handle
    }

    fn material(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        kind: BlockKind,
    ) -> Handle<StandardMaterial> {
        self.materials
            .entry(kind)
            .or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: kind.color(),
                    unlit: UNLIT,
                    ..default()
                })
            })
            .clone()
    }
}

/// Give every [`Block`] a body.
///
/// This is an observer on `Add` — Bevy's "a component just appeared on an entity"
/// lifecycle event — rather than a system in `Update`. The difference matters on
/// restart: `spawn_level` runs partway through a frame, and a scheduled system might
/// not get another look until the *next* frame, leaving one frame of invisible level.
/// An `Add` observer runs during the same command flush that created the entity, so the
/// mesh is attached before anything could possibly render without it.
///
/// It also means the level is spawnable without a renderer at all. No `Add` observer
/// registered — as in `tests.rs` — and blocks simply stay bodiless.
pub fn add_block_visual(
    add: On<Add, Block>,
    mut commands: Commands,
    blocks: Query<&Block>,
    mut assets: ResMut<BlockAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(block) = blocks.get(add.entity) else {
        return;
    };

    let size = block.size.extend(block.kind.depth());
    let mesh = assets.mesh(&mut meshes, size);
    let material = assets.material(&mut materials, block.kind);

    commands
        .entity(add.entity)
        .insert((Mesh3d(mesh), MeshMaterial3d(material)));
}
