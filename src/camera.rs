//! A smoothed follow-camera, clamped to the level.
//!
//! Three ideas, and the first two are worth stealing for any 2D game:
//!
//! 1. **Don't snap.** Ease toward the player instead of matching them exactly. A camera
//!    locked rigidly to a jumping player makes the whole world jitter vertically.
//! 2. **Don't show the void.** Clamp so the view never leaves the level, so the player
//!    never sees empty background past the edge.
//! 3. **Fake orthographic with a long lens.** This is a 3D perspective camera, but it
//!    sits far enough back with a narrow enough field of view that the result is
//!    indistinguishable from the flat 2D view it replaced. See [`CAMERA_FOV`].
//!
//! ## Why not just use an orthographic camera?
//!
//! Because of where this is going. The plan is for the camera to swing from this side-on
//! view into a first-person view inside the lighthouse at the end of the level, and you
//! **cannot interpolate an orthographic projection into a perspective one** — they aren't
//! the same family of matrix, and `Projection` is an enum, so there is nothing to lerp.
//!
//! Staying perspective the whole time sidesteps that entirely. The swing becomes an
//! animation of two ordinary numbers — widen [`CAMERA_FOV`], shorten the distance — which
//! is a reverse dolly zoom, and it happens to be a genuinely good-looking way to reveal
//! that a world was 3D all along.

use bevy::{
    core_pipeline::tonemapping::{DebandDither, Tonemapping},
    prelude::*,
};

use crate::{VIEW_HEIGHT, level::LevelBounds, physics::step_seconds, player::Player};

/// How aggressively the camera chases the player, in "per second" units.
///
/// Higher is tighter. Around 8 the camera reads as attached-but-springy; drop it to 2
/// and it feels like a drunk cameraman, push it to 40 and you may as well snap.
const CAMERA_SMOOTHING: f32 = 8.0;

/// The camera's vertical field of view, in radians. 10 degrees — a long lens.
///
/// A perspective camera makes near things bigger than far things; the whole trick here is
/// to make the level's depth negligible compared to how far away it is. At the distance
/// [`camera_distance`] works out to, the ~54 units of depth between the back of a wall
/// and the front of the player produce under 2% of size difference across the entire
/// scene. Real, measurable, and completely invisible in motion.
///
/// Narrowing this further flattens the image even more (and pushes the camera further
/// back to compensate, automatically). Widening it is the first half of the swing.
pub const CAMERA_FOV: f32 = 10.0 * core::f32::consts::PI / 180.0;

/// The frustum culling distance, in world units.
///
/// Bevy builds perspective projections with an *infinite* far plane, so unlike `near`
/// this number never appears in the projection matrix — it only bounds the frustum used
/// to decide what to bother drawing. That makes it easy to overlook, and the default of
/// `1000.0` is nearer than the camera itself, which would cull the entire level.
const CAMERA_FAR: f32 = 10_000.0;

/// How far back the camera sits, in world units.
///
/// Derived rather than hardcoded, so that [`CAMERA_FOV`] is the only dial: whatever field
/// of view you pick, the camera backs off to the distance that frames exactly
/// [`VIEW_HEIGHT`] world units at the level plane. At 10 degrees that lands around 2560.
///
/// A function rather than a `const` only because `tan` isn't callable in a const context.
pub fn camera_distance() -> f32 {
    (VIEW_HEIGHT * 0.5) / (CAMERA_FOV * 0.5).tan()
}

