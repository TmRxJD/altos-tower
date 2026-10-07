use super::*;

#[test]
fn protected_landings_do_not_starve_canyons_across_run_seeds() {
    for seed in [1, 2, 3, 4, 5, 6, 7, 8, 42] {
        let mut c = course::Course::new(seed);
        let mut fs = vec![];
        let mut gaps = std::collections::BTreeSet::new();
        for x in (0..300000).step_by(4000) {
            c.extend(x as f64, &mut fs);
            gaps.extend(c.gaps.iter().map(|&(left, _)| left as u32));
        }
        assert!(
            gaps.len() >= 3,
            "seed={seed} generated only{} canyons in7.5km",
            gaps.len()
        );
    }
}

#[test]
fn terrain_is_predominantly_moderate_downhill_with_only_short_rare_crests() {
    let mut total = 0_usize;
    let mut moderate = 0_usize;
    let mut uphill = 0_usize;
    let mut flat = 0_usize;
    let mut longest_uphill = 0;
    for seed in 1..=16 {
        let mut c = course::Course::new(seed);
        let mut fs = vec![];
        let mut run = 0;
        for x in (0..200000).step_by(100) {
            c.extend(x as f64, &mut fs);
            let slope = c.sample(x as f64).1;
            total += 1;
            moderate += usize::from((0.24..=0.66).contains(&slope));
            uphill += usize::from(slope < 0.);
            flat += usize::from(slope.abs() < 0.15);
            run = if slope < 0. { run + 100 } else { 0 };
            longest_uphill = longest_uphill.max(run);
        }
    }
    assert!(
        moderate as f64 / total as f64 > 0.85,
        "moderate={moderate}/{total}"
    );
    assert!(
        (uphill as f64 / total as f64) < 0.01,
        "uphill={uphill}/{total}"
    );
    assert!((flat as f64 / total as f64) < 0.02, "flat={flat}/{total}");
    assert!(longest_uphill <= 800, "uphill run={longest_uphill}");
}

#[test]
fn rope_endpoints_keep_exact_visible_parents_through_motion_and_pruning() {
    let mut checked = [0_usize; 2];
    let mut retained_trailing_parents = 0;
    for seed in 1..=8 {
        let mut c = course::Course::new(seed);
        let mut fs = vec![];
        for x in (0..160000).step_by(900) {
            c.extend(x as f64, &mut fs);
            for time in [0., 0.7, 2.9, 7.4] {
                c.anchor_features(time, &mut fs);
                for rail in fs.iter().filter(|f| f.kind == "rail" && f.variant != 1) {
                    let parents = rail
                        .appearance
                        .as_ref()
                        .and_then(|a| a.attachments)
                        .expect("every rope requires explicit visible endpoint parents");
                    for (index, id) in parents.into_iter().enumerate() {
                        let parent = fs
                            .iter()
                            .find(|f| f.id == id)
                            .expect("parent pruned before its rope");
                        assert!(matches!(parent.kind, "post" | "balloon"));
                        let px = rail.x
                            + if index == 0 {
                                -rail.width / 2.
                            } else {
                                rail.width / 2.
                            };
                        let py = if index == 0 { rail.y } else { rail.y2 };
                        let parent_y = parent.y
                            + if parent.kind == "balloon" {
                                parent.width * 0.9
                            } else {
                                0.
                            };
                        assert!((px - parent.x).abs() < 1e-7 && (py - parent_y).abs() < 1e-7,
                            "seed={seed} x={x} rail={} parent={} t={time} endpoint=({px},{py}) parent=({},{parent_y})", rail.id, id, parent.x);
                        if parent.x + parent.width / 2. <= x as f64 - 2200. {
                            retained_trailing_parents += 1;
                        }
                    }
                    checked[usize::from(rail.variant == 2)] += 1;
                }
            }
        }
    }
    assert!(checked.into_iter().all(|n| n > 100));
    assert!(retained_trailing_parents > 100);
}

