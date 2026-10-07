//! Project-authored placement grammar. Team Alto describes mixing procedural
//! rules and occasional handmade sequences; these are not its native algorithms.
use super::{terrain::hash, Appearance, Course, Feature, KEEP_BEHIND, LOOK_AHEAD};

const FAMILIES: [u32; 7] = [0, 1, 2, 3, 5, 9, 10];

impl Course {
    pub(super) fn appearance(&self, x: f64) -> Appearance {
        let h = |salt| hash(self.seed, self.next as i64, salt);
        Appearance {
            biome: self.biome_at(x),
            roof_pitch: 0.10 + h(1) * 0.42,
            facade_height: 70. + h(2) * 170.,
            door_offset: (h(3) - 0.5) * 0.65,
            window_spacing: 55. + h(4) * 90.,
            post_offsets: [(h(5) - 0.5) * 60., (h(6) - 0.5) * 60.],
            palette: (h(7) * 5.) as u8,
            motion_amplitude: 22. + h(8) * 48.,
            motion_frequency: 0.32 + h(9) * 0.50,
            motion_phase: h(10) * std::f64::consts::TAU,
            attachments: None,
        }
    }

    fn range(&mut self, min: f64, max: f64) -> f64 {
        min + self.rand() * (max - min)
    }

    fn slope_band(&self, left: f64, right: f64) -> (f64, f64, f64) {
        let mut low = f64::INFINITY;
        let mut high = f64::NEG_INFINITY;
        let mut curvature = 0_f64;
        let count = ((right - left) / 150.).ceil().max(1.) as usize;
        for i in 0..=count {
            let (_, slope, bend) = self.sample(left + (right - left) * i as f64 / count as f64);
            low = low.min(slope);
            high = high.max(slope);
            curvature = curvature.max(bend.abs());
        }
        (low, high, curvature)
    }

    fn family(&mut self, start: f64) -> u32 {
        let biome = self.biome_at(start);
        let local = self.slope_band(start, start + 4200.);
        let extended = self.slope_band(start, start + 15000.);
        // A steep face gets empty approach/runout, regardless of biome or RNG.
        if local.1 > 0.85 || local.0 < -0.16 {
            return 0;
        }
        if self.tension >= 1.0 {
            return 0;
        }
        let gap_eligible = local.0 > 0.12
            && local.1 < 0.68
            && local.2 < 0.00018
            && self.tension < 0.55
            && !self.history.iter().rev().take(2).any(|&k| k == 3)
            && self.chasm_lip(start).is_some();
        // Random weights alone can starve a family for an entire run. Give an
        // overdue canyon a safe available slot, retaining every placement gate.
        if gap_eligible && start - self.last_chasm > 35000. {
            return 3;
        }
        let weights = FAMILIES.map(|family| {
            if self.history.iter().rev().take(2).any(|&k| k == family) {
                return 0.;
            }
            match family {
                0 => 1.0,
                1 => {
                    if self.tension < 0.7 {
                        2.0
                    } else {
                        0.
                    }
                }
                2 => {
                    if biome == 0 {
                        2.8
                    } else {
                        1.7
                    }
                }
                3 => {
                    if gap_eligible {
                        1.5
                    } else {
                        0.
                    }
                }
                5 => {
                    if biome == 2 {
                        3.0
                    } else {
                        1.7
                    }
                }
                9 => {
                    if extended.0 > -0.07 && extended.1 < 0.68 && self.tension < 0.75 {
                        if biome == 1 {
                            3.0
                        } else {
                            1.8
                        }
                    } else {
                        0.
                    }
                }
                10 if local.0 > -0.05 && local.1 < 0.68 => {
                    if biome == 1 {
                        2.8
                    } else {
                        1.2
                    }
                }
                _ => 0.,
            }
        });
        let mut choice = self.rand() * weights.iter().sum::<f64>();
        for (family, weight) in FAMILIES.into_iter().zip(weights) {
            choice -= weight;
            if choice < 0. {
                return family;
            }
        }
        0
    }