/// Spawn the camera.
///
/// Note this is *not* tagged `LevelEntity`, so restarting the level leaves the camera
/// alone. Nice side effect: the camera eases from wherever it was to the spawn point
/// instead of hard-cutting.
pub fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        // `Camera3d` is (nearly) a marker component. Adding it pulls in the whole 3D
        // rendering setup via Bevy's *required components* mechanism — you don't assemble
        // a camera out of a dozen parts, you declare the one you want.
        Camera3d::default(),
        Projection::from(PerspectiveProjection {
            fov: CAMERA_FOV,
            far: CAMERA_FAR,
            ..default()
        }),
        // A Bevy camera looks down its own -Z, so parking it at +Z with no rotation
        // points it straight at the level plane. `follow_player` then moves it in X and
        // Y only, leaving this distance — and therefore the framing — alone.
        Transform::from_xyz(0.0, 0.0, camera_distance()),
        // These two are not details you can skip, and neither is visible in this file
        // until you go looking: `Camera2d` and `Camera3d` register *different defaults*
        // for both, so swapping the camera type silently changes how every pixel in the
        // game is post-processed.
        //
        // `Camera2d` defaults to `Tonemapping::None`; `Camera3d` defaults to
        // `TonyMcMapface`, a film-response curve that darkened the whole palette by up
        // to five points per channel. `None` is correct while the palette is flat unlit
        // color — there is no HDR range to compress, so a curve can only distort values
        // that were already display-ready.
        Tonemapping::None,
        // `Camera2d` defaults to `DebandDither::Disabled`; `Camera3d` enables it.
        // Debanding hides banding in smooth gradients by adding sub-LSB noise, which is
        // exactly the wrong trade for flat color: there are no gradients to band, and it
        // was dithering every solid surface by ±1.
        DebandDither::Disabled,
        // Both belong with `render::UNLIT`. Turn all three around together when the
        // lighting lands: real lights produce values above 1.0 and gradients across a
        // surface, and *those* genuinely want a tone curve and debanding.
    ));
}

/// Ease the camera toward the player, without leaving the level.
pub fn follow_player(
    time: Res<Time>,
    player: Query<&Transform, With<Player>>,
    // `Without<Player>` proves to Bevy that the `&mut Transform` here can't alias the
    // `&Transform` above. `With<Camera3d>` alone wouldn't be enough — Bevy checks
    // whether the queries are *provably* disjoint, and two `With` filters aren't.
    mut camera: Query<(&mut Transform, &Projection), (With<Camera3d>, Without<Player>)>,
    bounds: Res<LevelBounds>,
) {
    let Ok(player_transform) = player.single() else {
        return;
    };
    let Ok((mut camera_transform, projection)) = camera.single_mut() else {
        return;
    };
    // `Projection` is an enum; we only ever build the perspective variant, but the type
    // system doesn't know that, so we check rather than unwrap.
    let Projection::Perspective(perspective) = projection else {
        return;
    };

    // How much of the world is on screen, as a half-width and half-height.
    //
    // The orthographic version of this camera got the answer for free: `area` is a
    // rectangle the render pipeline keeps up to date. A perspective frustum has no single
    // answer — it depends how far away you measure — so we compute it against the plane
    // we actually care about, `render::LAYER_TERRAIN` at Z = 0, where the level lives.
    //
    // `aspect_ratio` is maintained by Bevy's camera system as the window resizes, so
    // clamping still adapts to window shape for free.
    let half_height = (perspective.fov * 0.5).tan() * camera_transform.translation.z;
    let half_view = Vec2::new(half_height * perspective.aspect_ratio, half_height);

    let mut target = player_transform.translation.truncate();

    // Clamp so the *edges* of the view stay inside the level, which means clamping the
    // *center* to the level shrunk by half a view on each side.
    let min = bounds.0.min + half_view;
    let max = bounds.0.max - half_view;

    // If the level is smaller than the view on an axis, `min > max` and `clamp` would
    // panic. Center on the level instead — the correct thing to show anyway.
    target.x = if min.x <= max.x {
        target.x.clamp(min.x, max.x)
    } else {
        bounds.0.center().x
    };
    target.y = if min.y <= max.y {
        target.y.clamp(min.y, max.y)
    } else {
        bounds.0.center().y
    };

    // Exponential smoothing, done the frame-rate-independent way.
    //
    // The tempting version is `current.lerp(target, 0.1)` — but that moves 10% of the
    // remaining distance *per frame*, so the camera is twice as fast at 120fps as at
    // 60fps. Running it through `1 - e^(-rate · dt)` converts a per-frame fraction into
    // a per-second rate, and the camera then behaves identically at any frame rate.
    let t = 1.0 - (-CAMERA_SMOOTHING * step_seconds(&time)).exp();
    let smoothed = camera_transform.translation.truncate().lerp(target, t);

    // Only x and y. Leave z alone: it's the camera's distance from the level plane, and
    // changing it would rescale the whole image. Dollying it in is the second half of
    // the swing into first person, and nothing else should touch it.
    camera_transform.translation.x = smoothed.x;
    camera_transform.translation.y = smoothed.y;
}