#[test]
fn normal_rock_jumps_land_safely_across_profiles_and_speed_extremes() {
    for seed in [1, 7, 19] {
        let mut base = Game::new("{}");
        base.start_seeded(false, seed);
        let rock = base
            .features
            .iter()
            .find(|f| f.kind == "rock")
            .unwrap()
            .clone();
        let launch = rock.x - 330.;
        for (rider, profile) in PROFILES.iter().enumerate() {
            for speed in [profile.speed_min, profile.speed_max] {
                let mut g = base.clone();
                g.rider = rider;
                g.x = launch;
                let (y, slope, _) = g.course.sample(launch);
                g.y = y - 18.;
                g.angle = slope.atan();
                g.speed = speed * WORLD_SCALE;
                g.vx = g.speed / (1. + slope * slope).sqrt();
                g.vy = g.vx * slope;
                g.next_chase = f64::INFINITY;
                g.press();
                g.release();
                let mut landed = false;
                for _ in 0..960 {
                    g.tick(1. / 120.);
                    assert!(
                        g.alive,
                        "seed={seed} rider={rider} speed={speed} x={} {}",
                        g.x, g.notice
                    );
                    if g.grounded || g.rail.is_some() {
                        landed = true;
                        break;
                    }
                }
                assert!(
                    landed,
                    "seed={seed} rider={rider} speed={speed}: no supported landing"
                );
                for _ in 0..30 {
                    g.tick(1. / 120.);
                    assert!(
                        g.alive,
                        "landing runout seed={seed} rider={rider} speed={speed} x={} {}",
                        g.x, g.notice
                    );
                }
            }
        }
    }
}

#[test]
fn feature_grammars_vary_dimensions_attachments_and_composition() {
    use std::collections::{BTreeMap, BTreeSet};
    let mut dimensions: BTreeMap<&str, BTreeSet<(i64, i64, i64)>> = BTreeMap::new();
    let mut village_shapes = BTreeSet::new();
    let mut balloon_shapes = BTreeSet::new();
    let mut biomes = BTreeSet::new();
    for seed in 1..=24 {
        let mut c = course::Course::new(seed);
        let mut fs = vec![];
        let mut seen = BTreeSet::new();
        for x in (0..180000).step_by(4000) {
            c.extend(x as f64, &mut fs);
            for f in &fs {
                if !seen.insert(f.id) {
                    continue;
                }
                if matches!(f.kind, "rail" | "wall" | "balloon" | "hut" | "ruin") {
                    let a = f.appearance.as_ref().unwrap();
                    biomes.insert(a.biome);
                    assert_eq!(a.biome, c.biome_at(f.x));
                    dimensions.entry(f.kind).or_default().insert((
                        f.width.round() as i64,
                        (a.roof_pitch * 100.).round() as i64,
                        a.facade_height.round() as i64,
                    ));
                }
            }
            for &(start, family) in &c.encounters {
                let end = c
                    .encounters
                    .iter()
                    .find(|&&(at, _)| at > start)
                    .map(|e| e.0)
                    .unwrap_or(start + 7400.);
                let group: Vec<_> = fs.iter().filter(|f| f.x >= start && f.x < end).collect();
                let roofs = group
                    .iter()
                    .filter(|f| f.kind == "rail" && f.variant == 1)
                    .count();
                let attachments = group
                    .iter()
                    .filter(|f| {
                        f.kind == "rail"
                            && f.appearance
                                .as_ref()
                                .is_some_and(|a| a.attachments.is_some())
                    })
                    .count();
                if family == 5 {
                    village_shapes.insert((
                        roofs,
                        attachments,
                        group.iter().filter(|f| f.kind == "hut").count(),
                    ));
                }
                if family == 10 {
                    balloon_shapes.insert((
                        group.iter().filter(|f| f.kind == "balloon").count(),
                        attachments,
                    ));
                }
            }
        }
    }
    for kind in ["rail", "wall", "balloon", "hut", "ruin"] {
        assert!(
            dimensions[kind].len() >= 20,
            "{kind}: only {} distinct dimension sets",
            dimensions[kind].len()
        );
    }
    assert!(
        village_shapes.len() >= 8,
        "village structures={village_shapes:?}"
    );
    assert!(
        balloon_shapes.len() >= 5,
        "balloon structures={balloon_shapes:?}"
    );
    assert_eq!(biomes.len(), 3);
}

