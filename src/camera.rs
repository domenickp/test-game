//! A smoothed follow-camera, clamped to the level.
//!
//! Two ideas, and both are worth stealing for any 2D game:
//!
//! 1. **Don't snap.** Ease toward the player instead of matching them exactly. A camera
//!    locked rigidly to a jumping player makes the whole world jitter vertically.
//! 2. **Don't show the void.** Clamp so the view never leaves the level, so the player
//!    never sees empty background past the edge.

use bevy::{camera::ScalingMode, prelude::*};

use crate::{VIEW_HEIGHT, level::LevelBounds, physics::step_seconds, player::Player};

/// How aggressively the camera chases the player, in "per second" units.
///
/// Higher is tighter. Around 8 the camera reads as attached-but-springy; drop it to 2
/// and it feels like a drunk cameraman, push it to 40 and you may as well snap.
const CAMERA_SMOOTHING: f32 = 8.0;

/// Spawn the 2D camera.
///
/// Note this is *not* tagged `LevelEntity`, so restarting the level leaves the camera
/// alone. Nice side effect: the camera eases from wherever it was to the spawn point
/// instead of hard-cutting.
pub fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        // `Camera2d` is a marker component. Adding it pulls in the whole 2D rendering
        // setup via Bevy's *required components* mechanism — you don't assemble a camera
        // out of a dozen parts, you declare the one you want.
        Camera2d,
        Projection::from(OrthographicProjection {
            // Keep the vertical field of view fixed at any window size and let the
            // width follow the aspect ratio. The alternative (`WindowSize`) would mean
            // players with taller windows literally see more of the level, which is
            // both a fairness problem and a level-design problem.
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: VIEW_HEIGHT,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
}

/// Ease the camera toward the player, without leaving the level.
pub fn follow_player(
    time: Res<Time>,
    player: Query<&Transform, With<Player>>,
    // `Without<Player>` proves to Bevy that the `&mut Transform` here can't alias the
    // `&Transform` above. `With<Camera2d>` alone wouldn't be enough — Bevy checks
    // whether the queries are *provably* disjoint, and two `With` filters aren't.
    mut camera: Query<(&mut Transform, &Projection), (With<Camera2d>, Without<Player>)>,
    bounds: Res<LevelBounds>,
) {
    let Ok(player_transform) = player.single() else {
        return;
    };
    let Ok((mut camera_transform, projection)) = camera.single_mut() else {
        return;
    };
    // `Projection` is an enum; we only ever build the orthographic variant, but the
    // type system doesn't know that, so we check rather than unwrap.
    let Projection::Orthographic(orthographic) = projection else {
        return;
    };

    // `area` is the world-space rectangle the camera currently sees, maintained by Bevy
    // whenever the window resizes. Reading it means the clamping below adapts to window
    // size for free instead of us recomputing the aspect ratio ourselves.
    let half_view = orthographic.area.size() * 0.5;

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

    // Only x and y. Leave z alone: for a 2D camera it's the near/far reference point,
    // and moving it would silently start clipping sprites.
    camera_transform.translation.x = smoothed.x;
    camera_transform.translation.y = smoothed.y;
}