    // Find a safe lip in the site rather than rejecting a whole family because
    // one fixed point overlaps the preceding feature's landing corridor.
    fn chasm_lip(&self, start: f64) -> Option<f64> {
        (0..=51)
            .map(|i| start + 1350. + i as f64 * 150.)
            .find(|&lip| {
                let terrain = self.slope_band(lip - 500., lip + 700.);
                terrain.0>0.12 && terrain.1<0.68 && terrain.2<0.00018
                    && self.rock_allowed(lip + 170., 740.)
                    // A light, fast rider needs a real landing runout. Do not
                    // aim a chasm jump at the next near-vertical macro face.
                    && (0..=32).all(|i|self.macro_slope(lip+i as f64*500.)<0.68)
            })
    }

    fn rocks(&mut self, fs: &mut Vec<Feature>, x: f64, count: u32) {
        let mut cx = x;
        let mut spawned = false;
        for _ in 0..count {
            let width = self.range(42., 68.);
            if self.rock_allowed(cx, width) {
                self.feature(fs, "rock", cx, self.sample(cx).0 - 22., width, 0.);
                spawned = true;
            }
            cx += self.range(120., 185.);
        }
        // Rejected hazards must not emit phantom jump corridors that keep
        // rejecting every future hazard indefinitely.
        if spawned {
            self.jump_coins(fs, x - 330., 3);
        }
    }

    fn decor(&mut self, fs: &mut Vec<Feature>, kind: &'static str, x: f64, width: f64) {
        self.feature(fs, kind, x, self.sample(x).0, width, 0.);
    }

    fn clear_rail(&self, fs: &mut [Feature], id: u32, clearance: f64) {
        let rail = fs.iter().find(|f| f.id == id).unwrap();
        let left = rail.x - rail.width / 2.;
        let lift = (0..=128)
            .map(|i| {
                let x = left + rail.width * i as f64 / 128.;
                super::super::Game::rail_line(rail, x).0 - self.sample(x).0 + clearance
            })
            .fold(0_f64, f64::max);
        let rail = fs.iter_mut().find(|f| f.id == id).unwrap();
        rail.y -= lift;
        rail.y2 -= lift;
    }

    fn roof(&mut self, fs: &mut Vec<Feature>, left: f64, width: f64) -> u32 {
        let height = self.range(85., 170.);
        let right = left + width;
        let pitch = self.range(-0.06, 0.06);
        self.feature(
            fs,
            "rail",
            (left + right) / 2.,
            self.sample(left).0 - height,
            width,
            self.sample(right).0 - height + width * pitch,
        );
        let id = self.next;
        let roof = fs.last_mut().unwrap();
        roof.variant = 1;
        if let Some(a) = &mut roof.appearance {
            a.facade_height = height;
        }
        self.clear_rail(fs, id, 85.);
        self.reserve_rail_exit(fs, id);
        id
    }

    fn village(&mut self, fs: &mut Vec<Feature>, start: f64, span: f64) {
        let count = 2 + (self.rand() * 3.) as usize;
        let mut left = start + self.range(850., 1100.);
        let mut previous: Option<(u32, f64, f64)> = None;
        for _ in 0..count {
            let width = self.range(650., 1450.).min(start + span - 700. - left);
            if width < 450. {
                break;
            }
            let roof_id = self.roof(fs, left, width);
            let roof = fs.iter().find(|f| f.id == roof_id).unwrap().clone();
            // Optional connecting edge between distinct house nodes. An absent
            // edge leaves a short jump or the continuous ground bypass.
            if let Some((from, edge, y)) = previous {
                if self.rand() < 0.68 {
                    self.feature(fs, "rail", (edge + left) / 2., y, left - edge, roof.y);
                    let id = self.next;
                    let sag = self.range(12., 35.);
                    let rail = fs.last_mut().unwrap();
                    rail.sag = sag;
                    rail.appearance.as_mut().unwrap().attachments = Some([from, roof_id]);
                    self.clear_rail(fs, id, 74.);
                    self.support_rope(fs, id);
                    self.reserve_rail_exit(fs, id);
                }
            }
            previous = Some((roof_id, left + width, roof.y2));
            if self.rand() < 0.55 {
                self.jump_coins(fs, left - 450., 3);
            }
            let hut_x = left + width * self.range(0.3, 0.7);
            let hut_width = self.range(130., 300.);
            if self.rand() < 0.55 {
                self.decor(fs, "hut", hut_x, hut_width);
            }
            left += width + self.range(260., 630.);
        }
    }

