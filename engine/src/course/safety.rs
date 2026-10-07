use super::{air_step, Course, Feature, PROFILES, WORLD_SCALE};

#[derive(Clone, Debug)]
pub(super) struct LandingCorridor {
    pub left: f64,
    pub right: f64,
    /// Contact surface height above terrain. Upper routes do not ban rocks
    /// underneath a safely supported cable.
    pub height: f64,
}

impl Course {
    pub(super) fn reserve_landing(&mut self, mut left: f64, mut right: f64, height: f64) {
        self.landings.retain(|band| {
            if (band.height - height).abs() < 10. && band.left <= right && band.right >= left {
                left = left.min(band.left);
                right = right.max(band.right);
                false
            } else {
                true
            }
        });
        self.landings.push(LandingCorridor {
            left,
            right,
            height,
        });
    }

    pub(super) fn rock_allowed(&self, x: f64, width: f64) -> bool {
        !self.landings.iter().any(|band| {
            band.height < 65. && x + width / 2. > band.left && x - width / 2. < band.right
        }) && !self.gaps.iter().any(|&(a, b)| x > a - 400. && x < b + 900.)
    }

    pub(super) fn next_hazard_site(&self, mut x: f64) -> f64 {
        for _ in 0..128 {
            let old = x;
            for band in &self.landings {
                if band.height < 65. && x + 500. > band.left && x - 500. < band.right {
                    x = x.max(band.right + 600.);
                }
            }
            for &(a, b) in &self.gaps {
                if x > a - 900. && x < b + 1500. {
                    x = b + 1600.;
                }
            }
            if (0..=8).any(|i| self.macro_slope(x - 350. + i as f64 * 250.) > 0.68) {
                x += 500.;
            }
            if x == old {
                break;
            }
        }
        x
    }

    pub(super) fn reserve_flight_landings(
        &mut self,
        fs: &[Feature],
        launch: f64,
        y: f64,
        slope: f64,
        impulse: f64,
    ) {
        self.reserve_flight_landings_except(fs, launch, y, slope, impulse, None);
    }

    fn reserve_flight_landings_except(
        &mut self,
        fs: &[Feature],
        launch: f64,
        y: f64,
        slope: f64,
        impulse: f64,
        excluded: Option<u32>,
    ) {
        for (rider, profile) in PROFILES.iter().enumerate() {
            for i in 0..=4 {
                let speed =
                    profile.speed_min + (profile.speed_max - profile.speed_min) * i as f64 / 4.;
                let mut x = launch;
                let mut y = y;
                let mut vx = speed * WORLD_SCALE / (1. + slope * slope).sqrt();
                let mut vy = vx * slope - profile.jump * WORLD_SCALE * impulse;
                for _ in 0..480 {
                    let old = (x, y);
                    let (nx, ny, dx, dy) = air_step(vx, vy, rider, false, 1. / 60.);
                    x += dx;
                    y += dy;
                    vx = nx;
                    vy = ny;
                    let rail = fs
                        .iter()
                        .filter(|f| {
                            f.active
                                && Some(f.id) != excluded
                                && f.kind == "rail"
                                && x >= f.x - f.width / 2.
                                && x < f.x + f.width / 2.
                        })
                        .find(|f| {
                            let (surface, slope) = super::super::Game::rail_line(f, x);
                            old.1 + 18. <= super::super::Game::rail_line(f, old.0).0 + 1.
                                && y + 18. >= surface
                                && vy - vx * slope > 0.
                        });
                    if let Some(rail) = rail {
                        let height = self.sample(x).0 - super::super::Game::rail_line(rail, x).0;
                        self.reserve_landing(x - 180., x + 350., height);
                        break;
                    }
                    let (ground, slope, _) = self.sample(x);
                    if !self.gap(x) && y + 18. >= ground && vy - vx * slope > 0. {
                        self.reserve_landing(x - 200., x + 1000., 0.);
                        break;
                    }
                }
            }
        }
    }

    pub(super) fn reserve_rail_exit(&mut self, fs: &[Feature], id: u32) {
        let rail = fs.iter().find(|f| f.id == id).unwrap();
        let x = rail.x + rail.width / 2. + 1.;
        let (y, slope) = super::super::Game::rail_line(rail, x);
        for impulse in [0., 1.] {
            self.reserve_flight_landings_except(fs, x, y - 18., slope, impulse, Some(id));
        }
        if rail.variant == 0 {
            // Contact can begin partway along a rope. Its three-second failure
            // releases the rider anywhere along the remaining span.
            for i in 1..=6 {
                let x = rail.x - rail.width / 2. + rail.width * i as f64 / 7.;
                let (y, slope) = super::super::Game::rail_line(rail, x);
                self.reserve_flight_landings_except(fs, x, y - 18., slope, 0., Some(id));
            }
        }
    }

    pub(super) fn support_rope(&mut self, fs: &mut Vec<Feature>, id: u32) {
        let rail = fs.iter().find(|f| f.id == id).unwrap().clone();
        if rail.variant != 0 {
            return;
        }
        let mut attachments = [0; 2];
        for (index, (x, y)) in [
            (rail.x - rail.width / 2., rail.y),
            (rail.x + rail.width / 2., rail.y2),
        ]
        .into_iter()
        .enumerate()
        {
            self.feature(fs, "post", x, y, 14., self.sample(x).0);
            attachments[index] = self.next;
        }
        fs.iter_mut()
            .find(|f| f.id == id)
            .unwrap()
            .appearance
            .as_mut()
            .unwrap()
            .attachments = Some(attachments);
    }

    pub(super) fn sync_static_supports(&self, fs: &mut [Feature]) {
        let endpoints: Vec<_> = fs
            .iter()
            .filter(|f| f.kind == "rail" && f.variant == 0)
            .filter_map(|rail| {
                rail.appearance.as_ref()?.attachments.map(|ids| {
                    (
                        ids,
                        rail.x - rail.width / 2.,
                        rail.x + rail.width / 2.,
                        rail.y,
                        rail.y2,
                    )
                })
            })
            .collect();
        for (ids, left, right, y, y2) in endpoints {
            for (id, x, top) in [(ids[0], left, y), (ids[1], right, y2)] {
                if let Some(post) = fs.iter_mut().find(|f| f.id == id && f.kind == "post") {
                    post.x = x;
                    post.y = top;
                }
            }
        }
    }

    pub(super) fn prune_features(&mut self, x: f64, fs: &mut Vec<Feature>) {
        let mut keep: std::collections::BTreeSet<u32> = fs
            .iter()
            .filter(|f| f.x + f.width / 2. > x - 2200.)
            .map(|f| f.id)
            .collect();
        loop {
            let before = keep.len();
            let parents: Vec<_> = fs
                .iter()
                .filter(|f| keep.contains(&f.id))
                .filter_map(|f| f.appearance.as_ref()?.attachments)
                .flatten()
                .collect();
            keep.extend(parents);
            if keep.len() == before {
                break;
            }
        }
        fs.retain(|f| {
            keep.contains(&f.id) && (f.kind != "rock" || self.rock_allowed(f.x, f.width))
        });
        self.landings.retain(|band| band.right > x - 2200.);
    }
}
