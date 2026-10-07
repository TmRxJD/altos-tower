#![recursion_limit = "256"]
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
const TAU: f64 = std::f64::consts::TAU;
#[cfg(test)]
mod course_playtests;
mod progression;
mod reference;
mod shop;
#[cfg(test)]
mod shop_tests;
use reference::{air_step, PROFILES, WORLD_SCALE};
const GRAVITY: f64 = reference::SOURCE_GRAVITY * WORLD_SCALE;
// User-facing calibration, separate from recovered serialized character factors.
const FLIP_RATE_SCALE: f64 = 0.65;
const CHASE_INTERVAL: f64 = 2500. * WORLD_SCALE;
#[derive(Clone, Default, Serialize)]
struct RopeState {
    load: f64,
    contact_time: f64,
    broken_at: Option<f64>,
    ridden_to: f64,
}
// Explicit finite window for this game. Captured successful jumps reach .839s;
// the original's exact cutoff has not been verified.
const DOUBLE_JUMP_WINDOW: f64 = 1.;
// Normalized horizontal coordinates and world-space vertical coordinates.
// The renderer receives this exact outline; collision never uses an invisible box.
const ROCK_OUTLINE: [[f64; 2]; 5] = [
    [-1., 22.],
    [-0.8, -9.],
    [-0.1, -22.],
    [0.7, -16.],
    [1., 22.],
];
fn rock_contact(f: &Feature, x: f64, y: f64) -> bool {
    let p = (x - f.x, y - f.y);
    let mut inside = true;
    for i in 0..ROCK_OUTLINE.len() {
        let a = ROCK_OUTLINE[i];
        let b = ROCK_OUTLINE[(i + 1) % ROCK_OUTLINE.len()];
        let a = (a[0] * f.width / 2., a[1]);
        let b = (b[0] * f.width / 2., b[1]);
        let d = (b.0 - a.0, b.1 - a.1);
        let q = (p.0 - a.0, p.1 - a.1);
        inside &= d.0 * q.1 - d.1 * q.0 >= 0.;
        let t = ((q.0 * d.0 + q.1 * d.1) / (d.0 * d.0 + d.1 * d.1)).clamp(0., 1.);
        if (q.0 - t * d.0).hypot(q.1 - t * d.1) < 6. {
            return true;
        }
    }
    inside
}
// Sweep the board against only the two upward-facing outline edges. Side and
// head contacts retain their usual collision behavior.
fn rock_top_crossing(f: &Feature, old: (f64, f64), new: (f64, f64)) -> Option<f64> {
    let d = (new.0 - old.0, new.1 - old.1);
    for i in 1..3 {
        let a = (
            f.x + ROCK_OUTLINE[i][0] * f.width / 2.,
            f.y + ROCK_OUTLINE[i][1],
        );
        let b = (
            f.x + ROCK_OUTLINE[i + 1][0] * f.width / 2.,
            f.y + ROCK_OUTLINE[i + 1][1],
        );
        let e = (b.0 - a.0, b.1 - a.1);
        let cross = d.0 * e.1 - d.1 * e.0;
        if cross >= -1e-9 {
            continue;
        }
        let q = (a.0 - old.0, a.1 - old.1);
        let t = (q.0 * e.1 - q.1 * e.0) / cross;
        let u = (q.0 * d.1 - q.1 * d.0) / cross;
        if (0. ..=1.).contains(&t) && (0. ..=1.).contains(&u) {
            return Some(old.1 + d.1 * t);
        }
    }
    None
}
fn wall_contains(f: &Feature, x: f64, y: f64) -> bool {
    f.active && (x - f.x).abs() < f.width / 2. && y > f.y2 && y < f.y
}
fn wall_up_curve(distance: f64) -> f64 {
    let x = (distance / (60. * WORLD_SCALE)).clamp(0., 1.);
    if x <= 0.2 {
        return 1.;
    }
    let t = (x - 0.2) / 0.8;
    // Recovered Hermite end tangent is in the original curve's x units.
    ((2. * t.powi(3) - 3. * t.powi(2) + 1.) + (t.powi(3) - t.powi(2)) * (-4.12751293 * 0.8))
        .clamp(0., 1.)
}