    fn balloons(&mut self, fs: &mut Vec<Feature>, start: f64, span: f64) {
        let count = 2 + (self.rand() * 3.) as usize;
        let mut nodes = vec![];
        for i in 0..count {
            let x = start + 1200. + (span - 2700.) * i as f64 / (count - 1) as f64;
            let width = self.range(600., 850.);
            let top = self.sample(x).0 - self.range(1050., 1450.);
            self.feature(fs, "balloon", x, top, width, top);
            nodes.push((self.next, x, top + width * 0.9));
        }
        for pair in nodes.windows(2) {
            if self.rand() < 0.78 {
                let (a, x, y) = pair[0];
                let (b, x2, y2) = pair[1];
                self.feature(fs, "rail", (x + x2) / 2., y, x2 - x, y2);
                let id = self.next;
                let sag = self.range(35., 100.);
                let rail = fs.last_mut().unwrap();
                rail.variant = 2;
                rail.anchored = false;
                rail.sag = sag;
                rail.appearance.as_mut().unwrap().attachments = Some([a, b]);
                // Clearance includes both endpoints' maximum motion excursion.
                let lift = (0..=64)
                    .map(|i| {
                        let cx = x + (x2 - x) * i as f64 / 64.;
                        super::super::Game::rail_line(rail, cx).0 - self.sample(cx).0 + 260.
                    })
                    .fold(0_f64, f64::max);
                if lift > 0. {
                    for balloon in fs.iter_mut().filter(|f| f.id == a || f.id == b) {
                        balloon.y -= lift;
                        balloon.y2 -= lift;
                    }
                }
                self.balloon_rails.push((id, a, b));
            }
        }
        // A shared balloon may have been lifted by either adjacent cable.
        // Expose exact parent endpoints even before the first movement tick.
        for &(id, a, b) in &self.balloon_rails {
            let (Some(left), Some(right)) =
                (fs.iter().find(|f| f.id == a), fs.iter().find(|f| f.id == b))
            else {
                continue;
            };
            let endpoints = (left.y + left.width * 0.9, right.y + right.width * 0.9);
            if let Some(rail) = fs.iter_mut().find(|f| f.id == id) {
                rail.y = endpoints.0;
                rail.y2 = endpoints.1;
            }
        }
        for &(id, _, _) in &nodes {
            if let Some(balloon) = fs.iter().find(|f| f.id == id) {
                self.reserve_flight_landings(fs, balloon.x, balloon.y - 18., 0., 1.2);
            }
        }
        self.jump_coins(fs, start + 700., 3);
    }

