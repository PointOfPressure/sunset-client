// Register as a child of game/remote.rs: native dry BlockItem.place preview.
// No world prediction, inventory mutation, packet send or guessed hand.
use crate::game::sunset_util as util;
use crate::game::{Game, Hit};
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BotClick {
    pub clicked: [i32; 3],
    pub normal: [i32; 3],
    pub point: [f64; 3],
}
pub(crate) fn face(click: BotClick) -> i32 {
    i32::from(super::face_direction(click.normal))
}
fn preview(game: &mut Game<'_>, desired: [i32; 3], slot: usize, click: BotClick) -> bool {
    let stack = if slot == util::OFFHAND {
        game.offhand.as_ref()
    } else if slot < 9 {
        game.inventory.get(slot).and_then(|s| s.as_ref())
    } else {
        None
    };
    let Some(stack) = stack
        .filter(|s| {
            s.count > 0
                && game
                    .items()
                    .get(&s.item)
                    .and_then(|i| i.block)
                    .is_some_and(|id| {
                        let f = game.res.facts().get(id);
                        f.full_collision && !f.has_class("FallingBlock") && !f.has_class("TntBlock")
                    })
        })
        .cloned()
    else {
        return false;
    };
    game.remote_can_place(
        &Hit {
            pos: click.clicked,
            face: click.normal,
            point: click.point,
        },
        &stack,
    )
    .is_some_and(|(pos, changes)| {
        [pos.x, pos.y, pos.z] == desired
            && changes.iter().any(|(p, id)| {
                [p.x, p.y, p.z] == desired && &*game.res.facts().get(*id).item == &*stack.item
            })
    })
}
fn inert(game: &Game<'_>, cell: [i32; 3]) -> bool {
    let f = util::facts(game, cell);
    // Deliberately narrow inert full cubes: a preview is not permission to
    // accidentally open a menu or charge/detonate the clicked support.
    f.full_collision
        && !f.block_entity
        && f.fluid == crate::game::Fluid::None
        && !f.waterlogged
        && f.fluid_level == 0
        && matches!(
            &*f.name,
            "minecraft:stone"
                | "minecraft:cobblestone"
                | "minecraft:obsidian"
                | "minecraft:crying_obsidian"
                | "minecraft:bedrock"
                | "minecraft:deepslate"
                | "minecraft:cobbled_deepslate"
                | "minecraft:dirt"
                | "minecraft:grass_block"
                | "minecraft:netherrack"
                | "minecraft:end_stone"
                | "minecraft:netherite_block"
        )
}
fn ray(game: &Game<'_>, click: BotClick, range: f64, rays: &mut usize) -> bool {
    let Some(eye) = game.eye_position() else {
        return false;
    };
    if !range.is_finite()
        || range <= 0.0
        || *rays == 0
        || !eye.iter().chain(click.point.iter()).all(|v| v.is_finite())
    {
        return false;
    }
    let distance = (0..3)
        .map(|i| (click.point[i] - eye[i]).powi(2))
        .sum::<f64>()
        .sqrt();
    if distance > range.min(game.reach()) {
        return false;
    }
    // Continue along the eye-to-face ray just beyond its surface. Moving the
    // endpoint along the face normal changes the intersection at shallow
    // angles and would make the recorded click disagree with the native hit.
    let delta: [f64; 3] = std::array::from_fn(|i| click.point[i] - eye[i]);
    let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !length.is_finite() || length <= 1.0e-8 {
        return false;
    }
    *rays -= 1;
    game.clip_ray(eye, delta.map(|v| v / length), length + 0.025)
        .is_some_and(|hit| {
            hit.pos == click.clicked
                && hit.face == click.normal
                && (0..3).all(|i| (hit.point[i] - click.point[i]).abs() < 1.0e-5)
        })
}
pub(crate) fn valid(
    game: &mut Game<'_>,
    desired: [i32; 3],
    slot: usize,
    click: BotClick,
    range: f64,
    rays: &mut usize,
) -> bool {
    desired == std::array::from_fn(|i| click.clicked[i].saturating_add(click.normal[i]))
        && click.normal.iter().all(|v| (-1..=1).contains(v))
        && click.normal.iter().map(|v| v.abs()).sum::<i32>() == 1
        && game.world.loaded(click.clicked[0], click.clicked[2])
        && inert(game, click.clicked)
        && ray(game, click, range, rays)
        && preview(game, desired, slot, click)
}
pub(crate) fn plan(
    game: &mut Game<'_>,
    desired: [i32; 3],
    slot: usize,
    range: f64,
    rays: &mut usize,
) -> Option<BotClick> {
    for direction in [
        [0, -1, 0],
        [0, 1, 0],
        [0, 0, -1],
        [0, 0, 1],
        [-1, 0, 0],
        [1, 0, 0],
    ] {
        let clicked = std::array::from_fn(|i| desired[i].saturating_add(direction[i]));
        if !game.world.loaded(clicked[0], clicked[2]) || !inert(game, clicked) {
            continue;
        }
        let normal = direction.map(|v| -v);
        for bounds in game.outline_boxes(clicked).into_iter().take(8) {
            let point = std::array::from_fn(|i| {
                if normal[i] > 0 {
                    bounds[i + 3]
                } else if normal[i] < 0 {
                    bounds[i]
                } else {
                    (bounds[i] + bounds[i + 3]) * 0.5
                }
            });
            let click = BotClick {
                clicked,
                normal,
                point,
            };
            if valid(game, desired, slot, click, range, rays) {
                return Some(click);
            }
            if *rays == 0 {
                return None;
            }
        }
    }
    None
}

pub(crate) fn uuid(game: &Game<'_>, id: u32) -> Option<u128> {
    game.remote
        .as_ref()?
        .level_players
        .get(&i32::try_from(id).ok()?)
        .copied()
}
pub(crate) fn server_position(game: &Game<'_>, id: u32) -> Option<[f64; 3]> {
    let p = game
        .remote
        .as_ref()?
        .entities
        .get(&i32::try_from(id).ok()?)?
        .position;
    p.iter().all(|v| v.is_finite()).then_some(p)
}
