use super::{Course, Segment, LOOK_AHEAD, TERRAIN_MARGIN};

pub(super) fn hash(seed: u32, index: i64, salt: u32) -> f64 {
    let mut v = seed ^ (index as u32).wrapping_mul(0x9e3779b9) ^ salt;
    v = (v ^ (v >> 16)).wrapping_mul(0x85ebca6b);
    v = (v ^ (v >> 13)).wrapping_mul(0xc2b2ae35);
    (v ^ (v >> 16)) as f64 / u32::MAX as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_stream_does_not_change_macro_terrain() {
        let mut a = Course::new(714);
        let mut b = Course::new(714);
        b.rng ^= 0x75316291;
        let (mut af, mut bf) = (vec![], vec![]);
        for x in (0..180000).step_by(5000) {
            a.extend(x as f64, &mut af);
            b.extend(x as f64, &mut bf);
            // Feature planners may prefetch farther, but every common macro
            // segment must remain identical, including heights and tangents.
            let end = a.end.min(b.end);
            assert_eq!(
                serde_json::to_string(
                    &a.segments
                        .iter()
                        .filter(|s| s.x1 <= end)
                        .collect::<Vec<_>>()
                )
                .unwrap(),
                serde_json::to_string(
                    &b.segments
                        .iter()
                        .filter(|s| s.x1 <= end)
                        .collect::<Vec<_>>()
                )
                .unwrap()
            );
            assert_eq!(a.biome_at(x as f64), b.biome_at(x as f64));
        }
        assert_ne!(
            serde_json::to_string(&af).unwrap(),
            serde_json::to_string(&bf).unwrap()
        );
    }
}

fn noise(seed: u32, x: f64, salt: u32) -> f64 {
    let i = x.floor() as i64;
    let t = x - x.floor();
    let t = t * t * (3. - 2. * t);
    (hash(seed, i, salt) * (1. - t) + hash(seed, i + 1, salt) * t) * 2. - 1.
}

impl Course {
    pub fn biome_at(&self, x: f64) -> u8 {
        (hash(self.seed, (x / 32000.).floor() as i64, 0xb10) * 3.) as u8 % 3
    }

    pub(super) fn macro_slope(&self, x: f64) -> f64 {
        let opening = 0.28 + hash(self.seed, 0, 0x51) * 0.06;
        if x <= 8000. {
            return opening;
        }
        let blend = ((x - 8000.) / 4000.).clamp(0., 1.);
        let blend = blend * blend * (3. - 2. * blend);
        let mut slope =
            0.44 + 0.14 * noise(self.seed, x / 4800., 11) + 0.06 * noise(self.seed, x / 1200., 31);
        // Short, rare crests; the ordinary route remains a downhill racer.
        let crest_region = (x / 55000.).floor() as i64;
        if hash(self.seed, crest_region, 71) < 0.35 {
            let crest =
                crest_region as f64 * 55000. + 11000. + hash(self.seed, crest_region, 73) * 34000.;
            let distance = (x - crest).abs() / 650.;
            if distance < 1. {
                slope -= 0.50 * (std::f64::consts::PI * distance / 2.).cos().powi(2);
            }
        }
        // Rare short faces sit in a continuous low-frequency mountain envelope.
        // Their locations do not depend on village/wall/balloon selections.
        let region = (x / 42000.).floor() as i64;
        let center = region as f64 * 42000. + 17000. + hash(self.seed, region, 53) * 13000.;
        let d = (x - center).abs() / 1600.;
        if d < 1. {
            slope += 1.90 * (std::f64::consts::PI * d / 2.).cos().powi(2);
        }
        opening * (1. - blend) + slope.clamp(-0.20, 2.4) * blend
    }

    pub(super) fn extend_terrain(&mut self, x: f64) {
        while self.end < x + LOOK_AHEAD + TERRAIN_MARGIN {
            let x0 = self.end;
            let x1 = x0 + 500.;
            let (y0, m0) = self
                .segments
                .last()
                .map(|s| (s.y1, s.m1))
                .unwrap_or((0., self.macro_slope(x0)));
            let m1 = self.macro_slope(x1);
            // Integrate the slope envelope with Simpson's rule; Hermite pieces
            // preserve common heights/tangents at every 500-unit boundary.
            let mean = (m0 + 4. * self.macro_slope((x0 + x1) / 2.) + m1) / 6.;
            self.segments.push(Segment {
                x0,
                x1,
                y0,
                y1: y0 + (x1 - x0) * mean,
                m0,
                m1,
            });
            self.end = x1;
            if m0.max(m1) > 1.2 {
                self.reserve_landing(x0 - 1000., x1 + 5500., 0.);
            }
        }
    }
}
