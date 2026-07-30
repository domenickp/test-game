//! Hand-rolled platformer physics.
//!
//! Real projects usually reach for a physics crate (`avian2d`, `bevy_rapier2d`). We do
//! it by hand here because a platformer needs *exactly* one thing from physics —
//! "move this box, and don't let it end up inside that box" — and about 60 lines of
//! honest code beats a black box you can't tune.
//!
//! The technique is **axis-separated AABB resolution**:
//!
//! 1. Move along X only. Find every solid you now overlap and shove yourself out
//!    horizontally. Zero your X velocity, because you hit a wall.
//! 2. Move along Y only. Repeat vertically. If the shove pushed you *up*, you landed
//!    on something, so you're on the ground.
//!
//! Doing one axis at a time is the trick that makes the "which side did I hit?"
//! question answerable. If you move diagonally and then look at the overlap, a corner
//! hit is genuinely ambiguous — you can't tell whether the player scraped a wall or
//! landed on a ledge, and you'll pick wrong often enough to be noticeable.

use bevy::prelude::*;

/// Downward acceleration, in world units per second squared. Negative is down.
pub const GRAVITY: f32 = -1800.0;

/// Gravity is multiplied by this while falling.
///
/// Real gravity is symmetric; good platformer gravity is not. Falling faster than you
/// rose makes a jump feel snappy and deliberate instead of floaty. This is the single
/// cheapest game-feel improvement in the whole file.
pub const FALL_GRAVITY_MULTIPLIER: f32 = 1.4;

/// Terminal velocity. Half of a safety net — see [`MAX_TIMESTEP`] for the other half.
pub const MAX_FALL_SPEED: f32 = 900.0;

/// The longest timestep the simulation will take in one frame, in seconds.
///
/// Collision here is *discrete*: an entity teleports from its old position to its new one
/// and we then look for overlaps. Nothing checks the space swept along the way, so an
/// entity that moves further than a solid is thick in a single step passes clean through
/// it. In this game the symptom was a death: after one long frame the player would drop
/// through the floor and out of the world without ever touching a spike.
///
/// Long frames are not hypothetical. The very first `Update` has to absorb window creation
/// and GPU initialization — easily 200-400ms — and any hitch afterwards does the same.
///
/// Clamping the timestep bounds how far anything can move per step, which makes tunneling
/// impossible rather than merely unlikely:
///
/// ```text
/// worst-case travel = MAX_FALL_SPEED × MAX_TIMESTEP = 900 / 30 = 30 units
/// thinnest solid    = TILE                          =            32 units
/// ```
///
/// (`tunneling_is_impossible_by_construction` in `tests.rs` asserts that inequality, so
/// retuning either constant fails loudly instead of quietly reintroducing the bug.)
///
/// The cost is that a frame slower than 1/30s makes the game run in slow motion rather
/// than skipping ahead. For a platformer that's the right trade: slow is playable, and
/// falling through the floor is not. The principled alternative is `FixedUpdate`, which
/// takes as many fixed steps as needed to catch up — see the README.
pub const MAX_TIMESTEP: f32 = 1.0 / 30.0;

/// Seconds elapsed since the last frame, clamped to [`MAX_TIMESTEP`].
///
/// Every system that integrates over time uses this instead of `time.delta_secs()` so the
/// whole simulation — physics, input, camera, and the run timer — agrees on how much time
/// a frame was worth.
pub fn step_seconds(time: &Time) -> f32 {
    time.delta_secs().min(MAX_TIMESTEP)
}

/// Contact tolerance, in world units. Small, but load-bearing.
///
/// When you rest on a surface, the two boxes' edges are at *exactly* the same coordinate,
/// so the distance between their centers is *exactly* the sum of their half-extents. That
/// is a knife-edge for floating-point arithmetic: `push_out` puts you at the contact point
/// to within an ULP or so, and next frame the recomputed distance is as likely to be a
/// hair under the boundary as a hair over it.
///
/// A hair under means "overlapping" — and that's a real bug, not a rounding curiosity. If
/// the X pass thinks a player standing on a platform overlaps it, the nearest way out
/// along X is *sideways by half the platform*, so the player gets flung off a surface they
/// were resting on peacefully.
///
/// The fix is to stop asking for exact equality. Along the axis being resolved, touching
/// still counts as a collision. On the *other* axis we require [`SKIN`] units of genuine
/// overlap, so mere contact can never be mistaken for penetration. At 0.05 units — 1/640th
/// of a tile — it's thousands of times larger than the error it absorbs and far too small
/// to see.
///
/// Every AABB collision implementation has a constant like this. It's the least obvious
/// necessary line in the file, which is why it gets the longest comment.
const SKIN: f32 = 0.05;

/// An axis-aligned bounding box, centered on the entity's `Transform`.
///
/// Stored as *half*-extents because that's the form every collision check wants: two
/// boxes overlap on an axis when the distance between their centers is less than the
/// sum of their half-extents. Storing full width would mean dividing by two everywhere.
#[derive(Component, Debug, Clone, Copy)]
pub struct Collider {
    pub half_size: Vec2,
}