    fn wall_graph(&mut self, fs: &mut Vec<Feature>, start: f64) {
        // Variant selects the visible face profile, not a fixed route recipe.
        let choice = (self.rand() * 3.) as u8;
        let variant = if self.last_wall_route == 99 {
            (self.rand() * 4.) as u8 % 4
        } else if choice >= self.last_wall_route {
            choice + 1
        } else {
            choice
        };
        self.last_wall_route = variant;
        // Retain an occasional measured-in-project transfer motif among the
        // independent expansions, without making it the universal sequence.
        let canonical = self.rand() < 0.18;
        let left = start + self.range(1000., 1400.);
        let width = if canonical {
            4400.
        } else {
            self.range(3000., 4700.)
        };
        let floor = self.sample(left).0;
        let height = if canonical {
            1700.
        } else {
            self.range(1500., 2100.)
        };
        self.feature(
            fs,
            "wall",
            left + width / 2.,
            self.sample(left + width).0 + 100.,
            width,
            floor - height,
        );
        fs.last_mut().unwrap().variant = variant;
        let wall_id = self.next;
        let edge = left + width;
        self.reserve_landing(edge + 1000., edge + 7500., 0.);
        self.jump_coins(fs, left - 300., 3);

        let second_face = !canonical && self.rand() < 0.28;
        let canopy_count = if canonical {
            2
        } else {
            (self.rand() * 4.) as usize
        };
        let cable = canonical || (!second_face && canopy_count == 0) || self.rand() < 0.58;
        let roof = !canonical && self.rand() < 0.48;
        let mut exit_edge = edge;
        if second_face {
            let second_left = edge + self.range(650., 1000.);
            let second_width = self.range(2500., 3500.);
            self.feature(
                fs,
                "wall",
                second_left + second_width / 2.,
                self.sample(second_left + second_width).0 + 100.,
                second_width,
                floor - height - 450.,
            );
            fs.last_mut().unwrap().variant = 4;
            fs.last_mut()
                .unwrap()
                .appearance
                .as_mut()
                .unwrap()
                .attachments = Some([wall_id, wall_id]);
            exit_edge = second_left + second_width;
        }
        // Independently selected canopies connect the descent from this face.
        // One wide canopy covers the two landing windows of a two-node chain.
        let top = floor
            - if canonical || width > 3950. {
                1150.
            } else {
                750.
            };
        for i in 0..canopy_count {
            let offset = if canonical {
                700. + i as f64 * 800.
            } else if canopy_count == 1 {
                1100.
            } else {
                600. + i as f64 * 900.
            };
            let canopy_width = if canonical {
                840.
            } else if canopy_count == 1 {
                1150.
            } else {
                self.range(800., 920.)
            };
            self.feature(fs, "balloon", exit_edge + offset, top, canopy_width, top);
        }
        if cable {
            let left = if canonical {
                edge + 1200.
            } else if second_face {
                exit_edge + 350.
            } else {
                edge + self.range(280., 630.)
            };
            let length = if canonical {
                3800.
            } else {
                self.range(2200., 3100.)
            };
            self.long_rail_with_reservation(
                fs,
                left,
                floor - if canonical { 750. } else { 650. },
                length,
                false,
            );
            let id = self.rail_coins.last().unwrap().0;
            if canonical {
                fs.iter_mut().find(|f| f.id == id).unwrap().y2 = floor - 650.;
            }
            self.clear_rail(fs, id, 74.);
            self.reserve_rail_exit(fs, id);
        }
        if roof {
            let left = exit_edge + self.range(2200., 2700.);
            let width = self.range(1000., 1700.);
            self.roof(fs, left, width);
        }
        for i in 0..(self.rand() * 3.) as usize {
            let x = left + width * (i as f64 + 1.) / 4.;
            let w = self.range(100., 260.);
            self.decor(fs, "ruin", x, w);
        }
    }
    fn opening(&mut self, fs: &mut Vec<Feature>, family: u32, span: f64) {
        self.ground_group(fs, 350.);
        // Reserve both ground rewards before spending the shared route budget
        // on optional elevated coins.
        self.ground_group(fs, span - 650.);
        match family {
            0 | 2 => {
                let width = self.range(4700., 5000.);
                self.long_rail(fs, 2200., self.sample(2200.).0 - 90., width);
                self.jump_coins(fs, 1600., 3);
                self.rocks(fs, 3400., 2);
            }
            5 => {
                self.village(fs, 0., span);
                self.rocks(fs, 1250., 2);
            }
            _ => {
                self.rocks(fs, 1250., 2);
                self.rocks(fs, 3800., 2);
            }
        }
        self.ground_group(fs, span - 650.);
    }

    fn decorations(&mut self, fs: &mut Vec<Feature>, start: f64, span: f64) {
        let biome = self.biome_at(start);
        let count = if biome == 0 {
            3 + (self.rand() * 5.) as usize
        } else {
            1 + (self.rand() * 3.) as usize
        };
        for i in 0..count {
            let x = start + span * (i as f64 + self.range(0.2, 0.8)) / count as f64;
            let kind = if biome == 0 {
                "pine"
            } else if biome == 1 {
                "ruin"
            } else {
                "hut"
            };
            let width = if biome == 0 {
                self.range(60., 140.)
            } else {
                self.range(100., 340.)
            };
            self.decor(fs, kind, x, width);
        }
    }

