//! The level: ASCII art in, entities out.
//!
//! [`LEVEL`] *is* the level editor. Each character becomes zero or more entities, and
//! adding a new kind of tile means adding a character to the art and a `match` arm to
//! [`spawn_level`]. Nothing else in the codebase needs to know about it.
//!
//! Storing a level as a string literal is not a toy technique — it's a genuinely good
//! fit for a small grid-based game, because the source file is also the picture. When
//! you outgrow it (irregular geometry, entity properties, more than one level) the next
//! step is usually a data file loaded through Bevy's asset system, at which point
//! `spawn_level` becomes "parse the asset" instead of "parse the const".
//!
//! ## Coordinates
//!
//! ASCII art reads top-to-bottom, but world Y goes *up*. [`tile_center`] does that flip
//! once, in one place, so the rest of the file can think in rows and columns. Getting
//! this backwards is the classic first bug in any tilemap loader.

use bevy::prelude::*;

use crate::{
    GameState, TILE,
    gameplay::{Coin, Goal, Hazard, Progress},
    physics::{Collider, Gravity, GroundState, MovingPlatform, OneWay, Solid, Velocity},
    player::Player,
};

/// The level.
///
/// | Char | Meaning |
/// |---|---|
/// | `#` | Solid block — walls, floors, ceilings |
/// | `=` | One-way platform — jump up through it, land on top of it |
/// | `o` | Coin |
/// | `^` | Spikes — touching them costs you a life |
/// | `P` | Player spawn (exactly one) |
/// | `F` | Goal flag |
/// | `.` | Empty space |
///
/// Every row must be the same length; [`spawn_level`] asserts it. The ruler above the
/// art is there so you can line new tiles up with the moving platforms in
/// [`MOVING_PLATFORMS`], which are addressed by column number.
pub const LEVEL: &[&str] = &[
    //         000000000011111111112222222222333333333344444444445555555555
    //         012345678901234567890123456789012345678901234567890123456789
    "............................................................", // r00
    "............................................................", // r01
    "........................................................F...", // r02
    "......................................................====..", // r03
    ".....................................................o......", // r04
    ".....................................................######.", // r05
    "............................................................", // r06
    "..................................................o.........", // r07
    "............................................................", // r08
    "......................oo....................................", // r09
    ".....................====...................................", // r10
    "..............................................o.............", // r11
    "...........oo.....oo........................................", // r12
    "...............======...........oo...........===............", // r13
    "..........###.............................o.................", // r14
    ".........o...............................###................", // r15
    "..P..o.##..............................o....................", // r16
    "#########################^^#..........########^^^###########", // r17
];

/// A moving platform, described in tile coordinates.
///
/// These live in a list rather than in the ASCII art because a moving platform needs
/// *two* positions, and a single character can only express one. Trying to encode a
/// path in ASCII gets clever and unreadable fast; a struct with named fields doesn't.
pub struct MovingPlatformSpec {
    /// Starting tile (column, row) — same coordinate system as [`LEVEL`].
    pub from: (f32, f32),
    /// Ending tile (column, row).
    pub to: (f32, f32),
    /// Platform width in tiles.
    pub width_tiles: f32,
    /// Speed in world units per second.
    pub speed: f32,
}

/// The level's moving platforms.
///
/// The first crosses the bottomless pit at columns 28–37; without it that gap is
/// impassable. The second is the elevator up to the goal ledge.
pub const MOVING_PLATFORMS: &[MovingPlatformSpec] = &[
    MovingPlatformSpec {
        from: (29.0, 15.0),
        to: (36.0, 15.0),
        width_tiles: 2.0,
        speed: 105.0,
    },
    MovingPlatformSpec {
        from: (50.0, 12.0),
        to: (50.0, 6.0),
        width_tiles: 2.0,
        speed: 85.0,
    },
];

// The palette. Flat colors, no textures — a rectangle in a readable color communicates
// "this kills you" just fine, and skipping art assets keeps the project to one crate.
const COLOR_SOLID: Color = Color::srgb(0.30, 0.36, 0.50);
const COLOR_ONE_WAY: Color = Color::srgb(0.55, 0.44, 0.32);
const COLOR_MOVING: Color = Color::srgb(0.85, 0.62, 0.28);
const COLOR_COIN: Color = Color::srgb(1.00, 0.84, 0.28);
const COLOR_HAZARD: Color = Color::srgb(0.88, 0.26, 0.36);
const COLOR_PLAYER: Color = Color::srgb(0.36, 0.78, 0.96);
const COLOR_GOAL_POLE: Color = Color::srgb(0.85, 0.87, 0.92);
const COLOR_GOAL_FLAG: Color = Color::srgb(0.34, 0.90, 0.52);