impl Collider {
    /// Build a collider from a full width and height.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            half_size: Vec2::new(width * 0.5, height * 0.5),
        }
    }
}

/// Current speed in world units per second.
///
/// Note that this is a *component*, not a field on some `Player` struct. Anything that
/// should move gets one. That's the ECS mindset: behavior attaches to data, so
/// `move_and_collide` below works on the player without ever mentioning the player.
#[derive(Component, Debug, Default)]
pub struct Velocity(pub Vec2);

/// Marks an entity that other things collide against: ground, walls, platforms.
#[derive(Component)]
pub struct Solid;

/// Marks a [`Solid`] you can jump up *through* but still land on top of.
///
/// The classic wooden-platform behavior. Cheap to implement, and it makes vertical
/// level design much less fiddly, since the player can never bonk their head on a
/// route they're supposed to take.
#[derive(Component)]
pub struct OneWay;

/// Marks an entity that [`apply_gravity`] should pull downward.
#[derive(Component)]
pub struct Gravity;

/// The result of last frame's vertical collision check.
///
/// Written by [`move_and_collide`], read by [`crate::player::player_input`] (to decide
/// whether a jump is allowed) and by [`carry_platform_riders`] (to decide who is
/// standing on a moving platform).
#[derive(Component, Debug, Default)]
pub struct GroundState {
    /// True if we ended the last collision pass resting on something solid.
    pub on_ground: bool,
    /// *What* we're standing on, if anything. Needed for moving platforms — knowing
    /// "I'm grounded" isn't enough, we need to know whose motion to inherit.
    pub ground: Option<Entity>,
}

/// A [`Solid`] that slides back and forth between two points forever.
#[derive(Component, Debug)]
pub struct MovingPlatform {
    /// One end of the path, in world units.
    pub start: Vec2,
    /// The other end.
    pub end: Vec2,
    /// Travel speed in world units per second.
    pub speed: f32,
    /// How far along the path we are, from 0.0 (`start`) to 1.0 (`end`).
    pub progress: f32,
    /// `1.0` when heading toward `end`, `-1.0` when heading back.
    pub direction: f32,
    /// How far the platform moved during the current frame.
    ///
    /// Recorded so [`carry_platform_riders`] can apply the same offset to whoever is
    /// standing on top. Without this, the platform slides out from under the player.
    pub last_delta: Vec2,
}

impl MovingPlatform {
    pub fn new(start: Vec2, end: Vec2, speed: f32) -> Self {
        Self {
            start,
            end,
            speed,
            progress: 0.0,
            direction: 1.0,
            last_delta: Vec2::ZERO,
        }
    }
}

/// Do two axis-aligned boxes overlap?
///
/// Used by the gameplay systems (coins, spikes, the goal) which just want a yes/no
/// answer and don't need the push-out math that [`move_and_collide`] does.
pub fn overlaps(a_center: Vec2, a: &Collider, b_center: Vec2, b: &Collider) -> bool {
    let distance = (a_center - b_center).abs();
    let combined = a.half_size + b.half_size;
    distance.x < combined.x && distance.y < combined.y
}

/// Accelerate everything with a [`Gravity`] component downward.
pub fn apply_gravity(time: Res<Time>, mut movers: Query<&mut Velocity, With<Gravity>>) {
    let dt = step_seconds(&time);

    for mut velocity in &mut movers {
        // Asymmetric gravity: heavier on the way down. See FALL_GRAVITY_MULTIPLIER.
        let gravity = if velocity.0.y <= 0.0 {
            GRAVITY * FALL_GRAVITY_MULTIPLIER
        } else {
            GRAVITY
        };

        velocity.0.y = (velocity.0.y + gravity * dt).max(-MAX_FALL_SPEED);
    }
}

/// Slide moving platforms along their paths, ping-ponging at each end.
pub fn move_platforms(
    time: Res<Time>,
    mut platforms: Query<(&mut Transform, &mut MovingPlatform)>,
) {
    let dt = step_seconds(&time);

    for (mut transform, mut platform) in &mut platforms {
        // `progress` is normalized 0..1, so convert the platform's speed (world units
        // per second) into progress-per-second by dividing by the path length. This is
        // what makes a long platform and a short one move at the same visual speed.
        let path_length = platform.start.distance(platform.end).max(f32::EPSILON);
        platform.progress += platform.direction * (platform.speed / path_length) * dt;

        if platform.progress >= 1.0 {
            platform.progress = 1.0;
            platform.direction = -1.0;
        } else if platform.progress <= 0.0 {
            platform.progress = 0.0;
            platform.direction = 1.0;
        }

        let target = platform.start.lerp(platform.end, platform.progress);

        // Record the frame's motion *before* committing it, so riders can copy it.
        platform.last_delta = target - transform.translation.truncate();
        transform.translation.x = target.x;
        transform.translation.y = target.y;
    }
}