    pub fn extend(&mut self, x: f64, fs: &mut Vec<Feature>) {
        self.extend_terrain(x);
        let previous_site = self.next_site;
        while self.next_site < x + LOOK_AHEAD {
            let start = self.next_site;
            let opening = start == 0.;
            let family = if opening {
                [0, 1, 2, 5][(self.rand() * 4.) as usize % 4]
            } else {
                self.family(start)
            };
            let span = if opening {
                7500.
            } else {
                match family {
                    9 => 14500.,
                    5 => self.range(5000., 7600.),
                    2 | 10 => self.range(5000., 7400.),
                    3 => {
                        let end = self.chasm_lip(start).unwrap() - start + 1600.;
                        self.range(4300., 5500.).max(end)
                    }
                    1 => {
                        let hazard = self.next_hazard_site(start + 1300.);
                        self.range(3300., 4500.).max(hazard - start + 3000.)
                    }
                    _ => self.range(2000., 3400.),
                }
            };
            self.next_site += span;
            // At most 72 spawned coin-value per kilometre across ALL routes,
            // plus a small opening allowance. Unused allowance is bounded.
            self.coin_budget = (self.coin_budget + span * 0.0018).min(28.);
            if self
                .encounters
                .last()
                .is_none_or(|&(_, previous)| previous != family)
            {
                self.encounters.push((start, family));
            }
            self.history.push(family);
            if self.history.len() > 4 {
                self.history.remove(0);
            }
            if opening {
                self.opening(fs, family, span);
            } else {
                self.ground_group(fs, start + 350.);
                match family {
                    1 => {
                        let hazard = self.next_hazard_site(start + 1300.);
                        // Keep the setup inside this site's span and generate
                        // actual macro samples before forecasting its jump.
                        self.extend_terrain(hazard);
                        let count = 2 + (self.rand() * 2.) as u32;
                        self.rocks(fs, hazard, count);
                        self.rocks(fs, start + span - 900., count);
                        if self.rand() < 0.45 && self.biome_at(start) == 0 {
                            let width = self.range(500., 1000.);
                            self.feature(
                                fs,
                                "ice",
                                start + span - 700.,
                                self.sample(start + span - 700.).0,
                                width,
                                0.,
                            );
                        }
                    }
                    2 => {
                        let left = start + self.range(1050., 1300.);
                        let width = span - (left - start) - 650.;
                        self.long_rail(fs, left, self.sample(left).0 - 95., width);
                        self.jump_coins(fs, left - 600., 3);
                        if self.rand() < 0.6 {
                            self.rocks(fs, left + 1200., 2);
                        }
                    }
                    3 => {
                        let lip = self
                            .chasm_lip(start)
                            .expect("selected chasm has a safe lip");
                        self.last_chasm = lip;
                        let width = self.range(240., 340.);
                        self.gaps.push((lip, lip + width));
                        self.jump_coins(fs, lip - 180., 3);
                        if self.rand() < 0.45 {
                            self.long_rail(
                                fs,
                                lip - 650.,
                                self.sample(lip - 650.).0 - 100.,
                                width + 1600.,
                            );
                        }
                    }
                    5 => self.village(fs, start, span),
                    9 => self.wall_graph(fs, start),
                    10 => self.balloons(fs, start, span),
                    _ => {}
                }
                self.ground_group(fs, start + span - 600.);
            }
            self.decorations(fs, start, span);
            self.tension = if family == 0 {
                (self.tension - 0.85).max(0.)
            } else {
                self.tension
                    + match family {
                        3 => 0.8,
                        9 => 0.65,
                        1 => 0.55,
                        _ => 0.3,
                    }
            };
            if opening || (family == 0 && self.rand() < 0.24) {
                let cx = start + span - 300.;
                let pickup = ["magnet", "feather", "shield"][(self.rand() * 3.) as usize % 3];
                self.feature(fs, pickup, cx, self.sample(cx).0 - 40., 30., 0.);
            }
        }
        self.segments.retain(|s| s.x1 > x - KEEP_BEHIND);
        self.gaps.retain(|&(_, b)| b > x - 2200.);
        self.encounters.retain(|&(a, _)| a > x - 20000.);
        if previous_site != self.next_site || x >= self.next_prune {
            self.sync_static_supports(fs);
            self.prune_features(x, fs);
            self.next_prune = x + 500.;
        }
        self.balloon_rails
            .retain(|(id, _, _)| fs.iter().any(|f| f.id == *id));
        self.rail_coins
            .retain(|(id, _)| fs.iter().any(|f| f.id == *id));
    }
}