/// Marks everything that belongs to the current level attempt.
///
/// Restarting is then "despawn everything with this marker, then build it again" — no
/// bookkeeping, no reset logic per entity type, and no chance of forgetting one. The
/// camera and HUD deliberately *don't* have it, so they survive a restart.
#[derive(Component)]
pub struct LevelEntity;

/// Where the player respawns after dying. Filled in by [`spawn_level`].
#[derive(Resource, Debug, Default)]
pub struct PlayerSpawn(pub Vec2);

/// The level's extent in world units, used by the camera to avoid showing the void.
#[derive(Resource, Debug, Default)]
pub struct LevelBounds(pub Rect);

/// "Please (re)build the level."
///
/// An event, not a function call, so that [`crate::ui::restart_input`] can ask for a
/// restart without needing any of the arguments a rebuild requires. Triggering an event
/// is how one part of a Bevy app tells another part *what happened* rather than *what
/// to do* — the sender stays ignorant of the work involved.
#[derive(Event)]
pub struct SpawnLevel;

/// Kick off the first build, during `Startup`.
pub fn request_first_level(mut commands: Commands) {
    // `trigger` queues the event. Observers run when Bevy next flushes commands, which
    // is before `Update` — so the level exists by the time the first frame is drawn.
    commands.trigger(SpawnLevel);
}

