use super::reference::{air_step, PROFILES, WORLD_SCALE};
use super::Feature;
pub const KEEP_BEHIND: f64 = 5000.;
pub const LOOK_AHEAD: f64 = 11000.;
mod grammar;
mod safety;
mod terrain;
// Terrain is sampled independently of feature choices, including their lengths.
const TERRAIN_MARGIN: f64 = 17000.;
#[cfg(test)]
pub const STORAGE_SEGMENT_LIMIT: usize = 96;

/// Seeded visual and attachment data, shared by the renderer and moving rails.
/// These are grammar design parameters, not recovered native Alto settings.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Appearance {
    pub biome: u8,
    pub roof_pitch: f64,
    pub facade_height: f64,
    pub door_offset: f64,
    pub window_spacing: f64,
    pub post_offsets: [f64; 2],
    pub palette: u8,
    pub motion_amplitude: f64,
    pub motion_frequency: f64,
    pub motion_phase: f64,
    pub attachments: Option<[u32; 2]>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Segment {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    pub m0: f64,
    pub m1: f64,
}
impl Segment {
    pub fn sample(&self, x: f64) -> (f64, f64, f64) {
        let l = self.x1 - self.x0;
        let t = ((x - self.x0) / l).clamp(0., 1.);
        let a = 2. * self.y0 - 2. * self.y1 + l * (self.m0 + self.m1);
        let b = -3. * self.y0 + 3. * self.y1 - l * (2. * self.m0 + self.m1);
        let c = l * self.m0;
        (
            (a * t * t * t + b * t * t + c * t + self.y0),
            (3. * a * t * t + 2. * b * t + c) / l,
            (6. * a * t + 2. * b) / (l * l),
        )
    }
}
#[derive(Clone)]
pub struct Course {
    pub segments: Vec<Segment>,
    pub gaps: Vec<(f64, f64)>,
    pub encounters: Vec<(f64, u32)>,
    pub end: f64,
    seed: u32,
    rng: u32,
    next_site: f64,
    next_prune: f64,
    history: Vec<u32>,
    tension: f64,
    last_chasm: f64,
    coin_budget: f64,
    last_wall_route: u8,
    next: u32,
    next_big_coin: f64,
    rail_coins: Vec<(u32, Vec<(u32, f64)>)>,
    balloon_rails: Vec<(u32, u32, u32)>,
    landings: Vec<safety::LandingCorridor>,
}
impl Course {
    pub fn new(seed: u32) -> Self {
        // Mix nearby run seeds before xorshift; small consecutive seeds otherwise
        // produce almost the same first random choices.
        let mut mixed = seed.wrapping_add(0x9e3779b9);
        mixed = (mixed ^ (mixed >> 16)).wrapping_mul(0x85ebca6b);
        mixed = (mixed ^ (mixed >> 13)).wrapping_mul(0xc2b2ae35);
        mixed ^= mixed >> 16;
        Self {
            segments: vec![],
            gaps: vec![],
            encounters: vec![],
            end: 0.,
            seed,
            rng: if mixed == 0 { 0x6d2b79f5 } else { mixed },
            next_site: 0.,
            next_prune: 0.,
            history: vec![],
            tension: 0.,
            last_chasm: 0.,
            coin_budget: 6.,
            last_wall_route: 99,
            next: 0,
            next_big_coin: 16000.,
            rail_coins: vec![],
            balloon_rails: vec![],
            landings: vec![],
        }
    }
    fn rand(&mut self) -> f64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x as f64 / u32::MAX as f64
    }
    pub fn sample(&self, x: f64) -> (f64, f64, f64) {
        if let Some(s) = self.segments.first() {
            if x < s.x0 {
                return (s.y0 + (x - s.x0) * s.m0, s.m0, 0.);
            }
        }
        if let Some(s) = self
            .segments
            .get(self.segments.partition_point(|s| s.x1 < x))
        {
            s.sample(x)
        } else if let Some(s) = self.segments.last() {
            (s.y1 + (x - s.x1) * s.m1, s.m1, 0.)
        } else {
            (x * 0.2, 0.2, 0.)
        }
    }
    pub fn gap(&self, x: f64) -> bool {
        self.gaps.iter().any(|&(a, b)| x > a && x < b)
    }
    pub fn sample_at(&self, x: f64, _time: f64) -> (f64, f64, f64, f64) {
        let (y, slope, curvature) = self.sample(x);
        (y, slope, curvature, 0.)
    }
    pub fn anchor_features(&self, time: f64, fs: &mut [Feature]) {
        for f in fs.iter_mut().filter(|f| f.anchored) {
            let (a, b) = if f.kind == "rail" {
                (f.x - f.width / 2., f.x + f.width / 2.)
            } else {
                (f.x, f.x)
            };
            let offset = self.sample_at(a, time).0 - self.sample(a).0;
            let offset2 = self.sample_at(b, time).0 - self.sample(b).0;
            f.y += offset - f.offset;
            f.y2 += offset2 - f.offset2;
            f.offset = offset;
            f.offset2 = offset2;
        }
        for f in fs.iter_mut().filter(|f| f.kind == "balloon") {
            let previous_y = f.y;
            let (amplitude, frequency, phase) = f
                .appearance
                .as_ref()
                .map(|a| (a.motion_amplitude, a.motion_frequency, a.motion_phase))
                .unwrap_or((50., 0.65, f.id as f64));
            f.y = f.y2 + ((time * frequency + phase).sin() - phase.sin()) * amplitude;
            f.offset = f.y - previous_y;
        }
        for &(rail_id, a, b) in &self.balloon_rails {
            let endpoints = fs
                .iter()
                .find(|f| f.id == a)
                .zip(fs.iter().find(|f| f.id == b))
                .map(|(a, b)| (a.y + a.width * 0.9, b.y + b.width * 0.9));
            if let Some((a, b)) = endpoints {
                if let Some(rail) = fs.iter_mut().find(|f| f.id == rail_id) {
                    rail.offset = a - rail.y;
                    rail.offset2 = b - rail.y2;
                    rail.y = a;
                    rail.y2 = b;
                }
            }
        }
        for (id, coins) in &self.rail_coins {
            let Some(rail) = fs.iter().find(|f| f.id == *id).cloned() else {
                continue;
            };
            for &(coin, t) in coins {
                if let Some(f) = fs.iter_mut().find(|f| f.id == coin && !f.magnetized) {
                    let x = rail.x - rail.width / 2. + rail.width * t;
                    f.y = super::Game::rail_line(&rail, x).0 - 40.;
                    f.y2 = f.y;
                }
            }
        }
    }
    fn feature(
        &mut self,
        fs: &mut Vec<Feature>,
        kind: &'static str,
        x: f64,
        y: f64,
        width: f64,
        y2: f64,
    ) -> bool {
        self.next += 1;
        let big = kind == "coin" && x >= self.next_big_coin && self.coin_budget >= 10.;
        if kind == "coin" {
            let cost = if big { 10. } else { 1. };
            if self.coin_budget < cost {
                return false;
            }
            self.coin_budget -= cost;
        }
        if big {
            self.next_big_coin = x + 16000. + (self.rng % 8000) as f64;
        }
        let appearance = matches!(
            kind,
            "rail" | "wall" | "balloon" | "hut" | "ruin" | "pine" | "rock" | "post"
        )
        .then(|| self.appearance(x));
        fs.push(Feature {
            id: self.next,
            kind,
            x,
            y,
            y2,
            width: if big { 30. } else { width },
            sag: 0.,
            variant: if big { 1 } else { 0 },
            anchored: kind != "balloon",
            magnetized: false,
            offset: 0.,
            offset2: 0.,
            active: true,
            appearance,
        });
        true
    }
    fn ground_group(&mut self, fs: &mut Vec<Feature>, start: f64) {
        // Shift the whole recovery group if a seeded rock occupies its intended
        // spot. Dropping each blocked coin could erase an entire reward group.
        let Some(start) = [start, start + 350., start - 350.]
            .into_iter()
            .find(|&candidate| {
                candidate >= 0.
                    && (0..5).all(|i| {
                        let x = candidate + i as f64 * 42.;
                        !self.gaps.iter().any(|&(a, b)| x > a - 80. && x < b + 80.)
                            && !fs.iter().any(|f| {
                                f.active
                                    && f.kind == "rock"
                                    && (x - f.x).abs() < f.width / 2. + 110.
                            })
                    })
            })
        else {
            return;
        };
        for i in 0..5 {
            let x = start + i as f64 * 42.;
            let y = self.sample(x).0 - 34.;
            let blocked = self.gaps.iter().any(|&(a, b)| x > a - 80. && x < b + 80.)
                || fs
                    .iter()
                    .any(|f| f.active && f.kind == "rock" && (x - f.x).abs() < f.width / 2. + 110.)
                || fs
                    .iter()
                    .any(|f| f.kind == "coin" && (x - f.x).abs() < 36. && (y - f.y).abs() < 48.);
            if !blocked {
                self.feature(fs, "coin", x, y, 18., y);
            }
        }
    }
    fn jump_coins(&mut self, fs: &mut Vec<Feature>, launch: f64, n: u32) {
        self.reserve_flight_landings(
            fs,
            launch,
            self.sample(launch).0 - 18.,
            self.sample(launch).1,
            1.,
        );
        let n = n.min(4);
        let slope = self.sample(launch).1;
        let vx = PROFILES[0].speed_min * WORLD_SCALE / (1. + slope * slope).sqrt();
        let mut ax = launch;
        let mut ay = self.sample(launch).0 - 18.;
        let mut avx = vx;
        let mut avy = vx * slope - super::jump_impulse(0);
        let mut elapsed = 0.;
        for i in 0..n {
            let t = 0.09 + 0.21 * i as f64;
            while elapsed < t {
                let dt = (t - elapsed).min(1. / 120.);
                let (nx, ny, dx, dy) = air_step(avx, avy, 0, false, dt);
                avx = nx;
                avy = ny;
                ax += dx;
                ay += dy;
                elapsed += dt;
            }
            // The end of a guide settles onto the authored landing surface.
            // It never tracks an individual rider's eventual jump.
            let coin_y = ay.min(self.sample(ax).0 - 35.);
            if self.feature(fs, "coin", ax, coin_y, 18., coin_y) {
                fs.last_mut().unwrap().anchored = false;
            }
        }
    }
    fn long_rail(&mut self, fs: &mut Vec<Feature>, left: f64, y: f64, width: f64) {
        self.long_rail_with_reservation(fs, left, y, width, true);
    }
    fn long_rail_with_reservation(
        &mut self,
        fs: &mut Vec<Feature>,
        left: f64,
        y: f64,
        width: f64,
        reserve: bool,
    ) {
        let y2 = self.sample(left + width).0 - 140.;
        self.feature(fs, "rail", left + width / 2., y, width, y2);
        let rail_id = self.next;
        let sag = 35. + self.rand() * 25.;
        fs.iter_mut().find(|f| f.id == rail_id).unwrap().sag = sag;
        // Keep the entire rideable cable above ground hazards, including its sag.
        let rail = fs.iter().find(|f| f.id == rail_id).unwrap().clone();
        let lift = (0..=128)
            .map(|i| {
                let x = left + width * i as f64 / 128.;
                super::Game::rail_line(&rail, x).0 - self.sample(x).0 + 74.
            })
            .fold(0_f64, f64::max);
        let rail = fs.iter_mut().find(|f| f.id == rail_id).unwrap();
        rail.y -= lift;
        rail.y2 -= lift;
        let y = rail.y;
        let y2 = rail.y2;
        let mut coins = vec![];
        let groups = if width >= 2200. { 2 } else { 1 };
        for i in 0..groups * 4 {
            let center = if groups == 1 {
                0.5
            } else if i < 4 {
                0.28
            } else {
                0.72
            };
            let t = center + (i % 4) as f64 * 42. / width - 63. / width;
            let x = left + width * t;
            let cy = y + (y2 - y) * t + 4. * sag * t * (1. - t) - 40.;
            if self.feature(fs, "coin", x, cy, 18., cy) {
                fs.last_mut().unwrap().anchored = false;
                coins.push((self.next, t));
            }
        }
        self.rail_coins.push((rail_id, coins));
        self.support_rope(fs, rail_id);
        if reserve {
            self.reserve_rail_exit(fs, rail_id);
        }
    }
}