#[test]
fn grammar_rhythm_has_recovery_and_no_immediate_family_repetition() {
    use std::collections::{BTreeMap, BTreeSet};
    let mut families = BTreeSet::new();
    for seed in 1..=12 {
        let mut c = course::Course::new(seed);
        let mut fs = vec![];
        let mut seen = BTreeMap::new();
        for x in (0..300000).step_by(5000) {
            c.extend(x as f64, &mut fs);
            seen.extend(
                c.encounters
                    .iter()
                    .map(|&(x, family)| (x.round() as i64, family)),
            );
        }
        let sequence: Vec<_> = seen.values().copied().collect();
        assert!(sequence.windows(2).all(|p| p[0] != p[1]));
        let mut busy = 0;
        for family in sequence {
            families.insert(family);
            busy = if family == 0 { 0 } else { busy + 1 };
            assert!(
                busy <= 4,
                "seed={seed}: {busy} busy sequences without relief"
            );
        }
    }
    assert_eq!(families, [0, 1, 2, 3, 5, 9, 10].into_iter().collect());
}

#[test]
fn each_wall_profile_expands_to_many_independent_component_graphs() {
    use std::collections::{BTreeMap, BTreeSet};
    let mut shapes: BTreeMap<u8, BTreeSet<(usize, usize, usize, usize)>> = BTreeMap::new();
    for seed in 1..=40 {
        let mut c = course::Course::new(seed);
        let mut fs = vec![];
        let mut seen = BTreeSet::new();
        for x in (0..360000).step_by(5000) {
            c.extend(x as f64, &mut fs);
            for wall in fs.iter().filter(|f| f.kind == "wall" && f.variant < 4) {
                if !seen.insert(wall.id) {
                    continue;
                }
                let start = c
                    .encounters
                    .iter()
                    .rev()
                    .find(|&&(at, family)| at <= wall.x && family == 9)
                    .unwrap()
                    .0;
                let end = start + 14500.;
                let nodes: Vec<_> = fs.iter().filter(|f| f.x >= start && f.x < end).collect();
                shapes.entry(wall.variant).or_default().insert((
                    nodes.iter().filter(|f| f.kind == "balloon").count(),
                    nodes
                        .iter()
                        .filter(|f| f.kind == "rail" && f.variant != 1)
                        .count(),
                    nodes
                        .iter()
                        .filter(|f| f.kind == "rail" && f.variant == 1)
                        .count(),
                    nodes
                        .iter()
                        .filter(|f| f.kind == "wall" && f.variant == 4)
                        .count(),
                ));
            }
        }
    }
    assert_eq!(shapes.len(), 4);
    for (profile, structures) in shapes {
        assert!(
            structures.len() >= 8,
            "profile{profile}: only {} structural graphs: {structures:?}",
            structures.len()
        );
        assert_eq!(
            structures.iter().map(|s| s.0).collect::<BTreeSet<_>>(),
            [0, 1, 2, 3].into_iter().collect()
        );
        assert!(structures.iter().any(|s| s.1 == 0) && structures.iter().any(|s| s.1 > 0));
        assert!(structures.iter().any(|s| s.3 == 0) && structures.iter().any(|s| s.3 > 0));
    }
}

#[test]
fn all_route_currency_stays_inside_sparse_seeded_economy_budget() {
    let mut densities = vec![];
    for seed in 1..=24 {
        let mut c = course::Course::new(seed);
        let mut fs = vec![];
        let mut values = std::collections::BTreeMap::new();
        let length = 200000.; // 5 km; includes every optional route, not collected coins.
        for x in (0..200000).step_by(2000) {
            c.extend(x as f64, &mut fs);
            for f in fs
                .iter()
                .filter(|f| f.kind == "coin" && f.x >= 0. && f.x < length)
            {
                values.insert(f.id, if f.variant == 1 { 10_u32 } else { 1 });
            }
        }
        let density = values.values().sum::<u32>() as f64 / (length / WORLD_SCALE / 1000.);
        assert!(
            (40. ..=100.).contains(&density),
            "seed={seed}: {density} coin-value/km"
        );
        densities.push(density);
    }
    densities.sort_by(f64::total_cmp);
    eprintln!("All-route coin-value/km over 24 seeds × 5km: median={:.1}, p95={:.1}, min={:.1}, max={:.1}",
        densities[12], densities[22], densities[0], densities[23]);
}