/// Tear down the current level and build a fresh one from [`LEVEL`].
///
/// This is an **observer**, not a scheduled system: it runs whenever [`SpawnLevel`] is
/// triggered, from wherever. The `On<SpawnLevel>` first parameter is what marks it as
/// one; every parameter after that is an ordinary system parameter.
pub fn spawn_level(
    _trigger: On<SpawnLevel>,
    mut commands: Commands,
    previous: Query<Entity, With<LevelEntity>>,
    mut progress: ResMut<Progress>,
    mut spawn_point: ResMut<PlayerSpawn>,
    mut bounds: ResMut<LevelBounds>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    // Clean slate. Despawning is queued, so it's safe to spawn the new level below in
    // the same run — these commands are applied in order, and the new entities are
    // distinct from the old ones.
    for entity in &previous {
        commands.entity(entity).despawn();
    }

    let rows = LEVEL.len();
    let columns = LEVEL[0].len();
    assert!(
        LEVEL.iter().all(|row| row.len() == columns),
        "every row of LEVEL must be the same length"
    );

    *progress = Progress::default();
    *bounds = LevelBounds(Rect::new(
        0.0,
        0.0,
        columns as f32 * TILE,
        rows as f32 * TILE,
    ));

    // Coming back from the win screen has to put us back into `Playing`. Doing it here
    // rather than in the restart handler means *any* rebuild resets the state, which is
    // one less thing to remember when you add another way to restart.
    next_state.set(GameState::Playing);

    let mut player_spawned = false;

    for (row, line) in LEVEL.iter().enumerate() {
        for (column, character) in line.chars().enumerate() {
            let center = tile_center(column as f32, row as f32);

            match character {
                '.' | ' ' => {}

                '#' => {
                    // `commands.spawn` takes a *tuple* of components. There is no
                    // "block" type anywhere — a block is just this specific pile of
                    // components, assembled here.
                    commands.spawn((
                        Sprite::from_color(COLOR_SOLID, Vec2::splat(TILE)),
                        Transform::from_xyz(center.x, center.y, 0.0),
                        Collider::new(TILE, TILE),
                        Solid,
                        LevelEntity,
                    ));
                }

                '=' => {
                    // Visually thin, and its collider matches: a one-way platform reads
                    // as something you stand *on* rather than a block you go around.
                    let height = TILE * 0.35;
                    commands.spawn((
                        Sprite::from_color(COLOR_ONE_WAY, Vec2::new(TILE, height)),
                        // Sit it at the top of its tile, so the surface you land on is
                        // where the grid line is. Off-by-a-few-pixels here is the kind
                        // of thing players feel without being able to name.
                        Transform::from_xyz(center.x, center.y + (TILE - height) * 0.5, 0.0),
                        Collider::new(TILE, height),
                        Solid,
                        OneWay,
                        LevelEntity,
                    ));
                }

                'o' => {
                    let size = TILE * 0.45;
                    commands.spawn((
                        Sprite::from_color(COLOR_COIN, Vec2::splat(size)),
                        Transform::from_xyz(center.x, center.y, 1.0),
                        // A slightly generous collider. Pickups should be easier to
                        // touch than they look; hazards (below) are the opposite.
                        Collider::new(size * 1.3, size * 1.3),
                        Coin {
                            base_y: center.y,
                            // Seed each coin's bob from its position so they don't all
                            // pulse in lockstep. Cheap, deterministic, no RNG needed.
                            phase: column as f32 * 0.7 + row as f32 * 1.3,
                        },
                        LevelEntity,
                    ));
                    progress.total_coins += 1;
                }

                '^' => {
                    // Short, and parked at the bottom of the tile: you can stand on the
                    // block behind a spike row without dying, and a near-miss jump
                    // stays a near miss. Forgiving hitboxes on hazards, generous
                    // hitboxes on pickups — that asymmetry is deliberate everywhere.
                    let height = TILE * 0.4;
                    commands.spawn((
                        Sprite::from_color(COLOR_HAZARD, Vec2::new(TILE * 0.9, height)),
                        Transform::from_xyz(center.x, center.y - (TILE - height) * 0.5, 1.0),
                        Collider::new(TILE * 0.8, height * 0.7),
                        Hazard,
                        LevelEntity,
                    ));
                }

                'P' => {
                    assert!(!player_spawned, "LEVEL must contain exactly one 'P'");
                    player_spawned = true;

                    spawn_point.0 = center;

                    commands.spawn((
                        Sprite::from_color(COLOR_PLAYER, Vec2::new(20.0, 28.0)),
                        // z = 2 puts the player in front of coins (z = 1) and blocks
                        // (z = 0). In 2D, z is purely draw order.
                        Transform::from_xyz(center.x, center.y, 2.0),
                        // Matches the sprite exactly, and both are comfortably smaller
                        // than a 32-unit tile so the player fits through a one-tile gap
                        // without catching on the corners.
                        Collider::new(20.0, 28.0),
                        Player::default(),
                        Velocity::default(),
                        GroundState::default(),
                        Gravity,
                        LevelEntity,
                    ));
                }

                'F' => {
                    // A parent entity with a child: the pole carries the collider and
                    // the `Goal` marker, and the flag is a child so it inherits the
                    // pole's `Transform`. Move the pole and the flag comes along.
                    commands.spawn((
                        Sprite::from_color(COLOR_GOAL_POLE, Vec2::new(4.0, TILE * 1.8)),
                        Transform::from_xyz(center.x, center.y, 1.0),
                        Collider::new(TILE * 0.8, TILE * 1.8),
                        Goal,
                        LevelEntity,
                        // `children!` is Bevy's macro for spawning a child inline. The
                        // child's `Transform` is relative to its parent's.
                        children![(
                            Sprite::from_color(COLOR_GOAL_FLAG, Vec2::new(TILE * 0.7, TILE * 0.5)),
                            Transform::from_xyz(TILE * 0.35 + 2.0, TILE * 0.6, 0.0),
                        )],
                    ));
                }

                other => panic!("unknown level character {other:?} at row {row}, column {column}"),
            }
        }
    }

    assert!(
        player_spawned,
        "LEVEL must contain a 'P' for the spawn point"
    );

    for spec in MOVING_PLATFORMS {
        let start = tile_center(spec.from.0, spec.from.1);
        let end = tile_center(spec.to.0, spec.to.1);
        let size = Vec2::new(TILE * spec.width_tiles, TILE * 0.5);

        commands.spawn((
            Sprite::from_color(COLOR_MOVING, size),
            Transform::from_xyz(start.x, start.y, 0.0),
            Collider::new(size.x, size.y),
            Solid,
            MovingPlatform::new(start, end, spec.speed),
            LevelEntity,
        ));
    }

    info!(
        "level built: {}x{} tiles, {} coins",
        columns, rows, progress.total_coins
    );
}

/// Convert a (column, row) in [`LEVEL`] to the world position of that tile's center.
///
/// The `LEVEL.len() - 1 - row` is the top-to-bottom flip; the `+ TILE * 0.5` is because
/// a Bevy `Sprite` is centered on its `Transform`, not anchored at a corner.
pub fn tile_center(column: f32, row: f32) -> Vec2 {
    Vec2::new(
        column * TILE + TILE * 0.5,
        (LEVEL.len() as f32 - 1.0 - row) * TILE + TILE * 0.5,
    )
}