/// Move anything standing on a moving platform by that platform's own motion.
///
/// This runs *after* [`move_platforms`] and *before* [`move_and_collide`]. That order
/// is the whole reason it works: the platform has already moved, so we hand its rider
/// the same offset, and only then does collision resolution get a look at the result.
/// Skip this system and the player just slides off the back of every platform.
pub fn carry_platform_riders(
    platforms: Query<&MovingPlatform>,
    // `Without<MovingPlatform>` isn't just a nicety. Bevy refuses to compile a system
    // holding `&MovingPlatform` and `&mut Transform` over potentially the same entity —
    // that would be aliased mutability. The filter proves the two queries are disjoint.
    mut riders: Query<(&mut Transform, &GroundState), Without<MovingPlatform>>,
) {
    for (mut transform, ground) in &mut riders {
        // `ground` is from last frame's collision pass, which is fine: if you were on
        // the platform a frame ago you're almost certainly still on it now, and being
        // one frame stale here is invisible.
        let Some(ground_entity) = ground.ground else {
            continue;
        };
        let Ok(platform) = platforms.get(ground_entity) else {
            continue;
        };

        transform.translation.x += platform.last_delta.x;
        transform.translation.y += platform.last_delta.y;
    }
}

/// Integrate velocity into position, then push back out of anything solid.
///
/// This is the heart of the game. See the module docs for the overall shape; the
/// per-axis details are commented inline.
pub fn move_and_collide(
    time: Res<Time>,
    mut movers: Query<(&mut Transform, &mut Velocity, &Collider, &mut GroundState)>,
    // Solids are everything we collide *against*. `Without<Velocity>` makes this query
    // provably disjoint from `movers` above (which implies `With<Velocity>` by asking
    // for `&mut Velocity`), so Bevy accepts both `Transform` accesses in one system.
    //
    // `Has<OneWay>` is a read-only "does this entity have the component?" — it doesn't
    // filter anything out, it just reports a `bool` per row.
    solids: Query<(Entity, &Transform, &Collider, Has<OneWay>), (With<Solid>, Without<Velocity>)>,
) {
    let dt = step_seconds(&time);

    for (mut transform, mut velocity, collider, mut ground) in &mut movers {
        // Assume airborne until a downward collision proves otherwise.
        ground.on_ground = false;
        ground.ground = None;

        // ---- X axis -------------------------------------------------------------
        transform.translation.x += velocity.0.x * dt;
        let mut position = transform.translation.truncate();

        for (_, solid_transform, solid_collider, one_way) in &solids {
            // One-ways are deliberately transparent sideways — you should be able to
            // walk right through the edge of a wooden platform, not catch on it.
            if one_way {
                continue;
            }

            let solid_position = solid_transform.translation.truncate();
            let combined = collider.half_size + solid_collider.half_size;
            let delta = position - solid_position;

            // Resolving X, so require real overlap on Y — see [`SKIN`]. Without that
            // margin, standing on a platform reads as overlapping it, and the "nearest
            // way out" is sideways off the edge.
            if delta.x.abs() >= combined.x || delta.y.abs() >= combined.y - SKIN {
                continue; // no overlap
            }

            // Overlapping: place ourselves exactly against the solid's side, on whichever
            // side of its center we're already on.
            position.x = solid_position.x + combined.x * delta.x.signum();
            velocity.0.x = 0.0;
        }

        transform.translation.x = position.x;

        // ---- Y axis -------------------------------------------------------------

        // Remember where our feet were *before* moving. One-way platforms need this:
        // "was I above this platform a moment ago?" is the question that distinguishes
        // landing on it from jumping up through it.
        let previous_bottom = position.y - collider.half_size.y;

        transform.translation.y += velocity.0.y * dt;
        let mut position = transform.translation.truncate();

        for (solid_entity, solid_transform, solid_collider, one_way) in &solids {
            let solid_position = solid_transform.translation.truncate();
            let combined = collider.half_size + solid_collider.half_size;
            let delta = position - solid_position;

            // Mirror image of the X pass: resolving Y, so require real overlap on X.
            // This side matters too — brushing a wall on the way down should not count
            // as overlapping it, or you'd get shoved vertically and told you'd landed.
            if delta.x.abs() >= combined.x - SKIN || delta.y.abs() >= combined.y {
                continue; // no overlap
            }

            if one_way {
                let platform_top = solid_position.y + solid_collider.half_size.y;
                // Only solid if we're moving downward *and* started above the surface.
                // The 1.0 unit of slack absorbs the tiny overshoot from having already
                // moved this frame; without it you'd fall through at high speed.
                if velocity.0.y > 0.0 || previous_bottom < platform_top - 1.0 {
                    continue;
                }
            }

            position.y = solid_position.y + combined.y * delta.y.signum();

            // Pushed *upward* means the solid was below us: we landed on it.
            if delta.y > 0.0 {
                ground.on_ground = true;
                ground.ground = Some(solid_entity);
            }

            velocity.0.y = 0.0;
        }

        transform.translation.y = position.y;
    }
}