#[test]
fn generated_cables_clear_terrain_through_moving_attachment_cycles() {
    for seed in 1..=12 {
        let mut c = course::Course::new(seed);
        let mut fs = vec![];
        for x in (0..160000).step_by(6000) {
            c.extend(x as f64, &mut fs);
            for time in [0., 1.7, 4.3, 8.5, 14.] {
                c.anchor_features(time, &mut fs);
                for rail in fs.iter().filter(|f| f.kind == "rail") {
                    let left = rail.x - rail.width / 2.;
                    // Retained long rails may extend beyond retained terrain;
                    // check the live section using actual segment coverage.
                    for i in 0..=64 {
                        let x = left + rail.width * i as f64 / 64.;
                        if x < c.segments[0].x0 {
                            continue;
                        }
                        assert!(
                            Game::rail_line(rail, x).0 <= c.sample(x).0 - 70.,
                            "seed={seed} rail={} variant={} time={time} x={x}",
                            rail.id,
                            rail.variant
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn macro_peaks_have_clear_approaches_and_runouts() {
    let mut faces = 0;
    for seed in 1..=8 {
        let mut c = course::Course::new(seed);
        let mut fs = vec![];
        for x in (0..160000).step_by(5000) {
            c.extend(x as f64, &mut fs);
            for segment in &c.segments {
                if segment.m0.max(segment.m1) < 1.8 || segment.x0 > x as f64 + 6000. {
                    continue;
                }
                faces += 1;
                assert!(segment.m0.max(segment.m1) <= 2.4);
                assert!(!c
                    .gaps
                    .iter()
                    .any(|&(a, b)| a < segment.x1 + 1200. && b > segment.x0 - 1000.));
                assert!(!fs.iter().any(|f| f.kind == "rock"
                    && f.x > segment.x0 - 1000.
                    && f.x < segment.x1 + 1200.));
            }
        }
    }
    assert!(faces >= 16);
}

#[test]
fn generated_macro_steep_faces_allow_a_real_unassisted_runout() {
    for seed in 1..=4 {
        let mut g = Game::new("{}");
        g.start_seeded(false, seed);
        g.course.extend(10000., &mut g.features);
        let face = g
            .course
            .segments
            .iter()
            .find(|s| s.m0.max(s.m1) > 1.8)
            .unwrap()
            .x0;
        g.x = face - 2200.;
        let (y, slope, _) = g.course.sample(g.x);
        g.y = y - 18.;
        g.angle = slope.atan();
        g.speed = PROFILES[0].speed_min * WORLD_SCALE;
        g.vx = g.speed / (1. + slope * slope).sqrt();
        g.vy = g.vx * slope;
        g.next_chase = f64::INFINITY;
        let end = face + 5000.;
        for _ in 0..1800 {
            g.tick(1. / 120.);
            assert!(
                g.alive,
                "seed={seed} x={} angle={} {}",
                g.x, g.angle, g.notice
            );
            if g.x >= end {
                break;
            }
        }
        assert!(g.x >= end, "seed={seed} x={} end={end}", g.x);
    }
}

#[test]
fn seeded_openings_collect_first_ground_group_without_jumping() {
    for seed in 1..=32 {
        let mut g = Game::new("{}");
        g.start_seeded(false, seed);
        while g.x < 780. {
            g.tick(1. / 120.);
            assert!(g.alive && g.grounded, "seed={seed} x={} {}", g.x, g.notice);
        }
        assert!(g.coins >= 3, "seed={seed} collected={}", g.coins);
    }
}

#[test]
fn ground_currency_is_static_and_avoids_hazard_approaches() {
    for seed in 1..=32 {
        let mut g = Game::new("{}");
        g.start_seeded(false, seed);
        let ground: Vec<_> = g
            .features
            .iter()
            .filter(|f| {
                f.kind == "coin" && f.anchored && (g.course.sample(f.x).0 - f.y - 34.).abs() < 1.
            })
            .map(|f| (f.id, f.x, f.y))
            .collect();
        let all = g.features.iter().filter(|f| f.kind == "coin").count();
        assert!(ground.len() >= 8, "seed={seed} ground={}", ground.len());
        let opening_coins = g
            .features
            .iter()
            .filter(|f| f.kind == "coin" && f.x < 7500.)
            .count();
        assert!(
            opening_coins <= 40,
            "seed={seed} opening={opening_coins} all={all}"
        );
        let mut positions: Vec<_> = ground
            .iter()
            .filter(|(_, x, _)| *x < 7500.)
            .map(|(_, x, _)| *x)
            .collect();
        positions.sort_by(f64::total_cmp);
        assert!(
            positions.windows(2).any(|p| p[1] - p[0] > 1000.),
            "seed={seed}: ground groups need empty stretches; positions={positions:?} encounters={:?}", g.course.encounters
        );
        for &(_, x, _) in &ground {
            assert!(!g.course.gap(x));
            assert!(!g
                .features
                .iter()
                .any(|f| f.kind == "rock" && (x - f.x).abs() < f.width / 2. + 70.));
        }
        g.tick(1. / 120.);
        for (id, x, y) in ground {
            let f = g.features.iter().find(|f| f.id == id).unwrap();
            assert_eq!((f.x, f.y), (x, y));
        }
    }
}

#[test]
fn opening_cables_support_a_quarter_second_launch_window_and_long_grind() {
    let seeds: Vec<_> = (1..=64)
        .filter(|&seed| {
            let mut g = Game::new("{}");
            g.start_seeded(false, seed);
            matches!(g.course.encounters[0].1, 0 | 2)
        })
        .take(12)
        .collect();
    assert_eq!(seeds.len(), 12);
    for seed in seeds {
        for distance in [450., 600., 750.] {
            let mut g = Game::new("{}");
            g.start_seeded(false, seed);
            let rail = g
                .features
                .iter()
                .filter(|f| f.kind == "rail")
                .min_by(|a, b| (a.x - a.width / 2.).total_cmp(&(b.x - b.width / 2.)))
                .unwrap()
                .clone();
            let launch = rail.x - rail.width / 2. - distance;
            while g.x < launch {
                g.tick(1. / 120.);
                assert!(g.alive);
            }
            g.press();
            g.release();
            for _ in 0..220 {
                g.tick(1. / 120.);
                if g.rail == Some(rail.id) || !g.alive {
                    break;
                }
            }
            assert!(
                g.alive && g.rail == Some(rail.id),
                "seed={seed} launch={distance} x={} y={} {}",
                g.x,
                g.y,
                g.notice
            );
            for _ in 0..300 {
                g.tick(1. / 120.);
            }
            assert!(
                g.alive && g.rail == Some(rail.id),
                "short grind seed={seed} launch={distance} x={} {}",
                g.x,
                g.notice
            );
            for _ in 0..100 {
                g.tick(1. / 120.);
            }
            assert!(
                g.rope_states
                    .get(&rail.id)
                    .is_some_and(|r| r.broken_at.is_some()),
                "rope must break after3s seed={seed}"
            );
            for _ in 0..180 {
                g.tick(1. / 120.);
                assert!(
                    g.alive,
                    "unsafe rope break seed={seed} x={} {}",
                    g.x, g.notice
                );
                if g.grounded {
                    break;
                }
            }
            assert!(
                g.grounded,
                "broken rope must have a safe ground landing seed={seed}"
            );
        }
    }
}

fn wall_run(launch_distance: f64) -> (Game, Feature) {
    let mut game = (1..=4096)
        .map(|seed| {
            let mut g = Game::new("{}");
            g.start_seeded(false, seed);
            g
        })
        .find(|g| {
            g.course.encounters[0].1 == 0
                && g.features.iter().any(|wall| {
                    let edge = wall.x + wall.width / 2.;
                    wall.kind == "wall"
                        && wall.variant == 0
                        && (wall.width - 4400.).abs() < 0.01
                        && g.features.iter().any(|f| {
                            f.kind == "rail" && ((f.x - f.width / 2.) - edge - 1200.).abs() < 0.01
                        })
                })
        })
        .expect("seeded opening followed by wall route");
    let wall = game
        .features
        .iter()
        .find(|f| f.kind == "wall" && f.variant == 0)
        .unwrap()
        .clone();
    let left = wall.x - wall.width / 2.;
    for _ in 0..3000 {
        if game.x >= left - launch_distance {
            break;
        }
        let rock_approach = game
            .features
            .iter()
            .any(|f| f.active && f.kind == "rock" && f.x > game.x && f.x - game.x < 400.);
        if rock_approach && game.grounded {
            game.press();
            game.release();
        }
        game.tick(1. / 120.);
        assert!(
            game.alive,
            "opening seed={} x={} {}",
            game.seed, game.x, game.notice
        );
    }
    assert!(game.x >= left - launch_distance && game.x < left);
    if game.rail.is_some() {
        game.press();
        game.release();
    }
    (game, wall)
}

#[test]
fn generated_wall_route_climbs_and_chains_across_broad_input_windows() {
    for launch_distance in [200., 300., 400.] {
        for release_distance in [400., 500., 600.] {
            let (mut g, wall) = wall_run(launch_distance);
            let exit_rail = g
                .features
                .iter()
                .find(|f| {
                    f.kind == "rail"
                        && ((f.x - f.width / 2.) - (wall.x + wall.width / 2. + 1200.)).abs() < 1.
                })
                .unwrap()
                .id;
            let entry_y = g.y;
            g.press();
            let mut attached = false;
            for _ in 0..1200 {
                g.tick(1. / 120.);
                attached |= g.wall == Some(wall.id);
                if g.x >= wall.x + wall.width / 2. - release_distance {
                    break;
                }
                assert!(g.alive, "wall x={} y={} {}", g.x, g.y, g.notice);
            }
            assert!(attached && g.y < entry_y - 200.,
                "launch={launch_distance} release={release_distance} x={} y={} entry={entry_y} wall={:?}",g.x,g.y,g.wall);
            g.release();
            for _ in 0..1000 {
                g.tick(1. / 120.);
                if g.tricks.iter().any(|t| t.name == "Balloon bounce") && g.rail == Some(exit_rail)
                {
                    break;
                }
                if g.grounded || !g.alive {
                    break;
                }
            }
            assert!(
                g.alive
                    && g.tricks.iter().any(|t| t.name == "Balloon bounce")
                    && g.rail == Some(exit_rail),
                "launch={launch_distance} release={release_distance} x={} y={} {} tricks={:?}",
                g.x,
                g.y,
                g.notice,
                g.tricks.iter().map(|t| t.name).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn seeded_walls_have_distinct_topologies_and_all_first_faces_are_rideable() {
    use std::collections::BTreeSet;
    let mut variants = BTreeSet::new();
    let mut dimensions = BTreeSet::new();
    let mut checked = [false; 4];
    for seed in 1..=512 {
        let mut g = Game::new("{}");
        g.start_seeded(false, seed);
        let Some(wall) = g
            .features
            .iter()
            .find(|f| f.kind == "wall" && f.variant < 4)
            .cloned()
        else {
            continue;
        };
        variants.insert(wall.variant);
        dimensions.insert((wall.width.round() as i32, (wall.y - wall.y2).round() as i32));
        let edge = wall.x + wall.width / 2.;
        assert!(
            g.features
                .iter()
                .any(|f| f.x > edge && matches!(f.kind, "wall" | "rail" | "balloon")),
            "wall has no optional aerial exit"
        );
        if !checked[wall.variant as usize] {
            g.x = wall.x - wall.width / 2. - 100.;
            g.y = g.course.sample(g.x).0 - 18.;
            g.vx = 800.;
            let entry_y = g.y;
            g.press();
            for _ in 0..96 {
                g.tick(1. / 120.);
            }
            assert!(
                g.alive && g.wall == Some(wall.id) && g.y < entry_y - 200.,
                "route={} seed={seed} {}",
                wall.variant,
                g.notice
            );
            checked[wall.variant as usize] = true;
        }
    }
    assert_eq!(variants.len(), 4);
    assert!(
        dimensions.len() > 20,
        "only {} face dimensions",
        dimensions.len()
    );
    assert!(checked.into_iter().all(|v| v));
}

#[test]
fn every_wall_topology_has_a_playable_airborne_transfer_across_multiple_seeds() {
    let mut checked = [0_u8; 4];
    let mut structures: [std::collections::BTreeSet<(usize, usize, usize)>; 4] =
        std::array::from_fn(|_| std::collections::BTreeSet::new());
    for seed in 1..=4096 {
        let mut base = Game::new("{}");
        base.start_seeded(false, seed);
        let Some(wall) = base
            .features
            .iter()
            .find(|f| f.kind == "wall" && f.variant < 4)
            .cloned()
        else {
            continue;
        };
        let route = wall.variant as usize;
        if checked[route] >= 8 {
            continue;
        }
        let edge = wall.x + wall.width / 2.;
        let start = base
            .course
            .encounters
            .iter()
            .rev()
            .find(|&&(at, family)| at <= wall.x && family == 9)
            .unwrap()
            .0;
        let nodes: Vec<_> = base
            .features
            .iter()
            .filter(|f| f.x >= start && f.x < start + 14500.)
            .collect();
        let signature = (
            nodes.iter().filter(|f| f.kind == "balloon").count(),
            nodes.iter().filter(|f| f.kind == "rail").count(),
            nodes
                .iter()
                .filter(|f| f.kind == "wall" && f.variant == 4)
                .count(),
        );
        if structures[route].contains(&signature) {
            continue;
        }
        for rider in [0, 5] {
            let mut transfer = false;
            for release_distance in [0., 200., 400., 600., 800., 1000.] {
                let mut g = base.clone();
                g.rider = rider;
                g.x = wall.x - wall.width / 2. - 100.;
                g.y = g.course.sample(g.x).0 - 18.;
                g.speed = PROFILES[rider].speed_min * WORLD_SCALE;
                g.vx = g.speed;
                g.press();
                let mut attached = false;
                for _ in 0..1200 {
                    g.tick(1. / 120.);
                    attached |= g.wall == Some(wall.id);
                    if g.x >= edge - release_distance || !g.alive {
                        break;
                    }
                }
                if !attached || !g.alive {
                    continue;
                }
                g.release();
                // Re-hold on the second face; all other routes use release to
                // catch their rail/canopy without an automatic input assistant.
                if g.features.iter().any(|f| {
                    f.kind == "wall"
                        && f.variant == 4
                        && f.x - f.width / 2. > edge
                        && f.x - f.width / 2. < edge + 1200.
                }) {
                    g.press();
                }
                for _ in 0..1200 {
                    g.tick(1. / 120.);
                    let rail_exit = g.rail.is_some_and(|id| {
                        g.features
                            .iter()
                            .any(|f| f.id == id && f.x - f.width / 2. > edge)
                    });
                    let second_face = g.wall.is_some_and(|id| id != wall.id);
                    let balloon_exit = g.tricks.iter().any(|t| t.name == "Balloon bounce");
                    if rail_exit || second_face || balloon_exit {
                        transfer = true;
                        break;
                    }
                    if g.grounded || !g.alive {
                        break;
                    }
                }
                if transfer {
                    break;
                }
            }
            assert!(
                transfer,
                "route={route} seed={seed} rider={rider} width={} has no tested transfer",
                wall.width
            );
        }
        structures[route].insert(signature);
        checked[route] += 1;
        if checked.into_iter().all(|n| n == 8) {
            break;
        }
    }
    assert_eq!(checked, [8; 4]);
}

#[test]
fn successive_wall_encounters_do_not_repeat_the_same_topology() {
    let mut course = course::Course::new(419);
    let mut features = vec![];
    let mut seen = std::collections::BTreeSet::new();
    let mut previous = None;
    let mut count = 0;
    for x in (0..2400000).step_by(1000) {
        course.extend(x as f64, &mut features);
        for wall in features
            .iter()
            .filter(|f| f.kind == "wall" && f.variant < 4)
        {
            if !seen.insert(wall.id) {
                continue;
            }
            assert_ne!(
                previous,
                Some(wall.variant),
                "same wall topology twice at x={}",
                wall.x
            );
            previous = Some(wall.variant);
            count += 1;
        }
    }
    assert!(count >= 8, "only {count} wall routes checked");
}