#[derive(Clone, Serialize)]
struct Trick {
    name: &'static str,
    count: u32,
    points: u32,
}
#[derive(Clone, Serialize)]
struct Chase {
    started_x: f64,
    x: f64,
    y: f64,
    previous_x: f64,
    previous_y: f64,
    speed: f64,
}
fn scarf_bank_levels(tricks: &[Trick]) -> f64 {
    tricks
        .iter()
        .map(|t| {
            let weight = match t.name {
                "Long grind boost" | "Long wallride boost" => 0.,
                "Double backflip" => 2.,
                "Triple backflip" => 3.,
                "Quadruple backflip" => 4.,
                // Distance increments are one trick, rather than one boost per coin-sized step.
                "Proximity flight" => return 1.,
                _ => 1.,
            };
            weight * t.count as f64
        })
        .sum()
}
fn push_trick(tricks: &mut Vec<Trick>, name: &'static str, count: u32, points: u32) {
    if let Some(last) = tricks.last_mut().filter(|t| t.name == name) {
        last.count += count;
        last.points += points;
    } else {
        tricks.push(Trick {
            name,
            count,
            points,
        });
    }
}
fn ground_speed(speed: f64, slope: f64, dt: f64, rider: usize, boost: f64) -> f64 {
    let p = PROFILES[rider];
    let base = p.speed_min * WORLD_SCALE;
    let target = (base * (1. + slope.clamp(-0.15, 0.65) * 0.6 * p.momentum)
        + (p.speed_max - p.speed_min)
            * WORLD_SCALE
            * 0.4
            * (boost / p.boost_max).clamp(0., 1.)
            * (1. + p.boost_offset))
        .clamp(base * 0.85, p.speed_max * WORLD_SCALE);
    (speed + (target - speed) * (1. - (-p.acceleration * dt).exp()))
        .clamp(0., p.speed_max * WORLD_SCALE)
}
fn ice_speed(speed: f64, rider: usize, dt: f64) -> f64 {
    let target = PROFILES[rider].speed_min * WORLD_SCALE * 1.399999976;
    speed + (target - speed).max(0.) * (1. - (-3. * dt).exp())
}
#[cfg(test)]
mod baseline_tests {
    use super::*;
    fn game(seed: u32) -> Game {
        let mut g = Game::new("{}");
        g.start_seeded(false, seed);
        g
    }
    fn advance(g: &mut Game, seconds: f64) {
        for _ in 0..(seconds * 120.) as usize {
            g.tick(1. / 120.);
        }
    }
    fn fixture(g: &mut Game, kind: &'static str, x: f64, y: f64, width: f64) {
        g.features.push(Feature {
            appearance: None,
            id: 9001 + g.features.len() as u32,
            kind,
            x,
            y,
            y2: y,
            width,
            sag: 0.,
            variant: 0,
            anchored: false,
            magnetized: false,
            offset: 0.,
            offset2: 0.,
            active: true,
        });
    }
    #[test]
    fn airborne_release_eases_toward_velocity_not_nearby_rail() {
        let mut g = flat_game(0);
        g.grounded = false;
        g.y = -1500.;
        g.angle = -2.4;
        fixture(&mut g, "rail", 200., -100., 2000.);
        g.release();
        advance(&mut g, 0.3);
        assert!(g.angle > -2.4 && g.angle < -1.9, "angle={}", g.angle);
        assert_eq!(g.awarded_flips, 0);
    }
    #[test]
    fn jumping_preserves_forward_velocity_for_every_rider() {
        for rider in 0..6 {
            let mut g = flat_game(rider);
            let vx = g.vx;
            g.press();
            assert_eq!(g.vx, vx);
            g.release();
            advance(&mut g, 0.2);
            assert!((g.vx - vx).abs() < 1e-8);
        }
    }
    #[test]
    fn rock_shape_does_not_hit_a_rider_above_its_visible_top() {
        let mut g = flat_game(0);
        fixture(&mut g, "rock", 100., -22., 50.);
        let rock = &g.features[0];
        assert!(!rock_contact(rock, 100., -54.));
        assert!(rock_contact(rock, 100., -38.));
        assert!(!rock_contact(rock, 140., -22.));
    }
    #[test]
    fn generated_cables_clear_ground_rocks_and_grinding_never_hits_them() {
        let mut tested = 0;
        for seed in 1..=256 {
            let mut g = game(seed);
            let Some(rail) = g.features.iter().find(|f| f.kind == "rail").cloned() else {
                continue;
            };
            tested += 1;
            for i in 0..=200 {
                let x = rail.x - rail.width / 2. + rail.width * i as f64 / 200.;
                assert!(Game::rail_line(&rail, x).0 <= g.course.sample(x).0 - 72.);
            }
            g.x = rail.x - rail.width / 2.;
            g.y = Game::rail_line(&rail, g.x).0 - 18.;
            g.grounded = false;
            g.rail = Some(rail.id);
            while g.rail.is_some() && g.alive {
                g.tick(1. / 120.);
            }
            assert!(g.alive, "seed={seed} notice={}", g.notice);
            if tested == 24 {
                break;
            }
        }
        assert_eq!(
            tested, 24,
            "not enough independently generated cables tested"
        );
    }
    #[test]
    fn classic_world_contains_only_core_encounters_and_is_stationary() {
        let allowed = [
            "coin", "rock", "rail", "magnet", "feather", "shield", "ice", "pine", "hut", "ruin",
            "wall", "balloon", "post",
        ];
        for seed in 1..=24 {
            let g = game(seed);
            assert!(g.features.iter().all(|f| allowed.contains(&f.kind)));
            for x in (0..3500).step_by(37) {
                assert_eq!(
                    g.course.sample_at(x as f64, 0.),
                    g.course.sample_at(x as f64, 40.)
                );
            }
        }
    }
    #[test]
    fn seeds_replay_and_storage_stays_bounded() {
        let a = game(5);
        let b = game(5);
        let c = game(6);
        assert_eq!(a.snapshot(), b.snapshot());
        assert_ne!(a.snapshot(), c.snapshot());
        let mut g = game(4);
        for x in (0..200000).step_by(2000) {
            g.course.extend(x as f64, &mut g.features);
            assert!(g
                .course
                .segments
                .iter()
                .all(|s| s.x1 > x as f64 - course::KEEP_BEHIND));
            assert!(g.course.segments.len() <= course::STORAGE_SEGMENT_LIMIT);
            assert!(g.course.end >= x as f64 + course::LOOK_AHEAD);
        }
        assert!(
            g.features.len() < 180,
            "count {} coins {} end {}",
            g.features.len(),
            g.features.iter().filter(|f| f.kind == "coin").count(),
            g.course.end
        );
    }
    #[test]
    fn terrain_is_c1_at_every_segment_boundary() {
        let g = game(17);
        for pair in g.course.segments.windows(2) {
            let a = pair[0].sample(pair[0].x1);
            let b = pair[1].sample(pair[1].x0);
            assert!((a.0 - b.0).abs() < 1e-7);
            assert!((a.1 - b.1).abs() < 1e-7);
        }
    }
    #[test]
    fn render_pose_spans_the_full_tick_including_internal_substeps() {
        let mut g = flat_game(0);
        for dt in [1. / 120., 1. / 60., 0.05, 0.002] {
            let before = (g.x, g.y, g.angle, g.time, g.terrain_time);
            g.tick(dt);
            assert_eq!(g.previous.x, before.0);
            assert_eq!(g.previous.y, before.1);
            assert_eq!(g.previous.angle, before.2);
            assert_eq!(g.previous.time, before.3);
            assert_eq!(g.previous.terrain_time, before.4);
            assert!((g.time - before.3 - dt).abs() < 1e-10);
        }
    }
    #[test]
    fn tap_jumps_and_lands_without_inventing_a_flip() {
        for seed in [1, 42, 43, 170] {
            let mut g = game(seed);
            g.press();
            g.release();
            for _ in 0..240 {
                g.tick(1. / 120.);
                if g.grounded || g.rail.is_some() || !g.alive {
                    break;
                }
            }
            assert!(
                g.alive && (g.grounded || g.rail.is_some()),
                "seed={seed}: {}",
                g.notice
            );
            assert_eq!(g.progress.flips, 0);
        }
    }
    #[test]
    fn release_gently_corrects_without_inventing_a_flip() {
        let mut g = game(1);
        g.press();
        advance(&mut g, 0.5);
        assert!(g.rotation < std::f64::consts::PI);
        let angle = g.angle;
        g.release();
        advance(&mut g, 0.15);
        assert!((g.angle - angle).abs() < 0.25);
        assert_ne!(g.angle, angle);
        assert_eq!(g.awarded_flips, 0);
    }
    #[test]
    fn captured_coin_catches_up_from_behind_after_magnet_expires() {
        let mut g = flat_game(0);
        g.speed = 2000.;
        g.vx = 2000.;
        g.magnet = 0.02;
        let y = g.y;
        fixture(&mut g, "coin", -120., y, 18.);
        advance(&mut g, 0.3);
        assert!(g.alive && g.coins == 1 && g.magnet == 0.);
    }
    #[test]
    fn active_boost_smashes_even_when_speed_is_below_legacy_threshold() {
        let mut g = flat_game(0);
        g.speed = 200.;
        g.vx = 200.;
        g.boost = 1.;
        fixture(&mut g, "rock", 8., -22., 50.);
        advance(&mut g, 0.02);
        assert!(g.alive && !g.features[0].active);
        assert!(g.banked_tricks.iter().any(|t| t.name == "Rock smash"));
    }
    #[test]
    fn descending_board_bounces_from_visible_rock_top_and_keeps_combo() {
        let mut g = flat_game(0);
        g.grounded = false;
        g.x = 170.;
        g.y = -90.;
        g.vy = 1000.;
        g.angle = 0.;
        g.combo = 1;
        g.combo_points = 10;
        fixture(&mut g, "rock", 200., -22., 80.);
        advance(&mut g, 0.06);
        assert!(g.alive && !g.grounded && g.vy < 0.);
        assert_eq!(g.combo_points, 90);
        assert!(g.tricks.iter().any(|t| t.name == "Rock bounce"));
        assert_eq!(g.score, 0);
    }
    #[test]
    fn swept_rock_contact_rejects_side_and_ascending_crossings() {
        let mut g = flat_game(0);
        fixture(&mut g, "rock", 200., -22., 80.);
        let f = &g.features[0];
        assert!(rock_top_crossing(f, (200., -80.), (200., -20.)).is_some());
        assert!(rock_top_crossing(f, (200., -20.), (200., -80.)).is_none());
        assert!(rock_top_crossing(f, (140., -20.), (180., -20.)).is_none());
    }
    #[test]
    fn generated_wall_is_reachable_and_held_contact_gains_elevation() {
        let mut g = (1..=128)
            .map(game)
            .find(|g| g.features.iter().any(|f| f.kind == "wall"))
            .unwrap();
        let wall = g
            .features
            .iter()
            .find(|f| f.kind == "wall")
            .unwrap()
            .clone();
        g.x = wall.x - wall.width / 2. - 100.;
        g.y = g.course.sample(g.x).0 - 18.;
        g.vx = 800.;
        let entry_y = g.y;
        g.press();
        advance(&mut g, 0.8);
        assert!(g.alive && g.wall == Some(wall.id), "{}", g.notice);
        assert!(g.y < entry_y - 200., "entry={entry_y}, y={}", g.y);
    }
    #[test]
    fn wall_entry_and_retention_share_exact_horizontal_bounds() {
        let mut g = flat_game(0);
        fixture(&mut g, "wall", 500., 100., 1000.);
        g.features[0].y2 = -1500.;
        assert!(!wall_contains(&g.features[0], -1., -100.));
        assert!(wall_contains(&g.features[0], 1., -100.));
        assert_eq!(wall_up_curve(0.), 1.);
        assert_eq!(wall_up_curve(12. * WORLD_SCALE), 1.);
        assert_eq!(wall_up_curve(60. * WORLD_SCALE), 0.);
    }
    #[test]
    fn release_correction_never_completes_a_nearly_finished_flip() {
        let mut g = flat_game(4);
        g.grounded = false;
        g.y = -5000.;
        g.angle = 0.5;
        g.rotation = TAU - 0.1;
        let rotation = g.rotation;
        g.release();
        advance(&mut g, 0.5);
        assert_eq!(g.rotation, rotation);
        assert_eq!(g.awarded_flips, 0);
    }
    #[test]
    fn uncollected_coin_guides_do_not_retarget_to_rider_or_boost() {
        let mut a = game(2);
        let mut b = a.clone();
        b.rider = 4;
        b.speed = 1800.;
        b.vx = 1800.;
        b.boost = 3.;
        for g in [&mut a, &mut b] {
            g.grounded = false;
            g.y = -5000.;
        }
        advance(&mut a, 0.2);
        advance(&mut b, 0.2);
        for f in a.features.iter().filter(|f| f.kind == "coin") {
            let other = b.features.iter().find(|other| other.id == f.id).unwrap();
            assert_eq!((f.x, f.y), (other.x, other.y));
        }
    }
    #[test]
    fn neighboring_seeds_choose_multiple_safe_opening_routes() {
        let mut routes = std::collections::BTreeSet::new();
        for seed in 1..=32 {
            let g = game(seed);
            routes.insert(g.course.encounters[0].1);
            assert!(g.course.gaps.iter().all(|&(left, _)| left > 7500.));
            assert!(g
                .features
                .iter()
                .filter(|f| f.kind == "rock")
                .all(|f| f.x > 1000.));
            assert_eq!(g.previous.y, g.y);
            assert_eq!(g.previous.angle, g.angle);
        }
        assert_eq!(routes.len(), 4);
    }
    #[test]
    fn wing_forward_release_stops_turning_on_next_controller_step() {
        let mut g = flat_game(0);
        g.grounded = false;
        g.y = -10000.;
        g.wing = 6.;
        g.wingsuit();
        g.held = true;
        g.angular = -250_f64.to_radians();
        let before = g.angular;
        g.release();
        assert_eq!(g.angular, before);
        advance(&mut g, 0.1);
        assert_eq!(g.angular, 0.);
        assert!(g.vx > 0.);
    }
    #[test]
    fn held_jump_survives_wingsuit_deactivation_and_keeps_flipping() {
        let mut g = flat_game(5);
        g.grounded = false;
        g.y = -10000.;
        g.wing = 6.;
        g.wingsuit();
        g.press();
        g.wingsuit();
        let angle = g.angle;
        advance(&mut g, 0.15);
        assert!(g.held && !g.wing_on && g.angle < angle - 0.2);
    }
    #[test]
    fn inverted_rock_bounce_is_safe_and_does_not_recollide() {
        let mut g = flat_game(0);
        g.grounded = false;
        g.x = 170.;
        g.y = -90.;
        g.vy = 1000.;
        g.angle = std::f64::consts::PI;
        fixture(&mut g, "rock", 200., -22., 80.);
        advance(&mut g, 0.12);
        assert!(g.alive && g.vy < 0. && g.tricks.iter().any(|t| t.name == "Rock bounce"));
        assert_eq!(g.combo_points, 80);
    }
    #[test]
    fn flip_upgrade_preserves_intervening_bounce_and_complete_charge_history() {
        let mut g = flat_game(4);
        g.grounded = false;
        g.y = -100000.;
        g.held = true;
        while g.awarded_flips == 0 {
            g.tick(1. / 120.);
        }
        push_trick(&mut g.tricks, "Rock bounce", 1, 80);
        while g.awarded_flips < 2 {
            g.tick(1. / 120.);
        }
        assert_eq!(g.tricks[0].name, "Double backflip");
        assert_eq!(g.tricks[0].points, 60);
        assert_eq!(g.tricks[1].name, "Rock bounce");
        assert_eq!(g.tricks[1].points, 80);
        for i in 0..12 {
            push_trick(
                &mut g.tricks,
                if i % 2 == 0 {
                    "Wingsuit"
                } else {
                    "Bunting grind"
                },
                1,
                10,
            );
        }
        assert_eq!(g.tricks.len(), 14);
        assert_eq!(scarf_bank_levels(&g.tricks), 15.);
    }
    #[test]
    fn pursuit_uses_2500_meter_milestones_and_closes_on_unboosted_rider() {
        let mut g = flat_game(0);
        assert_eq!(g.next_chase, CHASE_INTERVAL);
        g.x = CHASE_INTERVAL - 100.;
        advance(&mut g, 0.2);
        assert!(g.chase.is_some());
        assert_eq!(g.next_chase, CHASE_INTERVAL * 2.);
        let gap = g.x - g.chase.as_ref().unwrap().x;
        advance(&mut g, 0.5);
        assert!(g.x - g.chase.as_ref().unwrap().x < gap);
    }
    #[test]
    fn double_backflip_is_one_combo_part_and_banks_sixty_with_two_boost_levels() {
        let mut g = flat_game(4);
        g.grounded = false;
        g.y = -10000.;
        g.held = true;
        for _ in 0..600 {
            g.tick(1. / 120.);
            if g.awarded_flips == 2 {
                break;
            }
        }
        assert_eq!(g.awarded_flips, 2);
        assert_eq!(g.combo, 1);
        assert_eq!(g.combo_points, 60);
        assert_eq!(g.tricks[0].name, "Double backflip");
        g.release();
        g.bank();
        assert_eq!(g.score, 60);
        assert_eq!(g.wing, 1.);
    }
    #[test]
    fn crossing_a_camp_wakes_a_visible_pursuer() {
        let mut g = flat_game(0);
        g.next_chase = 50.;
        advance(&mut g, 0.1);
        let chase = g.chase.as_ref().unwrap();
        assert!(chase.x < g.x && g.notice.contains("Elder chase"));
        let view: serde_json::Value = serde_json::from_str(&g.snapshot()).unwrap();
        assert!(view["chase"]["x"].is_number());
    }
    #[test]
    fn crossing_a_chasm_escapes_pursuit_without_banking_the_combo() {
        let mut g = flat_game(0);
        g.course.gaps = vec![(1000., 1500.)];
        g.x = 1490.;
        g.y = -500.;
        g.grounded = false;
        g.vx = 1800.;
        g.combo = 1;
        g.combo_points = 10;
        g.chase = Some(Chase {
            started_x: 0.,
            x: 900.,
            y: -18.,
            previous_x: 900.,
            previous_y: -18.,
            speed: 848.,
        });
        advance(&mut g, 0.02);
        assert!(g.alive && g.chase.is_none() && g.score == 800 && g.combo == 1);
        assert!(g.notice.contains("Elder escaped"));
    }
    #[test]
    fn pursuer_catches_a_rider_who_does_not_maintain_a_lead() {
        let mut g = flat_game(0);
        g.speed = 100.;
        g.vx = 100.;
        g.chase = Some(Chase {
            started_x: 0.,
            x: -30.,
            y: -18.,
            previous_x: -30.,
            previous_y: -18.,
            speed: 900.,
        });
        advance(&mut g, 0.1);
        assert!(!g.alive && g.notice == "Caught by the elder");
    }
    #[test]
    fn a_gap_already_under_the_player_when_the_elder_wakes_is_not_an_escape() {
        let mut g = flat_game(0);
        g.course.gaps = vec![(1000., 1500.)];
        g.x = 1490.;
        g.y = -500.;
        g.grounded = false;
        g.vx = 1800.;
        g.chase = Some(Chase {
            started_x: 1400.,
            x: 900.,
            y: -18.,
            previous_x: 900.,
            previous_y: -18.,
            speed: 848.,
        });
        advance(&mut g, 0.02);
        assert!(g.alive && g.chase.is_some() && g.score == 0);
    }
    #[test]
    fn distance_and_elapsed_time_cannot_end_an_elder_chase_on_solid_ground() {
        let mut g = flat_game(0);
        g.x = 2000.;
        g.y = -500.;
        g.grounded = false;
        g.vx = 1800.;
        g.chase = Some(Chase {
            started_x: 0.,
            x: -10000.,
            y: -18.,
            previous_x: -10000.,
            previous_y: -18.,
            speed: 848.,
        });
        advance(&mut g, 0.5);
        assert!(g.alive && g.chase.is_some() && g.score == 0);
    }
    #[test]
    fn regularly_landed_single_tricks_eventually_make_wingsuit_ready() {
        let mut g = flat_game(0);
        let mut banks = 0;
        for _ in 0..12 {
            g.combo = 1;
            g.combo_points = 10;
            push_trick(&mut g.tricks, "Backflip", 1, 10);
            g.bank();
            banks += 1;
            if g.wing_charged {
                break;
            }
            g.x = 0.;
            advance(&mut g, 2.5);
            assert!(g.alive);
        }
        assert!(
            g.wing_charged && (10..=12).contains(&banks),
            "banks={banks}, wing={}",
            g.wing
        );
    }
    #[test]
    fn long_contact_boosts_are_not_credited_twice_at_the_bank() {
        let mut g = flat_game(0);
        g.wing = 2.;
        g.combo = 2;
        g.combo_points = 310;
        push_trick(&mut g.tricks, "Bunting grind", 1, 10);
        push_trick(&mut g.tricks, "Long grind boost", 1, 300);
        g.bank();
        assert_eq!(g.wing, 2.5);
    }
    #[test]
    fn faster_rider_can_bank_a_real_drop_backflip_and_speed_boost() {
        let mut g = flat_game(1);
        let initial_flips = g.progress.flips;
        g.press();
        g.y -= 160.;
        for _ in 0..200 {
            g.tick(1. / 120.);
            if g.rotation > TAU - 0.02 {
                break;
            }
        }
        assert!(!g.grounded);
        g.release();
        advance(&mut g, 0.8);
        assert!(g.alive && g.grounded);
        assert_eq!(g.progress.flips, initial_flips + 1);
        assert!(g.boost > 0.);
    }
    #[test]
    fn deliberately_late_held_flip_still_crashes() {
        let mut g = game(42);
        g.press();
        g.release();
        advance(&mut g, 0.7);
        g.press();
        advance(&mut g, 1.2);
        assert!(!g.alive);
        assert!(g.notice.contains("Unfinished"));
    }
    #[test]
    fn each_pickup_benefits_without_forced_launch_or_rotation() {
        for kind in ["magnet", "feather", "shield"] {
            let mut g = game(1);
            g.features.clear();
            fixture(&mut g, kind, 5., -35., 30.);
            g.tick(1. / 120.);
            assert!(g.alive && g.grounded);
            assert!(g.vy >= 0.);
            assert_eq!(g.rotation, 0.);
            assert!(match kind {
                "magnet" => g.magnet > 0.,
                "feather" => g.feather > 0.,
                _ => g.shield,
            });
        }
    }
    #[test]
    fn magnet_collects_coins_outside_normal_pickup_reach() {
        let mut g = game(1);
        g.features.clear();
        g.magnet = 2.;
        fixture(&mut g, "coin", 100., -100., 18.);
        advance(&mut g, 0.3);
        assert_eq!(g.coins, 1);
        assert_eq!(g.wing, 0., "coins are currency, not wingsuit fuel");
    }
    #[test]
    fn feather_hover_and_expiry_cannot_drop_player_into_a_rock() {
        let mut g = game(1);
        g.features.clear();
        g.feather = 0.03;
        let rock_y = g.course.sample(35.).0 - 22.;
        fixture(&mut g, "rock", 35., rock_y, 42.);
        advance(&mut g, 0.6);
        assert!(g.alive && g.grounded);
        assert_eq!(g.combo_points, 0);
        assert_eq!(g.progress.flips, 0);
        assert!(g.hover < 5.);
    }
    #[test]
    fn shield_consumes_on_bad_landing_and_rock_collision() {
        let mut g = game(1);
        g.shield = true;
        g.grounded = false;
        g.y = -19.;
        g.vy = 350.;
        g.angle = std::f64::consts::PI;
        g.tick(0.008);
        assert!(g.alive && g.grounded && !g.shield);
        assert_eq!(g.progress.flips, 0);
        let mut g = game(1);
        g.features.clear();
        g.shield = true;
        fixture(&mut g, "rock", 5., -22., 42.);
        g.tick(0.008);
        assert!(g.alive && !g.shield);
    }
    #[test]
    fn unprotected_inverted_ground_landing_remains_fatal() {
        let mut g = game(1);
        g.grounded = false;
        g.y = -19.;
        g.vy = 350.;
        g.angle = std::f64::consts::PI;
        g.tick(0.008);
        assert!(!g.alive);
    }
    #[test]
    fn wingsuit_requires_a_charged_scarf_and_expires_without_queued_jump() {
        let mut g = flat_game(0);
        g.grounded = false;
        g.y = -4000.;
        g.wing = 1.;
        g.wingsuit();
        assert!(!g.wing_on);
        g.wing = 6.;
        g.wingsuit();
        assert!(g.wing_on);
        g.wing = 0.301;
        g.tick(0.05);
        assert!(!g.wing_on && g.buffer == 0.);
    }
    #[test]
    fn flight_held_input_can_loop_and_release_does_not_snap_heading() {
        let mut g = flat_game(0);
        g.grounded = false;
        g.y = -5000.;
        g.vy = 0.;
        g.wing = 6.;
        g.wingsuit();
        g.press();
        advance(&mut g, 2.2);
        assert!(g.alive && g.tricks.iter().any(|t| t.name == "Wingsuit loop"));
        let angle = g.angle;
        g.release();
        g.tick(1. / 120.);
        assert!((g.angle - angle).abs() < 0.15);
        assert!(g.buffer == 0.);
    }
    #[test]
    fn inverted_rail_crossing_passes_through_but_upright_grinds_once() {
        for inverted in [false, true] {
            let mut g = game(1);
            g.features.clear();
            fixture(&mut g, "rail", 250., -50., 500.);
            g.grounded = false;
            g.y = -70.;
            g.vy = 200.;
            g.angle = if inverted { std::f64::consts::PI } else { 0. };
            advance(&mut g, 0.05);
            if inverted {
                assert!(g.rail.is_none());
            } else {
                assert!(g.rail.is_some());
                advance(&mut g, 0.5);
                assert_eq!(g.progress.grinds, 1);
                assert_eq!(g.wing, 0., "unbanked grind entry does not charge scarf");
            }
        }
    }
    #[test]
    fn actual_opening_jump_connects_to_long_grind() {
        let seeds: Vec<_> = (1..=32)
            .filter(|&seed| matches!(game(seed).course.encounters[0].1, 0 | 2))
            .take(8)
            .collect();
        assert_eq!(seeds.len(), 8);
        for seed in seeds {
            let mut g = game(seed);
            let launch = g
                .features
                .iter()
                .filter(|f| f.kind == "rail")
                .map(|f| f.x - f.width / 2. - 600.)
                .min_by(f64::total_cmp)
                .unwrap();
            while g.x < launch {
                g.tick(1. / 120.);
                assert!(g.alive);
            }
            g.press();
            g.release();
            for _ in 0..220 {
                g.tick(1. / 120.);
                if g.rail.is_some() {
                    break;
                }
            }
            assert!(
                g.alive && g.rail.is_some(),
                "seed {seed}: x={}, y={}",
                g.x,
                g.y
            );
            let id = g.rail.unwrap();
            let variant = g.features.iter().find(|f| f.id == id).unwrap().variant;
            advance(&mut g, 2.8);
            assert!(g.rail.is_some());
            assert_eq!(g.progress.grinds, 1);
            if variant == 0 {
                assert!(g.rope_states[&id].broken_at.is_none());
                advance(&mut g, 0.25);
                assert!(g.rope_states[&id].broken_at.is_some());
                assert!(!g.features.iter().find(|f| f.id == id).unwrap().active);
            }
        }
    }
    fn flat_game(rider: usize) -> Game {
        let mut g = game(1);
        g.rider = rider;
        g.course.segments = vec![course::Segment {
            x0: -1000.,
            x1: 20000.,
            y0: 0.,
            y1: 0.,
            m0: 0.,
            m1: 0.,
        }];
        g.course.end = 20000.;
        g.course.gaps.clear();
        g.features.clear();
        g.angle = 0.;
        g.y = -18.;
        g.vx = PROFILES[rider].speed_min * WORLD_SCALE;
        g.speed = g.vx;
        g
    }
    #[test]
    fn reference_profiles_match_curated_signed_apk_fields() {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../../reference/alto-adventure-1.8.24.json"))
                .unwrap();
        let names = ["Alto", "Maya", "Paz", "Izel", "Tupa", "Felipe"];
        for (i, name) in names.iter().enumerate() {
            let fields = &data["characters"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["name"] == *name)
                .unwrap()["fields"];
            assert!((PROFILES[i].jump - fields["jumpStrength"].as_f64().unwrap()).abs() < 1e-7);
            assert!(
                (PROFILES[i].gravity_scale - fields["gravityScale"].as_f64().unwrap()).abs() < 1e-7
            );
            assert!(
                (PROFILES[i].flip_factor - fields["backflipSpeed"].as_f64().unwrap()).abs() < 1e-7
            );
            assert_eq!(
                PROFILES[i].double_jump_enabled,
                fields["canDoubleJump"].as_u64().unwrap() == 1
            );
        }
    }
    #[test]
    fn airborne_integration_is_independent_of_step_partition() {
        let (nx, ny, dx, dy) = air_step(800., -640., 0, false, 1.234);
        let (mut vx, mut vy, mut x, mut y) = (800., -640., 0., 0.);
        for _ in 0..1234 {
            let (a, b, c, d) = air_step(vx, vy, 0, false, 0.001);
            vx = a;
            vy = b;
            x += c;
            y += d;
        }
        assert!((nx - vx).abs() < 1e-7 && (ny - vy).abs() < 1e-7);
        assert!((dx - x).abs() < 1e-7 && (dy - y).abs() < 1e-7);
    }
    #[test]
    fn flight_is_independent_of_render_frame_partition() {
        let mut a = flat_game(0);
        a.grounded = false;
        a.y = -5000.;
        a.wing = 6.;
        a.wingsuit();
        a.press();
        let mut b = a.clone();
        for _ in 0..120 {
            a.tick(1. / 120.);
        }
        for _ in 0..30 {
            b.tick(1. / 30.);
        }
        assert!(
            (a.x - b.x).abs() < 1e-6
                && (a.y - b.y).abs() < 1e-6
                && (a.angle - b.angle).abs() < 1e-6
        );
    }
    #[test]
    fn balloon_accepts_inverted_contact_and_preserves_pending_combo() {
        let mut g = flat_game(0);
        fixture(&mut g, "balloon", 100., -500., 640.);
        g.grounded = false;
        g.x = 100.;
        g.y = -530.;
        g.vx = 100.;
        g.vy = 500.;
        g.angle = std::f64::consts::PI;
        g.combo = 2;
        g.combo_points = 70;
        advance(&mut g, 0.08);
        assert!(g.alive && g.vy < 0. && g.combo == 3);
        assert!(g
            .tricks
            .iter()
            .any(|t| t.name == "Balloon bounce" && t.points == 60));
        assert_eq!(g.score, 0);
    }
    #[test]
    fn held_wall_contact_climbs_and_release_kicks_without_banking() {
        let mut g = flat_game(0);
        fixture(&mut g, "wall", 1000., -30., 2200.);
        g.features[0].y2 = -1600.;
        g.grounded = false;
        g.x = 0.;
        g.y = -80.;
        g.vy = -300.;
        g.press();
        advance(&mut g, 0.7);
        assert!(g.alive && g.wall.is_some() && g.y < -200.);
        let y = g.y;
        g.release();
        assert!(g.wall.is_none() && g.vy < 0. && g.combo >= 2 && g.score == 0);
        advance(&mut g, 0.05);
        assert!(g.y < y);
    }
    #[test]
    fn wall_balloon_rail_chain_keeps_score_pending_and_refreshes_scarf() {
        let mut g = flat_game(0);
        fixture(&mut g, "wall", 1000., -30., 2200.);
        g.features[0].y2 = -1600.;
        fixture(&mut g, "balloon", 1600., -500., 640.);
        fixture(&mut g, "rail", 4800., -100., 5600.);
        g.grounded = false;
        g.y = -80.;
        g.vy = -300.;
        g.wing = 6.;
        g.press();
        advance(&mut g, 0.7);
        g.release();
        for _ in 0..1000 {
            g.tick(1. / 120.);
            if g.tricks.iter().any(|t| t.name == "Long grind boost") {
                break;
            }
        }
        assert!(
            g.alive && g.rail.is_some(),
            "{} {} {} {:?}",
            g.x,
            g.y,
            g.notice,
            g.rail
        );
        for name in [
            "Wallride",
            "Wallride kicker",
            "Balloon bounce",
            "Bunting grind",
            "Long grind boost",
        ] {
            assert!(g.tricks.iter().any(|t| t.name == name), "missing {name}");
        }
        assert_eq!(g.score, 0);
        assert!(g.wing_charged);
        let combo = g.combo;
        g.press();
        g.wingsuit();
        assert!(g.wing_on && g.combo > combo && g.score == 0);
    }
    #[test]
    fn overlapping_wall_and_rail_never_share_contact_ownership() {
        let mut g = flat_game(0);
        fixture(&mut g, "wall", 500., -30., 1000.);
        g.features[0].y2 = -1000.;
        fixture(&mut g, "rail", 500., -142., 1000.);
        g.x = 100.;
        g.y = -160.;
        g.grounded = false;
        g.vy = 120.;
        g.held = true;
        g.wall = Some(g.features[0].id);
        g.tick(1. / 240.);
        assert!(g.wall.is_some() && g.rail.is_none());
    }
    #[test]
    fn returning_to_rewarded_rail_starts_a_new_distance_measurement() {
        let mut g = flat_game(0);
        fixture(&mut g, "rail", 500., -130., 1000.);
        g.rewarded_rails.push(g.features[0].id);
        g.x = 100.;
        g.y = -160.;
        g.grounded = false;
        g.vy = 1000.;
        g.contact_distance = 2399.;
        g.tick(0.02);
        assert!(g.rail.is_some() && g.contact_distance < 80.);
        assert_eq!(g.wing, 0.);
    }
    #[test]
    fn rising_balloon_catches_slower_ascending_rider_but_underside_passes() {
        let mut g = flat_game(0);
        fixture(&mut g, "balloon", 100., -500., 640.);
        g.features[0].id = 3;
        g.x = 100.;
        g.y = -518.;
        g.grounded = false;
        g.vy = -10.;
        g.vx = 0.;
        g.tick(1. / 240.);
        assert_eq!(g.progress.bounces, 1);
        assert!(g.vy < -500.);
        let mut g = flat_game(0);
        fixture(&mut g, "balloon", 100., -500., 640.);
        g.x = 100.;
        g.y = -100.;
        g.grounded = false;
        g.vy = -500.;
        g.vx = 0.;
        g.tick(0.05);
        assert_eq!(g.progress.bounces, 0);
    }
    #[test]
    fn tupa_double_jump_release_after_complete_rotation_lands_upright() {
        let mut g = game(1);
        g.rider = 5;
        g.press();
        advance(&mut g, 0.5);
        g.release();
        g.press();
        for _ in 0..300 {
            g.tick(1. / 120.);
            if g.rotation >= TAU - 0.02 {
                break;
            }
        }
        let angle = g.angle;
        g.release();
        advance(&mut g, 3.);
        assert!(
            g.alive,
            "award angle {angle}, landing angle {}, x {}: {}",
            g.angle, g.x, g.notice
        );
    }
    #[test]
    fn proximity_points_follow_distance_without_ground_contact() {
        let mut g = flat_game(0);
        g.grounded = false;
        g.y = -160.;
        g.wing = 6.;
        g.wingsuit();
        g.vy = 0.;
        advance(&mut g, 0.12);
        assert!(g.alive && !g.grounded && g.proximity);
        assert!(g
            .tricks
            .iter()
            .any(|t| t.name == "Proximity flight" && t.points >= 10));
        assert_eq!(g.score, 0);
    }
    #[test]
    fn alto_tap_matches_the_linear_drag_reconstruction_oracle() {
        let mut g = flat_game(0);
        g.press();
        g.release();
        let mut apex = 0_f64;
        while !g.grounded {
            g.tick(1. / 120.);
            apex = apex.max(-18. - g.y);
            assert!(g.time < 2.);
        }
        assert!((g.time - 1.167).abs() < 0.01, "airtime={}", g.time);
        assert!((apex - 169.47).abs() < 0.2, "apex={apex}");
        assert!(g.alive);
    }
    #[test]
    fn maya_full_flip_can_be_stopped_for_a_safe_drop_landing() {
        let mut g = flat_game(1);
        g.press();
        g.y -= 160.;
        while g.rotation < TAU - 0.02 && !g.grounded {
            g.tick(1. / 120.);
        }
        assert_eq!(g.awarded_flips, 1);
        assert!(!g.grounded);
        g.release();
        advance(&mut g, 0.8);
        assert!(g.alive && g.grounded);
        assert_eq!(g.progress.flips, 1);
        assert_eq!(g.banked_tricks[0].name, "Backflip");
    }
    #[test]
    fn felipe_double_jump_banks_on_changing_generated_slope() {
        let mut g = game(1);
        g.rider = 5;
        g.start_seeded(false, 1);
        g.press();
        advance(&mut g, 0.5);
        g.release();
        g.press();
        while g.rotation < TAU - 0.02 && g.alive && !g.grounded {
            g.tick(1. / 120.);
        }
        g.release();
        for _ in 0..1200 {
            g.tick(1. / 120.);
            if !g.alive || g.progress.flips > 0 {
                break;
            }
        }
        assert!(g.alive && g.grounded, "y={} angle={}", g.y, g.angle);
        assert_eq!(g.progress.flips, 1);
        assert!(g.boost > 0.);
        assert_eq!(g.banked_tricks[0].name, "Backflip");
    }
    #[test]
    fn held_rotation_is_slower_and_release_counts_the_native_inverted_gate() {
        let mut g = flat_game(5);
        g.y = -2000.;
        g.grounded = false;
        g.press();
        advance(&mut g, 0.8);
        assert!(
            g.rotation > 2. && g.rotation < 3.,
            "rotation={}",
            g.rotation
        );
        g.release();
        advance(&mut g, 0.6);
        assert!(g.angle < -std::f64::consts::PI);
        assert_eq!(g.awarded_flips, 1);
        assert_eq!(g.score, 0);
        let mut tap = flat_game(5);
        tap.press();
        advance(&mut tap, 0.15);
        tap.release();
        advance(&mut tap, 0.6);
        assert_eq!(tap.awarded_flips, 0);
        assert_eq!(tap.pending_board_flips, 0);
    }
    #[test]
    fn a_banked_flip_produces_immediate_velocity_and_travel_gain() {
        let mut coast = flat_game(0);
        coast.speed = 600.;
        coast.vx = 600.;
        let mut boosted = flat_game(0);
        boosted.speed = 600.;
        boosted.vx = 600.;
        boosted.combo = 1;
        boosted.combo_points = 100;
        push_trick(&mut boosted.tricks, "Backflip", 1, 100);
        boosted.bank();
        assert!(boosted.vx > 950.);
        advance(&mut coast, 0.5);
        advance(&mut boosted, 0.5);
        assert!(boosted.x - coast.x > 100., "gain={}", boosted.x - coast.x);
        assert_eq!(boosted.banked_score, 100);
        assert_eq!(boosted.banked_tricks[0].name, "Backflip");
    }
    #[test]
    fn boosted_rock_smash_and_ice_change_speed_without_launching() {
        let mut g = flat_game(0);
        g.speed = 1050.;
        g.vx = g.speed;
        g.boost = 2.;
        fixture(&mut g, "rock", 8., -22., 50.);
        g.tick(1. / 120.);
        assert!(g.alive && g.grounded);
        assert!(!g.features[0].active);
        assert_eq!(g.banked_tricks[0].name, "Rock smash");
        let mut ice = flat_game(0);
        let mut normal = ice.clone();
        fixture(&mut ice, "ice", 200., 0., 1000.);
        advance(&mut ice, 0.4);
        advance(&mut normal, 0.4);
        assert!(ice.grounded && ice.alive);
        assert!(ice.speed > normal.speed + 100.);
        assert_eq!(ice.rotation, 0.);
    }
    #[test]
    fn pickup_notices_do_not_erase_tricks_and_double_jump_is_silent() {
        let mut g = flat_game(5);
        g.combo = 1;
        g.combo_points = 100;
        push_trick(&mut g.tricks, "Backflip", 1, 100);
        fixture(&mut g, "magnet", 5., -35., 30.);
        g.tick(1. / 120.);
        assert!(g.magnet > 0. && g.tricks[0].name == "Backflip");
        g.press();
        g.release();
        advance(&mut g, 0.2);
        let notice = g.notice.clone();
        g.press();
        assert!(g.air_hop_used);
        assert_eq!(g.notice, notice);
    }
    #[test]
    fn normal_play_cannot_coast_through_the_opening_without_input() {
        for seed in [1, 42, 170, 777] {
            let mut g = game(seed);
            advance(&mut g, 12.);
            assert!(
                !g.alive
                    && g.features
                        .iter()
                        .any(|f| f.active && f.kind == "rock" && rock_contact(f, g.x, g.y + 12.)),
                "seed={seed}, x={}",
                g.x
            );
        }
    }
    #[test]
    fn weighted_encounters_recur_without_a_fixed_twelve_kind_cycle() {
        let mut g = game(42);
        let mut seen = std::collections::BTreeMap::new();
        let mut rock_count = 0;
        for x in (0..110000).step_by(1000) {
            g.course.extend(x as f64, &mut g.features);
            seen.extend(g.course.encounters.iter().map(|e| (e.0 as u32, e.1)));
            rock_count = rock_count.max(g.features.iter().filter(|f| f.kind == "rock").count());
        }
        let sequence: Vec<_> = seen.values().copied().collect();
        assert!(sequence.len() >= 12);
        assert!(sequence.windows(2).all(|p| p[0] != p[1]));
        assert!(
            sequence[..12]
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                < 12
        );
        assert!(
            rock_count >= 2,
            "generated rock encounters must contain real obstacles"
        );
    }
    #[test]
    fn generated_chasms_have_a_safe_single_jump_window_for_every_profile() {
        let mut base = game(42);
        let mut tested = std::collections::BTreeSet::new();
        let mut max_landing_time = 0_f64;
        let mut encounter_families = std::collections::BTreeMap::new();
        // Independent weighted selection has no guarantee of a chasm per bag.
        // Keep the same three-gap/six-profile safety coverage; search farther.
        for px in (0..1000000).step_by(4000) {
            base.course.extend(px as f64, &mut base.features);
            for &(start, kind) in &base.course.encounters {
                encounter_families.insert(start as u32, kind);
            }
            let gaps = base.course.gaps.clone();
            for (left, right) in gaps {
                if !tested.insert(left as u32) {
                    continue;
                }
                for (rider, profile) in PROFILES.iter().enumerate() {
                    for speed in [
                        profile.speed_min * WORLD_SCALE,
                        profile.speed_max * WORLD_SCALE,
                    ] {
                        let mut longest = 0_i32;
                        let mut streak = 0_i32;
                        for distance in (60..=1400).step_by(40) {
                            let mut g = base.clone();
                            g.x = left - distance as f64;
                            let (y, slope, _, _) = g.course.sample_at(g.x, 0.);
                            g.rider = rider;
                            g.y = y - 18.;
                            g.speed = speed;
                            g.vx = speed / (1. + slope * slope).sqrt();
                            g.vy = g.vx * slope;
                            g.angle = slope.atan();
                            g.time = 0.;
                            g.grounded = true;
                            g.alive = true;
                            g.next_chase = f64::INFINITY;
                            g.press();
                            g.release();
                            let mut safe = false;
                            while g.alive && g.time < 12. {
                                g.tick(1. / 120.);
                                if g.x > right + 40. && (g.grounded || g.rail.is_some()) {
                                    max_landing_time = max_landing_time.max(g.time);
                                    advance(&mut g, 0.25);
                                    safe = g.alive;
                                    break;
                                }
                                if g.grounded && g.x < right {
                                    break;
                                }
                            }
                            streak = if safe { streak + 1 } else { 0 };
                            longest = longest.max(streak);
                        }
                        let window = (longest - 1).max(0) as f64 * 40. / speed;
                        assert!(
                            window >= 0.25,
                            "gap={left}..{right} rider={rider} speed={speed} window={window} max_first_support={max_landing_time}"
                        );
                    }
                }
                if tested.len() == 3 {
                    eprintln!("validated three chasms, six profiles, min/max speeds: latest first landing {max_landing_time:.2}s");
                    return;
                }
            }
        }
        let mut counts = std::collections::BTreeMap::new();
        for kind in encounter_families.values() {
            *counts.entry(*kind).or_insert(0) += 1;
        }
        panic!(
            "not enough generated chasms tested: gaps={}, families={counts:?}",
            tested.len()
        );
    }
    #[test]
    fn later_rock_encounters_remain_available_across_multiple_regions() {
        for seed in [1, 42] {
            let mut g = game(seed);
            let mut rocks = std::collections::BTreeMap::new();
            for px in (0..400000).step_by(1000) {
                g.course.extend(px as f64, &mut g.features);
                for f in g.features.iter().filter(|f| f.kind == "rock" && f.active) {
                    rocks.insert(f.id, f.x);
                }
            }
            let mut counts = std::collections::BTreeMap::new();
            for x in rocks
                .values()
                .filter(|&&x| x >= 1000. * WORLD_SCALE && x < 10000. * WORLD_SCALE)
            {
                *counts.entry((x / WORLD_SCALE / 1000.) as u32).or_insert(0) += 1;
            }
            // Authored minimum, not a recovered native density: later rock
            // missions need multiple opportunities beyond the shared opening.
            assert!(
                counts.values().sum::<i32>() >= 8,
                "seed={seed} later rocks starved: {counts:?}"
            );
            assert!(
                counts.len() >= 4,
                "seed={seed} rock encounters clustered into too few regions: {counts:?}"
            );
        }
    }
    #[test]
    fn backward_wingsuit_release_completes_loop_then_returns_forward() {
        let mut g = flat_game(0);
        g.grounded = false;
        g.y = -100000.;
        g.wing = 6.;
        g.wingsuit();
        let angle = 200_f64.to_radians();
        g.angle = angle;
        g.vx = 1000. * angle.cos();
        g.vy = 1000. * angle.sin();
        g.angular = -250_f64.to_radians();
        g.release();
        advance(&mut g, 1.);
        assert!(g.alive && g.wing_on);
        assert!(g.vx > 0., "released loop stranded vx={}", g.vx);
        assert_eq!(g.angular, 0.);
    }
    #[test]
    fn native_half_turn_queues_points_but_bad_landing_does_not_bank() {
        let mut g = flat_game(1);
        g.grounded = false;
        g.y = -100000.;
        g.angle = -179_f64.to_radians();
        g.held = true;
        g.held_time = 1.;
        advance(&mut g, 0.02);
        assert_eq!(g.awarded_flips, 1);
        assert_eq!(g.combo_points, 10);
        assert_eq!(g.score, 0);
        g.bad_landing();
        assert!(!g.alive);
        assert_eq!(g.progress.flips, 0);
        assert_eq!(g.score, 0);
    }
    #[test]
    fn pending_board_flip_survives_wingsuit_mode_changes() {
        let mut g = flat_game(1);
        g.grounded = false;
        g.y = -100000.;
        g.angle = -179_f64.to_radians();
        g.held = true;
        g.held_time = 1.;
        advance(&mut g, 0.02);
        g.wing = 6.;
        g.wingsuit();
        g.wingsuit();
        g.bank();
        assert_eq!(g.progress.flips, 1);
        assert!(g.score >= 10);
    }
    #[test]
    fn feather_board_collects_fixed_ground_coin_rows() {
        let mut g = flat_game(0);
        g.feather = 5.;
        g.hover = 60.;
        g.y = -78.;
        fixture(&mut g, "coin", 4., -34., 18.);
        g.tick(1. / 120.);
        assert_eq!(g.coins, 1);
        assert!(g.grounded);
    }
    #[test]
    fn double_jump_expires_and_cannot_be_rearmed_by_wingsuit_toggles() {
        let mut g = flat_game(4);
        g.press();
        g.release();
        g.y = -100000.;
        advance(&mut g, 0.85);
        g.press();
        assert!(g.air_hop_used);
        g.release();
        let after_second_jump = g.vy;
        g.press();
        assert_eq!(g.vy, after_second_jump);
        let mut expired = flat_game(4);
        expired.press();
        expired.release();
        expired.y = -100000.;
        advance(&mut expired, DOUBLE_JUMP_WINDOW + 0.02);
        expired.wing = 6.;
        expired.wingsuit();
        expired.wingsuit();
        let before = expired.vy;
        expired.press();
        assert!(!expired.air_hop_used);
        assert_eq!(expired.vy, before);
    }
    #[test]
    fn double_jump_is_passive_for_felipe_and_tupa_only() {
        for rider in 0..6 {
            let mut g = flat_game(rider);
            g.press();
            g.release();
            advance(&mut g, 0.3);
            let before = g.vy;
            g.press();
            if [4, 5].contains(&rider) {
                assert!(g.air_hop_used && g.vy < before);
            } else {
                assert!(!g.air_hop_used);
            }
        }
    }
    #[test]
    fn legacy_distance_is_migrated_without_losing_earned_progress() {
        let g = Game::new(r#"{"meters":7500,"coins":20,"flips":4}"#);
        assert_eq!(g.progress.meters, 750.);
        assert_eq!(g.progress.save_version, 5);
        let restored = Game::new(&g.save());
        assert_eq!(restored.progress.meters, 750.);
    }
    #[test]
    fn rock_bounce_has_airtime_for_a_followup_trick() {
        let mut g = flat_game(5);
        fixture(&mut g, "rock", 50., -22., 120.);
        g.x = 50.;
        g.y = -68.;
        g.grounded = false;
        g.vx = 0.;
        g.vy = 19.7 * WORLD_SCALE;
        advance(&mut g, 0.02);
        assert!(g.vy < -24. * WORLD_SCALE && g.vx >= 4.8 * WORLD_SCALE);
        let bounce_y = g.y;
        advance(&mut g, 0.6);
        assert!(g.alive && g.y < bounce_y - 300.);
        assert_eq!(g.progress.bounces, 1);
        assert!(g.tricks.iter().any(|t| t.name == "Rock bounce"));
    }
    #[test]
    fn bunting_load_relaxes_off_contact_and_breaks_only_bunting() {
        for variant in [0, 1, 2] {
            let mut g = flat_game(0);
            fixture(&mut g, "rail", 3000., -500., 12000.);
            let id = g.features[0].id;
            g.features[0].variant = variant;
            g.rail = Some(id);
            g.grounded = false;
            g.y = -518.;
            advance(&mut g, 2.);
            if variant == 0 {
                let loaded = g.rope_states[&id].load;
                g.rail = None;
                g.y = -4000.;
                advance(&mut g, 0.5);
                assert!(g.rope_states[&id].load < loaded);
                g.rail = Some(id);
                g.y = -518.;
            }
            advance(&mut g, 4.);
            if variant == 0 {
                assert!(!g.features[0].active && g.rail.is_none());
                assert!(g.rope_states[&id].broken_at.is_some());
                g.start_seeded(false, 1);
                assert!(g.rope_states.is_empty());
            } else {
                assert!(g.features[0].active && g.rail.is_some());
                assert!(!g.rope_states.contains_key(&id));
            }
        }
    }
    #[test]
    fn bunting_breaks_at_three_seconds_for_every_rider_and_jump_resets_pressure() {
        for rider in 0..6 {
            let mut g = flat_game(rider);
            fixture(&mut g, "rail", 3000., -500., 12000.);
            let id = g.features[0].id;
            g.rail = Some(id);
            g.grounded = false;
            g.y = -518.;
            advance(&mut g, 2.99);
            assert!(g.rope_states[&id].broken_at.is_none());
            g.press();
            g.release();
            g.tick(1. / 120.);
            assert_eq!(g.rope_states[&id].load, 0.);
            g.rail = Some(id);
            g.y = -518.;
            g.held = false;
            advance(&mut g, 2.99);
            assert!(g.rope_states[&id].broken_at.is_none());
            advance(&mut g, 0.02);
            assert!(g.rope_states[&id].broken_at.is_some());
            assert!(!g.features[0].active);
        }
    }
    #[test]
    fn convex_downhill_stays_attached_but_a_true_crest_and_jump_can_release() {
        for rider in 0..6 {
            let mut g = flat_game(rider);
            g.course.segments = vec![course::Segment {
                x0: 0.,
                x1: 2000.,
                y0: 0.,
                y1: 1800.,
                m0: 0.2,
                m1: 1.6,
            }];
            g.y = -18.;
            g.speed = PROFILES[rider].speed_min * WORLD_SCALE;
            for _ in 0..360 {
                g.tick(1. / 120.);
                assert!(
                    g.grounded && g.alive,
                    "rider={rider} x={} {}",
                    g.x,
                    g.notice
                );
            }
            assert!((g.y - g.course.sample(g.x).0 + 18.).abs() < 1e-6);
            g.press();
            assert!(!g.grounded && g.vy < g.vx * g.course.sample(g.x).1);
        }
        let mut g = flat_game(0);
        g.course.segments = vec![course::Segment {
            x0: 0.,
            x1: 500.,
            y0: 250.,
            y1: 250.,
            m0: -2.,
            m1: 2.,
        }];
        g.x = 249.5;
        g.y = g.course.sample(g.x).0 - 18.;
        g.speed = 1000.;
        g.tick(1. / 120.);
        assert!(
            !g.grounded,
            "a sharp true crest should retain an airborne exit"
        );
    }
    #[test]
    fn elder_closes_on_downhill_cruising_but_a_banked_trick_opens_a_lead() {
        let mut g = flat_game(0);
        g.course.segments[0].m0 = 0.4;
        g.course.segments[0].m1 = 0.4;
        g.course.segments[0].y0 = -400.;
        g.course.segments[0].y1 = 8000.;
        g.speed = ground_speed(800., 0.4, 100., 0, 0.);
        g.vx = g.speed / 1.16_f64.sqrt();
        g.next_chase = 1.;
        advance(&mut g, 0.1);
        let before = g.x - g.chase.as_ref().unwrap().x;
        advance(&mut g, 1.);
        let after = g.x - g.chase.as_ref().unwrap().x;
        assert!(after < before - 50. && g.alive);
        g.combo = 1;
        g.combo_points = 10;
        push_trick(&mut g.tricks, "Backflip", 1, 10);
        g.bank();
        advance(&mut g, 0.5);
        assert!(g.x - g.chase.as_ref().unwrap().x > after && g.alive);
    }
    #[test]
    fn save_and_roster_gates_survive_restart() {
        let mut g = game(1);
        assert!(!g.select(5));
        advance(&mut g, 1.);
        let saved = g.save();
        let restored = Game::new(&saved);
        assert_eq!(restored.progress.runs, 1);
        assert!(restored.progress.meters > 0.);
    }
    #[test]
    fn challenges_credit_landed_flips_and_ignore_zen_practice() {
        let mut g = flat_game(0);
        g.pending_board_flips = 2;
        g.combo = 1;
        g.combo_points = 60;
        push_trick(&mut g.tricks, "Double backflip", 2, 60);
        assert_eq!(g.progress.missions.values[0], 0.);
        g.bank();
        assert!(g.progress.missions.completed[0]);
        let mut zen = flat_game(0);
        zen.zen = true;
        zen.pending_board_flips = 2;
        zen.combo = 1;
        zen.combo_points = 60;
        push_trick(&mut zen.tricks, "Double backflip", 2, 60);
        zen.bank();
        assert_eq!(zen.progress.missions.values, [0.; 3]);
    }
    #[test]
    fn ordered_chain_goals_require_the_landed_sequence_and_correct_character() {
        for reverse in [false, true] {
            let mut g = flat_game(0);
            g.progress.missions.level = 26;
            g.combo = 2;
            g.combo_points = 20;
            g.pending_board_flips = 1;
            for name in if reverse {
                ["Backflip", "Bunting grind"]
            } else {
                ["Bunting grind", "Backflip"]
            } {
                push_trick(&mut g.tricks, name, 1, 10);
            }
            assert_eq!(g.progress.missions.values[0], 0.);
            g.bank();
            assert_eq!(g.progress.missions.values[0], if reverse { 0. } else { 1. });
        }
        let mut g = flat_game(0);
        g.progress.missions.level = 24;
        g.combo = 3;
        g.combo_points = 50;
        for name in ["Wallride", "Wingsuit", "Wallride"] {
            push_trick(&mut g.tricks, name, 1, 10);
        }
        g.bank();
        assert_eq!(g.progress.missions.values[0], 1.);
        for rider in [0, 1] {
            let mut g = flat_game(rider);
            g.progress.missions.level = 11;
            g.combo = 1;
            g.combo_points = 10;
            g.pending_board_flips = 1;
            push_trick(&mut g.tricks, "Backflip", 1, 10);
            g.bank();
            assert_eq!(
                g.progress.missions.values[0],
                if rider == 1 { 1. } else { 0. }
            );
        }
    }
    #[test]
    fn legacy_mission_migration_preserves_economy_but_removes_automatic_unlocks() {
        let g = Game::new(
            r#"{"save_version":4,"meters":999999,"coins":90000,"wallet":42000,"runs":100,"selected":5,"upgrades":{"magnet":6,"wingsuit":5}}"#,
        );
        assert_eq!(level(&g.progress), 1);
        assert_eq!(g.rider, 0);
        assert!(g.progress.missions.migrated);
        assert_eq!((g.progress.wallet, g.progress.coins), (42000, 90000));
        assert_eq!(
            (g.progress.upgrades.magnet, g.progress.upgrades.wingsuit),
            (6, 5)
        );
        let restored = Game::new(&g.save());
        assert_eq!(restored.progress.save_version, 5);
        assert_eq!(level(&restored.progress), 1);
    }
    #[test]
    fn sixty_levels_advance_in_order_and_roster_unlocks_only_at_ten_level_milestones() {
        let mut g = Game::new("{}");
        for l in 1..=60 {
            assert_eq!(g.progress.missions.level, l);
            assert_eq!(
                RIDERS
                    .iter()
                    .filter(|r| r.level <= level(&g.progress))
                    .count(),
                (((l - 1) / 10) + 1).min(6) as usize
            );
            let events: Vec<_> = progression::definitions(l)
                .into_iter()
                .map(|goal| (goal.metric, goal.target))
                .collect();
            g.progress.missions.record_for_rider(
                &events,
                Some([0, 1, 2, 3, 5, 4][((l / 10).min(5)) as usize]),
            );
            assert_eq!(g.progress.missions.level, l + 1);
            let restored = Game::new(&g.save());
            assert_eq!(restored.progress.missions.level, l + 1);
        }
        assert_eq!(g.progress.missions.history.len(), 60);
        assert!(g.progress.missions.goals().is_empty());
    }
}
fn jump_impulse(rider: usize) -> f64 {
    PROFILES[rider].jump * WORLD_SCALE
}
#[derive(Clone, Serialize)]
struct Pose {
    x: f64,
    y: f64,
    angle: f64,
    time: f64,
    terrain_time: f64,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    pub save_version: u32,
    pub meters: f64,
    pub coins: u32,
    pub wallet: u32,
    pub upgrades: shop::Upgrades,
    pub helmets: u8,
    pub rescue_picks: u8,
    pub flips: u32,
    pub grinds: u32,
    pub bounces: u32,
    pub best: u32,
    pub runs: u32,
    pub selected: usize,
    pub missions: progression::Missions,
    pub post_campaign_meters: f64,
}
#[derive(Serialize)]
pub struct Rider {
    name: &'static str,
    ability: &'static str,
    description: &'static str,
    level: u32,
    color: &'static str,
}
const RIDERS: [Rider; 6] = [
    Rider {
        name: "Basic",
        ability: "Balanced",
        description: "Hold to rotate; release for gentle recovery toward your travel direction.",
        level: 1,
        color: "#e8edf9",
    },
    Rider {
        name: "Fast",
        ability: "Quick spin",
        description: "Faster flips with lighter gravity.",
        level: 11,
        color: "#64e2ed",
    },
    Rider {
        name: "Tank",
        ability: "Momentum",
        description: "Heavy momentum with slower flips.",
        level: 21,
        color: "#a6b8ff",
    },
    Rider {
        name: "Ranged",
        ability: "Long boost",
        description: "Longer speed boosts after successful tricks.",
        level: 31,
        color: "#ffb786",
    },
    Rider {
        name: "Protector",
        ability: "Veteran",
        description: "Quick flips, double jump, and one chasm rescue.",
        level: 51,
        color: "#8ee0b6",
    },
    Rider {
        name: "Vampire",
        ability: "Double jump",
        description: "Higher jumps with one midair hop.",
        level: 41,
        color: "#ef91b8",
    },
];
fn goals(p: &Progress, _level: u32) -> Vec<progression::Goal> {
    p.missions.goals()
}
fn level(p: &Progress) -> u32 {
    p.missions.level.min(60)
}
mod course;
use course::Course;
#[derive(Clone, Serialize)]
pub struct Feature {
    #[serde(skip_serializing_if = "Option::is_none")]
    appearance: Option<course::Appearance>,
    id: u32,
    kind: &'static str,
    x: f64,
    y: f64,
    y2: f64,
    width: f64,
    sag: f64,
    variant: u8,
    #[serde(skip)]
    anchored: bool,
    #[serde(skip)]
    magnetized: bool,
    #[serde(skip)]
    offset: f64,
    #[serde(skip)]
    offset2: f64,
    active: bool,
}
#[wasm_bindgen]
#[derive(Clone)]
pub struct Game {
    rope_states: std::collections::BTreeMap<u32, RopeState>,
    progress: Progress,
    rider: usize,
    course: Course,
    seed: u32,
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    speed: f64,
    angle: f64,
    angular: f64,
    rotation: f64,
    awarded_flips: u32,
    pending_board_flips: u32,
    flip_stack: Option<usize>,
    grounded: bool,
    held: bool,
    held_time: f64,
    buffer: f64,
    coyote: f64,
    rail: Option<u32>,
    rewarded_rails: Vec<u32>,
    rewarded_walls: Vec<u32>,
    chase: Option<Chase>,
    next_chase: f64,
    wall: Option<u32>,
    alive: bool,
    zen: bool,
    time: f64,
    terrain_time: f64,
    previous: Pose,
    air_hop_used: bool,
    double_jump_until: f64,
    rock_cooldown: f64,
    cooldown: f64,
    power: f64,
    immune: f64,
    score: u32,
    coins: u32,
    combo: u32,
    combo_points: u32,
    tricks: Vec<Trick>,
    banked_tricks: Vec<Trick>,
    banked_score: u32,
    bank_time: f64,
    notice: String,
    notice_time: f64,
    features: Vec<Feature>,
    wing: f64,
    wing_on: bool,
    wing_charged: bool,
    scarf_growth: f64,
    scarf_length: f64,
    contact_distance: f64,
    proximity: bool,
    proximity_distance: f64,
    wall_time: f64,
    wall_boosted: bool,
    wall_cooldown: f64,
    ground_contact_cooldown: f64,
    magnet: f64,
    feather: f64,
    feather_grace: f64,
    hover: f64,
    shield: bool,
    shop_helmet: bool,
    boost: f64,
    rescues_left: u32,
    slow: bool,
    poison: f64,
    exposure: f64,
}
#[wasm_bindgen]
impl Game {
    #[wasm_bindgen(constructor)]
    pub fn new(saved: &str) -> Game {
        let mut p: Progress = serde_json::from_str(saved).unwrap_or_default();
        if !p.meters.is_finite() || p.meters < 0. {
            p.meters = 0.;
        }
        if p.save_version < 2 {
            p.meters /= 10.;
            p.save_version = 2;
        }
        if p.save_version < 3 {
            p.wallet = p.coins;
            p.save_version = 3;
        }
        if p.save_version < 5 {
            p.missions = progression::Missions {
                migrated: p.runs > 0 || p.meters > 0. || p.coins > 0,
                ..Default::default()
            };
            p.save_version = 5;
        }
        p.missions.sanitize();
        if !p.post_campaign_meters.is_finite() || p.post_campaign_meters < 0. {
            p.post_campaign_meters = 0.;
        }
        if p.missions.level <= 60 {
            p.post_campaign_meters = 0.;
        }
        p.wallet = p.wallet.min(p.coins);
        p.upgrades.sanitize();
        p.helmets = p.helmets.min(3);
        p.rescue_picks = p.rescue_picks.min(3);
        let selected = p.selected.min(5);
        let rider = if RIDERS[selected].level <= level(&p) {
            selected
        } else {
            0
        };
        p.selected = rider;
        let mut course = Course::new(1);
        let mut features = vec![];
        course.extend(0., &mut features);
        let (ground, slope, _) = course.sample(0.);
        Game {
            rope_states: std::collections::BTreeMap::new(),
            progress: p,
            rider,
            course,
            seed: 1,
            x: 0.,
            y: ground - 18.,
            vx: PROFILES[rider].speed_min * WORLD_SCALE / (1. + slope * slope).sqrt(),
            vy: 0.,
            speed: PROFILES[rider].speed_min * WORLD_SCALE,
            angle: slope.atan(),
            angular: 0.,
            rotation: 0.,
            awarded_flips: 0,
            pending_board_flips: 0,
            flip_stack: None,
            grounded: true,
            held: false,
            held_time: 0.,
            buffer: 0.,
            coyote: 0.,
            rail: None,
            rewarded_rails: vec![],
            rewarded_walls: vec![],
            chase: None,
            next_chase: CHASE_INTERVAL,
            wall: None,
            alive: false,
            zen: false,
            time: 0.,
            terrain_time: 0.,
            previous: Pose {
                x: 0.,
                y: ground - 18.,
                angle: slope.atan(),
                time: 0.,
                terrain_time: 0.,
            },
            air_hop_used: false,
            double_jump_until: -1.,
            rock_cooldown: 0.,
            cooldown: 0.,
            power: 0.,
            immune: 0.,
            score: 0,
            coins: 0,
            combo: 0,
            combo_points: 0,
            tricks: vec![],
            banked_tricks: vec![],
            banked_score: 0,
            bank_time: 0.,
            notice: String::new(),
            notice_time: 0.,
            features,
            wing: 0.,
            wing_on: false,
            wing_charged: false,
            scarf_growth: 0.,
            scarf_length: 0.,
            contact_distance: 0.,
            proximity: false,
            proximity_distance: 0.,
            wall_time: 0.,
            wall_boosted: false,
            wall_cooldown: 0.,
            ground_contact_cooldown: 0.,
            magnet: 0.,
            feather: 0.,
            feather_grace: 0.,
            hover: 0.,
            shield: false,
            shop_helmet: false,
            boost: 0.,
            rescues_left: PROFILES[rider].chasm_rescues,
            slow: false,
            poison: 0.,
            exposure: 0.,
        }
    }
    pub fn select(&mut self, rider: usize) -> bool {
        if self.alive || rider >= 6 || level(&self.progress) < RIDERS[rider].level {
            return false;
        }
        self.rider = rider;
        self.progress.selected = rider;
        true
    }
    pub fn start(&mut self, zen: bool) {
        self.start_seeded(
            zen,
            self.progress
                .runs
                .wrapping_mul(747796405)
                .wrapping_add(2891336453),
        );
    }
    pub fn start_seeded(&mut self, zen: bool, seed: u32) {
        let p = self.progress.clone();
        let r = self.rider;
        *self = Self::new("{}");
        self.progress = p;
        self.rider = r;
        self.speed = PROFILES[r].speed_min * WORLD_SCALE;
        self.vx = self.speed / 1.04_f64.sqrt();
        self.rescues_left = PROFILES[r].chasm_rescues;
        self.seed = seed;
        self.next_chase = CHASE_INTERVAL;
        self.course = Course::new(seed);
        self.features.clear();
        self.course.extend(0., &mut self.features);
        let (ground, slope, _) = self.course.sample(0.);
        self.y = ground - 18.;
        self.angle = slope.atan();
        self.vx = self.speed / (1. + slope * slope).sqrt();
        self.previous = Pose {
            x: self.x,
            y: self.y,
            angle: self.angle,
            time: 0.,
            terrain_time: 0.,
        };
        self.zen = zen;
        if !zen {
            self.progress.missions.start_run();
        }
        self.shop_helmet = !zen && self.progress.helmets > 0;
        self.shield = self.shop_helmet;
        self.alive = true;
        self.progress.runs += 1;
        self.say("Tap to jump. Hold airborne to flip; release to land.");
    }
    pub fn press(&mut self) {
        if !self.alive || self.held {
            return;
        }
        self.held = true;
        self.held_time = 0.;
        if self.wing_on {
            self.buffer = 0.;
            return;
        }
        self.buffer = 0.1;
        if self.grounded || self.rail.is_some() || self.wall.is_some() || self.coyote > 0. {
            self.jump();
        } else if PROFILES[self.rider].double_jump_enabled
            && !self.air_hop_used
            && self.time <= self.double_jump_until
        {
            self.air_hop_used = true;
            self.vy = -PROFILES[self.rider].double_jump * WORLD_SCALE;
            self.buffer = 0.;
        }
    }
    pub fn release(&mut self) {
        self.held = false;
        if !self.wing_on {
            self.angular = 0.;
        }
        if self.wall.is_some() && self.wall_time >= 0.15 {
            let scale = (self.contact_distance / (15. * WORLD_SCALE))
                .clamp(0.2, 1.)
                .powi(2);
            self.vx += 5. * WORLD_SCALE * scale;
            self.vy = self.vy.min(-10. * WORLD_SCALE * scale);
            self.wall = None;
            self.wall_cooldown = 0.3;
            self.combo += 1;
            self.combo_points += 20;
            push_trick(&mut self.tricks, "Wallride kicker", 1, 20);
        }
    }
    pub fn ability(&mut self) {
        // Compatibility for older hosts. Rider traits trigger through play.
    }
    pub fn wingsuit(&mut self) {
        if self.alive
            && !self.grounded
            && (self.wing_on
                || self.wing_charged
                || self.scarf_length >= 4.8
                || (self.wing >= 4.8 && self.scarf_growth == 0.))
        {
            self.wing_on = !self.wing_on;
            if self.wing_on {
                self.angle = self.vy.atan2(self.vx);
                self.combo += 1;
                self.combo_points += 30;
                push_trick(&mut self.tricks, "Wingsuit", 1, 30);
                self.wing_charged = true;
                self.rotation = 0.;
                self.awarded_flips = 0;
                self.flip_stack = None;
            }
            self.angular = 0.;
            self.buffer = 0.;
            if !self.wing_on {
                self.rotation = 0.;
                self.awarded_flips = 0;
                self.flip_stack = None;
                if self.held {
                    self.held_time = reference::FLIP_DELAY + 0.01;
                }
            }
            self.rail = None;
            self.wall = None;
        }
    }
    pub fn save(&self) -> String {
        serde_json::to_string(&self.progress).unwrap()
    }
    pub fn challenges(&self) -> String {
        self.progress.missions.catalog().to_string()
    }
    pub fn economy(&self) -> String {
        shop::balance_report(&self.progress).to_string()
    }
    pub fn shop(&self) -> String {
        serde_json::to_string(&shop::catalog(&self.progress)).unwrap()
    }
    pub fn purchase(&mut self, id: &str) -> bool {
        !self.alive && shop::buy(&mut self.progress, id)
    }
    pub fn snapshot(&self) -> String {
        let terrain: Vec<_> = (-63..=219)
            .map(|i| {
                let x = (self.x / 16.).floor() * 16. + i as f64 * 16.;
                let (y, s, _, vy) = self.course.sample_at(x, self.terrain_time);
                serde_json::json!({"x":x,"y":y,"slope":s,"vy":vy,"gap":self.course.gap(x)})
            })
            .collect();
        let effects:Vec<_>=self.features.iter().filter(|f|f.active).map(|f|{let phase=(self.time*0.65+f.id as f64*0.173).rem_euclid(1.);serde_json::json!({"id":f.id,"phase":phase,"danger":match f.kind{"lightning"=>(0.62..0.9).contains(&phase),"spotlight"=>phase<0.65,_=>true}})}).collect();
        serde_json::json!({"x":self.x,"y":self.y,"vx":self.vx,"vy":self.vy,"angle":self.angle,"grounded":self.grounded,"speed":self.speed,"alive":self.alive,"zen":self.zen,"seed":self.seed,"biome":((self.x/8500.) as u32+self.seed%3)%3,"slow":self.slow,"poison":self.poison,"exposure":self.exposure,"rail":self.rail,"wall":self.wall,"effects":effects,"score":self.score,"coins":self.coins,"distance":(self.x/WORLD_SCALE) as u32,"combo":self.combo,"combo_points":self.combo_points,"cooldown":self.cooldown,"power":self.power,"immune":self.immune,"wing":self.wing,"wing_on":self.wing_on,"rider":self.rider,"riders":RIDERS,"features":self.features,"terrain":terrain,"terrain_segments":self.course.segments,"level":level(&self.progress),"goals":goals(&self.progress,level(&self.progress)),"progress":self.progress,"notice":if self.notice_time>0.{&self.notice}else{""},"time":self.time,"previous":self.previous,"terrain_time":self.terrain_time,"horde_phase":(self.terrain_time/6.).rem_euclid(1.),"horde_flow":20.,"magnet":self.magnet,"feather":self.feather,"hover":self.hover,"shield":self.shield,"boost":self.boost,"tricks":self.tricks,"banked_tricks":self.banked_tricks,"banked_score":self.banked_score,"bank_time":self.bank_time,"ruleset":6,"chase":self.chase,"camp":{"x":self.next_chase,"y":self.course.sample(self.next_chase).0-18.},"gaps":self.course.gaps,"angular":self.angular,"held":self.held,"wall_distance":self.contact_distance,"wall_time":self.wall_time,"contact_cooldown":self.ground_contact_cooldown,"boost_max":PROFILES[self.rider].boost_max,"scarf_length":self.scarf_length,"scarf_growth":self.scarf_growth,"wing_ready":self.wing_charged || self.scarf_length>=4.8 || (self.wing>=4.8 && self.scarf_growth==0.),"proximity":self.proximity,"rock_outline":ROCK_OUTLINE,"rope_states":self.rope_states,"grind_distance_m":self.contact_distance/WORLD_SCALE}).to_string()
    }
    pub fn tick(&mut self, dt: f64) {
        if !self.alive || !dt.is_finite() || dt <= 0. {
            return;
        }
        if let Some(chase) = &mut self.chase {
            chase.previous_x = chase.x;
            chase.previous_y = chase.y;
        }
        // Renderer interpolates one outer fixed tick. Internal collision
        // substeps must not overwrite the beginning of that interval.
        self.previous = Pose {
            x: self.x,
            y: self.y,
            angle: self.angle,
            time: self.time,
            terrain_time: self.terrain_time,
        };
        let total = dt.min(0.05);
        let steps = (total * 240. - 1e-8).ceil().max(1.) as u32;
        for _ in 0..steps {
            if self.alive {
                self.step(total / steps as f64);
            }
        }
    }
}
impl Game {
    fn say(&mut self, s: &str) {
        self.notice = s.into();
        self.notice_time = 2.5;
    }
    fn crash(&mut self, s: &str) {
        if self.immune > 0. {
            return;
        }
        if self.shield && !s.starts_with("Lost in the chasm") {
            self.recover_shield();
            return;
        }
        if self.zen {
            self.immune = 2.;
            self.power = 0.;
            self.angle = self.course.sample(self.x).1.atan();
            self.angular = 0.;
            self.rotation = 0.;
            self.combo = 0;
            self.combo_points = 0;
            self.tricks.clear();
            self.pending_board_flips = 0;
            self.awarded_flips = 0;
            self.flip_stack = None;
            self.say(if self.zen {
                "Zen recovery · keep going"
            } else {
                "Fortify absorbed the hit"
            });
        } else {
            self.alive = false;
            self.held = false;
            self.say(s);
        }
    }
    fn recover_shield(&mut self) {
        if self.shop_helmet {
            if !self.zen {
                self.progress.helmets = self.progress.helmets.saturating_sub(1);
            }
            self.shop_helmet = false;
        }
        self.shield = false;
        self.immune = 0.8;
        self.angle = self.course.sample_at(self.x, self.terrain_time).1.atan();
        self.angular = 0.;
        self.rotation = 0.;
        self.awarded_flips = 0;
        self.flip_stack = None;
        self.combo = 0;
        self.combo_points = 0;
        self.tricks.clear();
        self.pending_board_flips = 0;
        self.buffer = 0.;
        self.held = false;
        self.say("Shield saved the landing");
    }
    fn bad_landing(&mut self) {
        if self.shield {
            self.recover_shield();
            return;
        }
        if self.zen {
            self.angle = self.course.sample(self.x).1.atan();
            self.angular = 0.;
            self.rotation = 0.;
            self.combo = 0;
            self.combo_points = 0;
            self.tricks.clear();
            self.pending_board_flips = 0;
            self.say("Zen recovery · unfinished flip");
        } else {
            self.alive = false;
            self.held = false;
            self.say("Unfinished flip · release before landing");
        }
    }
    fn rail_line(f: &Feature, x: f64) -> (f64, f64) {
        let t = ((x - (f.x - f.width / 2.)) / f.width).clamp(0., 1.);
        (
            f.y + (f.y2 - f.y) * t + f.sag * 4. * t * (1. - t),
            (f.y2 - f.y + 4. * f.sag * (1. - 2. * t)) / f.width,
        )
    }
    fn jump(&mut self) {
        let rail = self
            .rail
            .and_then(|id| self.features.iter().find(|f| f.id == id));
        let s = rail
            .map(|f| Self::rail_line(f, self.x).1)
            .unwrap_or_else(|| self.course.sample_at(self.x, self.terrain_time).1);
        let surface_vy = rail
            .map(|f| {
                if !f.anchored {
                    return 0.;
                }
                let t = ((self.x - f.x + f.width / 2.) / f.width).clamp(0., 1.);
                self.course
                    .sample_at(f.x - f.width / 2., self.terrain_time)
                    .3
                    * (1. - t)
                    + self
                        .course
                        .sample_at(f.x + f.width / 2., self.terrain_time)
                        .3
                        * t
            })
            .unwrap_or_else(|| self.course.sample_at(self.x, self.terrain_time).3);
        self.vy = self.vx * s + surface_vy - jump_impulse(self.rider);
        self.grounded = false;
        self.rail = None;
        self.wall = None;
        self.buffer = 0.;
        self.coyote = 0.;
        self.double_jump_until = self.time + DOUBLE_JUMP_WINDOW;
        self.angle = s.atan();
        self.angular = 0.;
    }
    fn bank(&mut self) {
        self.progress.flips += self.pending_board_flips;
        if self.combo > 0 {
            let bank = self.combo_points * self.combo;
            self.score += bank;
            if !self.zen {
                use progression::Metric as M;
                let mut events = vec![
                    (M::Flips, self.pending_board_flips as f64),
                    (M::RiderFlips, self.pending_board_flips as f64),
                    (M::ComboSize, self.combo as f64),
                    (M::ComboScore, bank as f64),
                    (M::Score, bank as f64),
                ];
                let mut wall_chain = 0;
                let mut balloon_before_flip = false;
                let mut grind_before_flip = false;
                for t in &self.tricks {
                    if t.name == "Wallride" && wall_chain == 0 {
                        wall_chain = 1;
                    } else if t.name == "Wingsuit" && wall_chain == 1 {
                        wall_chain = 2;
                    } else if t.name == "Wallride" && wall_chain == 2 {
                        wall_chain = 3;
                    }
                    if t.name == "Balloon bounce" {
                        balloon_before_flip = true;
                    }
                    if t.name.contains("grind") && t.name != "Long grind boost" {
                        grind_before_flip = true;
                    }
                    if t.name.to_lowercase().contains("backflip") {
                        if balloon_before_flip {
                            events.push((M::BalloonFlip, 1.));
                            balloon_before_flip = false;
                        }
                        if grind_before_flip {
                            events.push((M::GrindFlip, 1.));
                            grind_before_flip = false;
                        }
                    }
                    let metric = if t.name.to_lowercase().contains("backflip") {
                        Some(M::FlipStack)
                    } else if t.name == "Long grind boost" {
                        Some(M::LongGrinds)
                    } else if t.name.contains("grind") {
                        Some(M::Grinds)
                    } else if t.name == "Wallride" {
                        Some(M::Walls)
                    } else if t.name == "Rock bounce" {
                        Some(M::Bounces)
                    } else if t.name == "Balloon bounce" {
                        Some(M::Balloons)
                    } else if t.name == "Wingsuit" {
                        Some(M::Wingsuits)
                    } else if t.name == "Wingsuit loop" {
                        Some(M::Loops)
                    } else if t.name == "Proximity flight" {
                        Some(M::Proximity)
                    } else {
                        None
                    };
                    if let Some(metric) = metric {
                        events.push((
                            metric,
                            if matches!(metric, M::Proximity | M::Wingsuits) {
                                1.
                            } else {
                                t.count as f64
                            },
                        ));
                    }
                }
                if wall_chain == 3 {
                    events.push((M::WallWingWall, 1.));
                }
                self.progress
                    .missions
                    .record_for_rider(&events, Some(self.rider));
            }
            // Captured target resets use discrete boost levels (target = level/12).
            // Round the decaying current level upward before applying this bank.
            let charge =
                scarf_bank_levels(&self.tricks) * (1. + self.progress.upgrades.scarf as f64 * 0.1);
            self.wing = (((self.wing * 2.).ceil() + charge) / 2.).min(6.);
            self.scarf_growth = 1.25;
            self.boost = PROFILES[self.rider].boost_max;
            self.banked_tricks = self.tricks.clone();
            self.banked_score = bank;
            self.bank_time = 2.;
            // A landed trick must change velocity now, not just a future target.
            let base = PROFILES[self.rider].speed_min * WORLD_SCALE;
            let kick = 1.22 + (self.combo.saturating_sub(1) as f64 * 0.04).min(0.18);
            self.speed =
                (self.speed.max(base) * kick).min(PROFILES[self.rider].speed_max * WORLD_SCALE);
            let slope = self.course.sample(self.x).1;
            self.vx = self.speed / (1. + slope * slope).sqrt();
            self.vy = self.vx * slope;
            self.angle = slope.atan();
        }
        self.tricks.clear();
        self.combo = 0;
        self.combo_points = 0;
        self.rotation = 0.;
        self.awarded_flips = 0;
        self.flip_stack = None;
        self.wing_on = false;
        self.pending_board_flips = 0;
        self.angular = 0.;
        self.air_hop_used = false;
        self.double_jump_until = -1.;
    }
    fn step(&mut self, dt: f64) {
        let old_x = self.x;
        let post_campaign_active = self.progress.missions.level > 60;
        let old_rescues = self.rescues_left + self.progress.rescue_picks as u32;
        let old_y = self.y;
        let old_hover = self.hover;
        let old_feather = self.feather;
        for (&id, rope) in &mut self.rope_states {
            if self.rail != Some(id) && rope.broken_at.is_none() {
                rope.load = 0.;
                rope.contact_time = 0.;
            }
        }
        self.magnet = (self.magnet - dt).max(0.);
        self.feather = (self.feather - dt).max(0.);
        self.feather_grace = (self.feather_grace - dt).max(0.);
        self.boost = (self.boost - dt).max(0.);
        self.bank_time = (self.bank_time - dt).max(0.);
        if old_feather > 0. && self.feather == 0. {
            self.feather_grace = 0.6;
        }
        let target_hover = if self.feather > 0. { 60. } else { 0. };
        self.hover += (target_hover - self.hover) * (1. - (-6. * dt).exp());
        let old_terrain_time = self.terrain_time;
        let old_ground = self.course.sample_at(old_x, self.terrain_time);
        self.time += dt;
        self.cooldown = (self.cooldown - dt).max(0.);
        self.power = (self.power - dt).max(0.);
        self.immune = (self.immune - dt).max(0.);
        self.notice_time -= dt;
        self.buffer = (self.buffer - dt).max(0.);
        self.coyote = (self.coyote - dt).max(0.);
        self.rock_cooldown = (self.rock_cooldown - dt).max(0.);
        if self.held {
            self.held_time += dt;
        }
        self.slow = self.features.iter().any(|f| {
            f.active
                && f.kind == "chrono"
                && (self.x - f.x).abs() < f.width / 2.
                && (self.y - f.y).abs() < 260.
        });
        let h = dt * if self.slow { 0.55 } else { 1. };
        self.terrain_time += h;
        for f in self.features.iter_mut().filter(|f| f.kind == "balloon") {
            f.sag = (f.sag - h).max(0.);
        }
        self.course
            .anchor_features(self.terrain_time, &mut self.features);
        let mut ax = 0.;
        let mut ay = 0.;
        for f in &self.features {
            if f.active && f.kind == "blackhole" && self.immune == 0. {
                let dx = f.x - self.x;
                let dy = f.y - self.y;
                let r = (dx * dx + dy * dy).sqrt();
                if r < 250. && r > 1. {
                    let force = 1200. * (1. - r / 250.);
                    ax += dx / r * force;
                    ay += dy / r * force;
                }
            }
        }
        self.wall_cooldown = (self.wall_cooldown - h).max(0.);
        self.ground_contact_cooldown = (self.ground_contact_cooldown - h).max(0.);
        // Native trace: normalized depletion is approximately .02 + .04 * length.
        // Exact integration avoids display/substep-dependent charge loss.
        let growth_time = self.scarf_growth.min(h);
        self.scarf_growth = (self.scarf_growth - h).max(0.);
        let drain = 1. - self.progress.upgrades.wingsuit as f64 * 0.07;
        self.wing = ((self.wing + 3.) * (-0.04 * drain * (h - growth_time)).exp() - 3.).max(0.);
        self.scarf_length = if self.scarf_growth > 0. {
            self.scarf_length + (self.wing - self.scarf_length) * (1. - (-6. * h).exp())
        } else {
            self.wing
        };
        self.wing_charged |= self.scarf_length >= 4.8;
        if self.wing < 0.3 {
            self.wing_charged = false;
        }
        if self.wing_on && self.wing < 0.3 {
            self.wing_on = false;
            self.rotation = 0.;
            self.awarded_flips = 0;
            self.flip_stack = None;
            self.buffer = 0.;
            self.angular = 0.;
        }
        let flying = self.wing_on;
        self.proximity = false;
        if let Some(id) = self.wall {
            let valid = self
                .features
                .iter()
                .any(|f| f.id == id && wall_contains(f, self.x, self.y));
            if self.held && valid {
                self.wall_time += h;
                let fade = wall_up_curve(self.contact_distance);
                self.vy =
                    (self.vy + (GRAVITY - 37. * WORLD_SCALE * fade) * h).min(3. * WORLD_SCALE);
                self.x += self.vx * h;
                self.y += self.vy * h;
                self.angle = self.vy.atan2(self.vx);
                self.contact_distance += self.vx.abs() * h;
                let points = ((self.contact_distance / (2. * WORLD_SCALE)) as u32
                    - ((self.contact_distance - self.vx.abs() * h) / (2. * WORLD_SCALE)) as u32)
                    * 5;
                self.combo_points += points;
                if let Some(t) = self.tricks.iter_mut().rev().find(|t| t.name == "Wallride") {
                    t.points += points;
                }
                if self.contact_distance >= 30. * WORLD_SCALE && !self.wall_boosted {
                    self.wing = (self.wing + 2.).min(6.);
                    self.scarf_growth = 1.25;
                    self.combo += 1;
                    self.combo_points += 25;
                    push_trick(&mut self.tricks, "Long wallride boost", 1, 25);
                    self.wall_boosted = true;
                }
            } else {
                self.wall = None;
                self.wall_cooldown = 0.15;
            }
        }
        if self.wall.is_some() {
            // Wall faces are ride zones behind the rider, never solid barriers.
        } else if let Some(id) = self.rail {
            if let Some(f) = self.features.iter_mut().find(|f| f.id == id && f.active) {
                let (_, s) = Self::rail_line(f, self.x);
                self.speed = ground_speed(self.speed, s, h, self.rider, self.boost);
                self.vx = self.speed / (1. + s * s).sqrt();
                self.vy = self.vx * s;
                self.x += self.vx * h;
                self.y = Self::rail_line(f, self.x).0 - 18.;
                self.angle = Self::rail_line(f, self.x).1.atan();
                let before = self.contact_distance;
                self.contact_distance += self.vx.abs() * h;
                let points = ((self.contact_distance / (2. * WORLD_SCALE)) as u32
                    - (before / (2. * WORLD_SCALE)) as u32)
                    * 5;
                self.combo_points += points;
                if let Some(t) =
                    self.tricks.iter_mut().rev().find(|t| {
                        matches!(t.name, "Rooftop grind" | "Bunting grind" | "Balloon grind")
                    })
                {
                    t.points += points;
                }
                if ((before / (60. * WORLD_SCALE)) as u32)
                    < (self.contact_distance / (60. * WORLD_SCALE)) as u32
                {
                    self.wing = (self.wing + 2.).min(6.);
                    self.scarf_growth = 1.25;
                    self.combo += 1;
                    self.combo_points += 300;
                    push_trick(&mut self.tricks, "Long grind boost", 1, 300);
                }
                if self.x > f.x + f.width / 2. {
                    self.rail = None;
                    self.coyote = 0.08;
                }
                // Requested pressure mechanic, not a recovered native break timer.
                // User-defined three seconds of uninterrupted contact.
                if f.variant == 0 && self.rail == Some(id) {
                    let rope = self.rope_states.entry(id).or_default();
                    rope.ridden_to = rope.ridden_to.max(self.x);
                    rope.contact_time += dt;
                    rope.load = (rope.contact_time / 3.).min(1.);
                    if rope.contact_time >= 3. - 1e-9 {
                        rope.broken_at = Some(self.time);
                        f.active = false;
                        self.rail = None;
                        self.coyote = 0.08;
                    }
                }
            } else {
                self.rail = None;
            }
        } else if self.grounded {
            let s = old_ground.1;
            let normal = (1. + s * s).sqrt();
            self.speed = if self.poison > 0. && self.immune == 0. {
                self.speed
            } else {
                ground_speed(self.speed, s, h, self.rider, self.boost)
            };
            if self
                .features
                .iter()
                .any(|f| f.active && f.kind == "ice" && (old_x - f.x).abs() < f.width / 2.)
            {
                self.speed = ice_speed(self.speed, self.rider, h);
            }
            if self.poison > 0. && self.immune == 0. {
                self.speed = (self.speed - 420. * h).max(175.);
            }
            self.vx = self.speed / normal;
            self.vy = self.vx * s + old_ground.3;
            self.x += self.vx * h;
            let g = self.course.sample_at(self.x, self.terrain_time);
            if self.course.gap(self.x)
                || (self.ground_contact_cooldown == 0.
                    && s <= 0.
                    && g.1 > 0.
                    && g.2 * self.speed * self.speed / (1. + g.1 * g.1).powf(1.5)
                        > GRAVITY * PROFILES[self.rider].gravity_scale / normal)
            {
                self.grounded = false;
                self.coyote = 0.08;
                self.y += self.vy * h;
            } else {
                self.y = g.0 - 18. - self.hover;
                self.angle = g.1.atan();
                self.vx = self.speed / (1. + g.1 * g.1).sqrt();
                self.vy = self.vx * g.1 + g.3;
            }
        } else {
            if flying {
                // Captured native release: a backward-facing flight completes its
                // loop through the downward-forward quadrant; forward release
                // stops turning immediately instead of stranding negative vx.
                let heading = self.angle.rem_euclid(TAU);
                let committed_loop = heading > std::f64::consts::FRAC_PI_2
                    && heading < 3. * std::f64::consts::FRAC_PI_2;
                if self.held || committed_loop {
                    let target = -250_f64.to_radians();
                    self.angular += (target - self.angular) * (1. - (-10. * h).exp());
                } else {
                    self.angular = 0.;
                }
                let clearance = (old_ground.0 - self.y - 18.) / WORLD_SCALE;
                if clearance > 0. && clearance < 22. {
                    ay -= 17. * WORLD_SCALE * (1. - clearance / 22.);
                }
                let (vx, vy, dx, dy) =
                    reference::wing_step(self.vx, self.vy, self.angular, ax, ay, h);
                self.vx = vx;
                self.vy = vy;
                self.x += dx;
                self.y += dy;
            } else {
                let (vx, vy, dx, dy) = air_step(
                    self.vx,
                    self.vy,
                    self.rider,
                    self.held && self.held_time > reference::FLIP_DELAY,
                    h,
                );
                self.vx = vx + ax * h;
                self.vy = vy + ay * h;
                self.x += dx;
                self.y += dy;
            }
            let previous_angle = self.angle;
            if flying {
                let heading = self.vy.atan2(self.vx);
                let delta = (heading - self.angle + std::f64::consts::PI).rem_euclid(TAU)
                    - std::f64::consts::PI;
                self.angle += delta;
            } else if self.held && self.held_time > reference::FLIP_DELAY {
                let target = if self.rider == 5 {
                    -225_f64.to_radians()
                } else {
                    -PROFILES[self.rider].flip_factor * TAU * FLIP_RATE_SCALE
                };
                self.angular += (target - self.angular) * (1. - (-30. * h).exp());
            } else {
                let heading = self.vy.atan2(self.vx);
                let delta = (heading - self.angle + std::f64::consts::PI).rem_euclid(TAU)
                    - std::f64::consts::PI;
                self.angular =
                    (delta * std::f64::consts::FRAC_PI_4 * PROFILES[self.rider].unwind_factor)
                        .clamp(-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
            }
            if !flying {
                self.angle += self.angular * h;
            }
            if flying || (self.held && self.held_time > reference::FLIP_DELAY) {
                self.rotation = (self.rotation + previous_angle - self.angle).max(0.);
            }
            // Native flipCheck awards the pending trick at the directed 180°
            // world-angle crossing. Landing safety remains a separate check.
            // Counting crossed phases also includes the small release unwind.
            let before_phase = ((std::f64::consts::PI - previous_angle) / TAU).floor();
            let after_phase = ((std::f64::consts::PI - self.angle) / TAU).floor();
            let completed = (after_phase - before_phase).max(0.) as u32;
            let n = self.awarded_flips + completed;
            if n > self.awarded_flips {
                if flying {
                    self.combo += completed;
                    self.combo_points += 40 * completed;
                    push_trick(&mut self.tricks, "Wingsuit loop", completed, 40 * completed);
                } else {
                    self.pending_board_flips += completed;
                    let total = |flips: u32| match flips {
                        0 => 0,
                        1 => 10,
                        2 => 60,
                        3 => 200,
                        _ => 500 + (flips - 4) * 300,
                    };
                    let points = total(n) - total(self.awarded_flips);
                    self.combo_points += points;
                    let name = match n {
                        1 => "Backflip",
                        2 => "Double backflip",
                        3 => "Triple backflip",
                        _ => "Quadruple backflip",
                    };
                    if self.awarded_flips == 0 {
                        self.combo += 1;
                        self.tricks.push(Trick {
                            name,
                            count: 1,
                            points,
                        });
                        self.flip_stack = Some(self.tricks.len() - 1);
                    } else if let Some(t) =
                        self.flip_stack.and_then(|index| self.tricks.get_mut(index))
                    {
                        t.name = name;
                        t.points += points;
                    }
                }
                self.awarded_flips = n;
            }
        }
        if flying && !self.course.gap(self.x) {
            let clearance = self.course.sample(self.x).0 - self.y - 18.;
            self.proximity = clearance > 0. && clearance < 6. * WORLD_SCALE;
            if self.proximity {
                let before = self.proximity_distance;
                self.proximity_distance += (self.x - old_x).max(0.);
                let count = (self.proximity_distance / (2. * WORLD_SCALE)) as u32
                    - (before / (2. * WORLD_SCALE)) as u32;
                if count > 0 {
                    if !self.tricks.iter().any(|t| t.name == "Proximity flight") {
                        self.combo += 1;
                    }
                    self.combo_points += count * 10;
                    push_trick(&mut self.tricks, "Proximity flight", count, count * 10);
                }
            } else {
                self.proximity_distance = 0.;
            }
        }
        self.progress.meters += (self.x - old_x).max(0.) / WORLD_SCALE;
        let mut event = None;
        let mut danger = None;
        let mut in_swamp = false;
        let mut in_spot = false;
        for f in &mut self.features {
            if !f.active {
                continue;
            }
            let near = (self.x - f.x).abs() < f.width / 2. + 14.;
            let phase = (self.time * 0.65 + f.id as f64 * 0.173).rem_euclid(1.);
            match f.kind {
                "magnet" | "feather" | "shield" => {
                    if (self.x - f.x).abs() < 42. && (self.y - f.y).abs() < 65. {
                        f.active = false;
                        match f.kind {
                            "magnet" => {
                                self.magnet = 5. + self.progress.upgrades.magnet as f64 * 2.5
                            }
                            "feather" => {
                                self.feather = 5. + self.progress.upgrades.feather as f64 * 2.5
                            }
                            _ => {
                                self.shield = true;
                                self.shop_helmet = false;
                            }
                        }
                        event = Some(match f.kind {
                            "magnet" => "Magnet",
                            "feather" => "Feather",
                            _ => "Shield",
                        });
                    }
                }
                "coin" => {
                    if self.magnet > 0. && (self.x - f.x).hypot(self.y - f.y) < 180. {
                        f.anchored = false;
                        f.magnetized = true;
                    }
                    if f.magnetized {
                        // Once caught, follow player translation before closing
                        // the relative gap: speed cannot strand coins behind.
                        f.x += self.x - old_x;
                        f.y += self.y - old_y;
                        let attract = 1. - (-10. * h).exp();
                        f.x += (self.x - f.x) * attract;
                        f.y += (self.y - f.y) * attract;
                    }
                    let reach = if self.rider == 3 { 70. } else { 38. };
                    // Use the rider's body/board extent, not a single center point.
                    // Hovering shifts the center above ground coin rows, but the board
                    // still passes within collection reach.
                    let vertical_gap = ((self.y - f.y).abs() - 18.).max(0.);
                    if (self.x - f.x).abs() < reach && vertical_gap < reach {
                        f.active = false;
                        let value = if f.variant == 1 { 10 } else { 1 };
                        self.coins = self.coins.saturating_add(value);
                        if !self.zen {
                            self.progress.coins = self.progress.coins.saturating_add(value);
                            self.progress.wallet = self.progress.wallet.saturating_add(value);
                        }
                        self.score = self.score.saturating_add(value * 10);
                        if !self.zen {
                            use progression::Metric as M;
                            self.progress.missions.record(&[
                                (M::Coins, value as f64),
                                (M::MagnetCoins, if f.magnetized { value as f64 } else { 0. }),
                                (M::BigCoins, if f.variant == 1 { 1. } else { 0. }),
                                (M::Score, (value * 10) as f64),
                            ]);
                        }
                    }
                }
                "rail" => {
                    let (surface, s) = Self::rail_line(f, self.x);
                    let mut old_surface = Self::rail_line(f, old_x).0;
                    let mut rail_vy = 0.;
                    if f.anchored {
                        let left = f.x - f.width / 2.;
                        let right = f.x + f.width / 2.;
                        let t = ((old_x - left) / f.width).clamp(0., 1.);
                        let a = self.course.sample_at(left, self.terrain_time);
                        let b = self.course.sample_at(right, self.terrain_time);
                        old_surface -= (a.0 - self.course.sample_at(left, old_terrain_time).0)
                            * (1. - t)
                            + (b.0 - self.course.sample_at(right, old_terrain_time).0) * t;
                        rail_vy = a.3 * (1. - t) + b.3 * t;
                    } else if f.variant == 2 {
                        let t = ((old_x - f.x + f.width / 2.) / f.width).clamp(0., 1.);
                        let movement = f.offset * (1. - t) + f.offset2 * t;
                        old_surface -= movement;
                        rail_vy = movement / h;
                    }
                    if self.x >= f.x - f.width / 2.
                        && self.x <= f.x + f.width / 2.
                        && self.rail.is_none()
                        && self.wall.is_none()
                        && !self.grounded
                        && old_y + 18. <= old_surface + 1.
                        && self.y + 18. >= surface
                        && self.vy - self.vx * s - rail_vy > 0.
                    {
                        let err = (self.angle - s.atan() + std::f64::consts::PI).rem_euclid(TAU)
                            - std::f64::consts::PI;
                        if err.abs() > PROFILES[self.rider].landing_degrees.to_radians() {
                            continue;
                        }
                        self.rail = Some(f.id);
                        self.contact_distance = 0.;
                        self.wing_on = false;
                        self.air_hop_used = false;
                        self.y = surface - 18.;
                        self.speed = ((self.vx + self.vy * s) / (1. + s * s).sqrt())
                            .clamp(0., PROFILES[self.rider].speed_max * WORLD_SCALE);
                        self.vy = self.vx * s;
                        self.angle = s.atan();
                        self.angular = 0.;
                        if !self.rewarded_rails.contains(&f.id) {
                            self.rewarded_rails.push(f.id);
                            self.combo += 1;
                            self.combo_points += 10;
                            self.contact_distance = 0.;
                            push_trick(
                                &mut self.tricks,
                                if f.variant == 1 {
                                    "Rooftop grind"
                                } else if f.variant == 2 {
                                    "Balloon grind"
                                } else {
                                    "Bunting grind"
                                },
                                1,
                                10,
                            );
                            self.progress.grinds += 1;
                        }
                    }
                }
                "wall" => {
                    if self.held
                        && self.wall.is_none()
                        && self.rail.is_none()
                        && !self.grounded
                        && self.wall_cooldown == 0.
                        && wall_contains(f, self.x, self.y)
                    {
                        self.wall = Some(f.id);
                        self.wing_on = false;
                        self.angular = 0.;
                        self.wall_time = 0.;
                        self.wall_boosted = self.rewarded_walls.contains(&f.id);
                        self.contact_distance = 0.;
                        self.vy = self.vy.min(3. * WORLD_SCALE);
                        if !self.rewarded_walls.contains(&f.id) {
                            self.rewarded_walls.push(f.id);
                            self.combo += 1;
                            self.combo_points += 10;
                            push_trick(&mut self.tricks, "Wallride", 1, 10);
                        }
                    }
                }
                "balloon" => {
                    let r = f.width / 2.;
                    let dx = self.x - f.x;
                    if dx.abs() < r
                        && !self.grounded
                        && self.rail.is_none()
                        && self.wall.is_none()
                        && self.vy - f.offset / h - self.vx * dx / (r * r - dx * dx).sqrt().max(1.)
                            > 0.
                        && f.sag < 0.01
                    {
                        let top = f.y + r - (r * r - dx * dx).sqrt();
                        let odx = (old_x - f.x).clamp(-r, r);
                        let oldtop = f.y - f.offset + r - (r * r - odx * odx).sqrt();
                        if old_y + 18. <= oldtop + 2. && self.y + 18. >= top {
                            self.y = top - 18.;
                            self.vy = -jump_impulse(self.rider) * 1.2;
                            self.vx += 5. * WORLD_SCALE;
                            self.wing_on = false;
                            self.buffer = 0.;
                            self.air_hop_used = false;
                            f.sag = 0.18;
                            self.combo += 1;
                            self.combo_points += 60;
                            self.progress.bounces += 1;
                            push_trick(&mut self.tricks, "Balloon bounce", 1, 60);
                        }
                    }
                }
                "bounce" => {
                    let left = f.x - f.width / 2.;
                    let right = f.x + f.width / 2.;
                    let entered = self.grounded
                        && old_x < left + 14.
                        && self.x + 14. >= left
                        && self.x <= right + 14.;
                    let descending = near
                        && !self.grounded
                        && old_y + 18. <= f.y + 1.
                        && self.y + 18. >= f.y
                        && self.vy > 0.;
                    if entered || descending {
                        self.grounded = false;
                        self.rail = None;
                        self.buffer = 0.;
                        self.coyote = 0.;
                        self.vy = -500.;
                        self.y = f.y - 18.;
                        f.active = false;
                        self.combo += 1;
                        self.combo_points += 70;
                        self.progress.bounces += 1;
                        self.wing = (self.wing + 0.6).min(6.);
                        event = Some("Bounce · aim for the rail");
                    }
                }
                "blackhole" => {
                    let dx = self.x - f.x;
                    let dy = self.y - f.y;
                    if dx * dx + dy * dy < 25. * 25. {
                        danger = Some("Black Hole core · take the lower route");
                    }
                }
                "swamp" => {
                    if near && self.grounded && self.immune == 0. {
                        in_swamp = true;
                    }
                }
                "spotlight" => {
                    let cone = f.x + (self.time * 1.4 + f.id as f64).sin() * 65.;
                    if (self.x - cone).abs() < f.width * 0.36
                        && self.y + 18. > self.course.sample_at(self.x, self.terrain_time).0 - 70.
                        && phase < 0.65
                        && self.immune == 0.
                    {
                        in_spot = true;
                    }
                }
                "deathwave" => {
                    let wx = f.x + (self.time * 2. + f.id as f64).sin() * 24.;
                    let height = 38. + 12. * (self.time * 3. + f.id as f64).sin();
                    if (self.x - wx).abs() < f.width / 2. + 14.
                        && self.y + 18.
                            > self.course.sample_at(self.x, self.terrain_time).0 - height
                    {
                        danger = Some("Death Wave · jump the low barrier");
                    }
                }
                "lightning" => {
                    if near
                        && (0.62..0.9).contains(&phase)
                        && self.y < self.course.sample_at(self.x, self.terrain_time).0 - 85.
                    {
                        danger = Some("Chain Lightning · stay low during the arc");
                    }
                }
                "rock" => {
                    if self.rock_cooldown > 0. && self.boost == 0. {
                        continue;
                    }
                    let top = rock_top_crossing(f, (old_x, old_y + 18.), (self.x, self.y + 18.));
                    if !rock_contact(f, self.x, self.y + 12.) && top.is_none() {
                        continue;
                    }
                    if self.boost > 0. {
                        f.active = false;
                        self.score += 50;
                        self.banked_score += 50;
                        self.bank_time = 2.;
                        push_trick(&mut self.banked_tricks, "Rock smash", 1, 50);
                        if !self.zen {
                            self.progress.missions.record(&[
                                (progression::Metric::Rocks, 1.),
                                (progression::Metric::Score, 50.),
                            ]);
                        }
                    } else if let Some(top) =
                        top.filter(|_| !self.grounded && self.rail.is_none() && self.wall.is_none())
                    {
                        self.y = top - 18.;
                        // Recorded Felipe bounce: incoming down19.70, outgoing
                        // up25.80 and forward gain5.45 source units. The bounded
                        // impact response is calibrated, not the native equation.
                        self.vy =
                            -(self.vy.max(0.) * 1.3).clamp(26. * WORLD_SCALE, 40. * WORLD_SCALE);
                        self.vx += 5. * WORLD_SCALE;
                        self.wing_on = false;
                        self.air_hop_used = false;
                        self.rock_cooldown = 0.2;
                        self.ground_contact_cooldown = 0.08;
                        self.combo += 1;
                        self.combo_points += 80;
                        self.progress.bounces += 1;
                        push_trick(&mut self.tricks, "Rock bounce", 1, 80);
                    } else if self.feather > 0. || self.feather_grace > 0. {
                        self.feather_grace = 0.6;
                    } else {
                        danger = Some("Enemy collision · jump earlier");
                    }
                }
                _ => {}
            }
        }
        self.poison = if in_swamp {
            self.poison + dt
        } else {
            (self.poison - dt * 1.5).max(0.)
        };
        self.exposure = if in_spot {
            self.exposure + dt
        } else {
            (self.exposure - dt * 2.).max(0.)
        };
        if self.poison > 0.65 {
            danger = Some("Poison exposure · jump or shield");
        }
        if self.exposure > 0.38 {
            danger = Some("Spotlight exposure · jump or shield");
        }
        if let Some(e) = event {
            self.say(e);
        }
        if let Some(d) = danger {
            self.crash(d);
        }
        let g = self.course.sample_at(self.x, self.terrain_time);
        // A helpful hover pickup must not intercept a descending rail approach.
        let rail_below = self.features.iter().any(|f| {
            f.active
                && f.kind == "rail"
                && self.x >= f.x - f.width / 2.
                && self.x <= f.x + f.width / 2.
                && self.y + 18. <= Self::rail_line(f, self.x).0 + 1.
        });
        let contact_hover = if rail_below { 0. } else { self.hover };
        if !self.grounded
            && self.wall.is_none()
            && self.rail.is_none()
            && !self.course.gap(self.x)
            && old_y + 18. + old_hover - old_ground.0 <= 1.
            && self.y + 18. + contact_hover - g.0 >= 0.
            && self.vy - self.vx * g.1 - g.3 > 0.
        {
            let err = (self.angle - g.1.atan() + std::f64::consts::PI).rem_euclid(TAU)
                - std::f64::consts::PI;
            if err.abs() > PROFILES[self.rider].landing_degrees.to_radians() {
                self.bad_landing();
            }
            if self.alive {
                self.grounded = true;
                self.ground_contact_cooldown = 0.08;
                self.wall = None;
                self.y = g.0 - 18. - self.hover;
                self.speed = (self.vx + (self.vy - g.3) * g.1) / (1. + g.1 * g.1).sqrt();
                self.speed = self
                    .speed
                    .clamp(0., PROFILES[self.rider].speed_max * WORLD_SCALE);
                self.bank();
                if self.buffer > 0. {
                    self.jump();
                }
            }
        }
        if self.y > g.0 + 220. {
            let rescue = self.rescues_left > 0 || (!self.zen && self.progress.rescue_picks > 0);
            if rescue {
                if self.rescues_left > 0 {
                    self.rescues_left -= 1;
                } else {
                    self.progress.rescue_picks -= 1;
                }
                self.say("Chasm rescue");
            } else {
                self.crash("Lost in the chasm");
            }
            if self.zen || rescue {
                while self.course.gap(self.x) {
                    self.x += 16.;
                }
                self.y = self.course.sample_at(self.x, self.terrain_time).0 - 18.;
                self.grounded = true;
                self.rail = None;
                self.vy = 0.;
            }
        }
        if let Some(id) = self.wall {
            if !self
                .features
                .iter()
                .any(|f| f.id == id && (self.x - f.x).abs() < f.width / 2. + 20.)
            {
                self.wall = None;
            }
        }
        self.progress.best = self.progress.best.max(self.score);
        self.course.extend(self.x, &mut self.features);
        if self.chase.is_none() && self.x >= self.next_chase {
            let x = self.x - 500.;
            let y = self.course.sample(x).0 - 18.;
            self.chase = Some(Chase {
                started_x: self.x,
                x,
                y,
                previous_x: x,
                previous_y: y,
                speed: PROFILES[self.rider].speed_min * WORLD_SCALE * 1.06,
            });
            self.next_chase = ((self.x / CHASE_INTERVAL).floor() + 1.) * CHASE_INTERVAL;
            self.say("Elder chase · land tricks for speed");
        }
        let mut escaped = false;
        let mut caught = false;
        if let Some(chase) = &mut self.chase {
            // The elder stops at a chasm; a successful crossing ends the pursuit.
            escaped =
                self.course.gaps.iter().any(|&(a, b)| {
                    a >= chase.started_x && self.x >= b && old_x < b && chase.x <= a
                });
            let slope = self.course.sample(chase.x).1;
            // Reconstructed pursuit using recovered catchup3.5, lerp6 and
            // offscreen multiplier3. Follow the unboosted terrain pace, so a
            // banked trick opens a lead instead of being immediately matched.
            let player_slope = self.course.sample(self.x).1;
            let baseline = ground_speed(
                PROFILES[self.rider].speed_min * WORLD_SCALE,
                player_slope,
                100.,
                self.rider,
                0.,
            );
            let stage = (self.x / CHASE_INTERVAL).floor().max(1.);
            let catchup = (3.5 + (stage - 1.) * 0.35).min(6.) * WORLD_SCALE;
            let gap = (self.x - chase.x).max(0.);
            let offscreen = 1. + 2. * ((gap / WORLD_SCALE - 25.) / 50.).clamp(0., 1.);
            let target = (baseline + catchup).max(15. * WORLD_SCALE) * offscreen;
            chase.speed += (target - chase.speed) * (1. - (-6. * h).exp());
            let next = chase.x + chase.speed * h / (1. + slope * slope).sqrt();
            chase.x = self
                .course
                .gaps
                .iter()
                .find(|&&(a, b)| a >= chase.started_x && chase.x < b && next > a)
                .map(|&(a, _)| next.min(a))
                .unwrap_or(next);
            chase.y = self.course.sample(chase.x).0 - 18.;
            caught = !escaped
                && self.rail.is_none()
                && self.wall.is_none()
                && (self.x - chase.x).abs() < 2.5 * WORLD_SCALE
                && (self.y - chase.y).abs() < 90.;
        }
        if escaped {
            self.chase = None;
            self.score += 800;
            if !self.zen {
                self.progress.missions.record(&[
                    (progression::Metric::Elders, 1.),
                    (progression::Metric::Score, 800.),
                ]);
            }
            self.say("Elder escaped · +800");
        } else if caught {
            self.chase = None;
            self.crash("Caught by the elder");
        }
        if !self.zen && self.alive {
            use progression::Metric as M;
            let chasms = if old_rescues == self.rescues_left + self.progress.rescue_picks as u32 {
                self.course
                    .gaps
                    .iter()
                    .filter(|&&(a, b)| old_x >= a - 2000. && old_x < b && self.x >= b)
                    .count() as f64
            } else {
                0.
            };
            let distance = (self.x - old_x).max(0.) / WORLD_SCALE;
            if post_campaign_active {
                self.progress.post_campaign_meters += distance;
            }
            self.progress.missions.record(&[
                (M::Distance, distance),
                (M::Chasms, chasms),
                (M::FlightDistance, if self.wing_on { distance } else { 0. }),
            ]);
        }
        self.rewarded_rails
            .retain(|id| self.features.iter().any(|f| f.id == *id));
        self.rewarded_walls
            .retain(|id| self.features.iter().any(|f| f.id == *id));
        self.rope_states
            .retain(|id, _| self.features.iter().any(|f| f.id == *id));
    }
}
