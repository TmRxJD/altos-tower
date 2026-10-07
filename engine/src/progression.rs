//! Authored Tower challenges using observable Alto-style mechanics. This is
//! not a recovered native list; unsupported llama/bird goals are excluded.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    Distance,
    Coins,
    MagnetCoins,
    BigCoins,
    Flips,
    FlipStack,
    Grinds,
    LongGrinds,
    Rocks,
    Bounces,
    ComboScore,
    ComboSize,
    Score,
    Walls,
    Balloons,
    Wingsuits,
    Loops,
    Proximity,
    FlightDistance,
    Chasms,
    Elders,
    WallWingWall,
    BalloonFlip,
    GrindFlip,
    RiderFlips,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Run,
    Total,
    Combo,
}
#[derive(Clone, Serialize)]
pub struct Goal {
    pub id: String,
    pub label: String,
    pub value: f64,
    pub target: f64,
    pub complete: bool,
    pub scope: Scope,
    pub metric: Metric,
    pub rider: Option<usize>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Missions {
    pub level: u32,
    pub values: [f64; 3],
    pub completed: [bool; 3],
    pub history: Vec<u32>,
    pub migrated: bool,
    #[serde(skip)]
    pub(crate) cached_level: u32,
    #[serde(skip)]
    pub(crate) cached_definitions: Vec<Goal>,
}
impl Default for Missions {
    fn default() -> Self {
        Self {
            level: 1,
            values: [0.; 3],
            completed: [false; 3],
            history: vec![],
            migrated: false,
            cached_level: 0,
            cached_definitions: vec![],
        }
    }
}
fn goal(level: u32, slot: usize, metric: Metric, target: f64, scope: Scope, text: &str) -> Goal {
    let suffix = if scope == Scope::Run {
        " in one run"
    } else if scope == Scope::Combo {
        " in one landed combo"
    } else {
        " while this level is active"
    };
    Goal {
        id: format!("L{level}-{}", slot + 1),
        label: format!("{}{}", text.replace("{}", &format!("{target:.0}")), suffix),
        value: 0.,
        target,
        complete: false,
        scope,
        metric,
        rider: None,
    }
}
pub fn definitions(level: u32) -> Vec<Goal> {
    use Metric::*;
    use Scope::*;
    let level = level.clamp(1, 60);
    let c = ((level - 1) / 10) as f64;
    if level == 60 {
        return vec![
            goal(level, 0, Score, 350000., Run, "Score {} points"),
            goal(level, 1, Distance, 15000., Run, "Travel {} m"),
            goal(level, 2, Elders, 6., Run, "Escape {} elders across chasms"),
        ];
    }
    let row = match (level - 1) % 10 {
        0 => [
            (Flips, 1. + c * 5., Run, "Land {} backflips"),
            (Distance, 250. + c * 1500., Run, "Travel {} m"),
            (Coins, 10. + c * 80., Run, "Collect {} coins"),
        ],
        1 => [
            (Flips, 3. + c * 8., Total, "Land {} backflips"),
            (BigCoins, 1. + c * 2., Run, "Collect {} large coins"),
            (Score, 500. + c * 7000., Run, "Score {} points"),
        ],
        2 => [
            (Grinds, 1. + c * 4., Run, "Land {} grinds"),
            (ComboSize, 2. + c * 2., Combo, "Bank a {}× multiplier"),
            (Distance, 750. + c * 1800., Run, "Travel {} m"),
        ],
        3 => [
            (Bounces, 1. + c * 3., Run, "Land {} rock bounces"),
            (
                Coins,
                50. + c * 100.,
                if (c as u32).is_multiple_of(2) {
                    Run
                } else {
                    Total
                },
                "Collect {} coins",
            ),
            (ComboScore, 1000. + c * 4500., Combo, "Bank {} points"),
        ],
        4 => [
            (Rocks, 2. + c * 5., Run, "Smash {} rocks with boost"),
            (
                MagnetCoins,
                10. + c * 60.,
                Total,
                "Collect {} coins with a magnet",
            ),
            (LongGrinds, 1. + c * 3., Run, "Land {} long grind bonuses"),
        ],
        5 => [
            (
                FlipStack,
                (2. + c).min(4.),
                Combo,
                "Land a {}-backflip stack",
            ),
            (Coins, 100. + c * 120., Run, "Collect {} coins"),
            (ComboScore, 3000. + c * 6500., Combo, "Bank {} points"),
        ],
        6 => [
            (Chasms, 1. + c * 3., Run, "Clear {} chasms"),
            (Balloons, 2. + c * 3., Run, "Land {} balloon bounces"),
            (Walls, 1. + c * 3., Run, "Land {} wall rides"),
        ],
        7 => [
            (Wingsuits, 1. + c * 3., Run, "Land {} wingsuit combos"),
            (Loops, 1. + c * 2., Run, "Land {} wingsuit loops"),
            (FlightDistance, 250. + c * 350., Run, "Wingsuit for {} m"),
        ],
        8 => [
            (
                FlipStack,
                (3. + c).min(4.),
                Combo,
                "Land a {}-backflip stack",
            ),
            (
                Proximity,
                1. + c * 3.,
                Run,
                "Land {} proximity flight combos",
            ),
            (Score, 5000. + c * 10000., Run, "Score {} points"),
        ],
        _ => [
            (Elders, 1. + c, Run, "Escape {} elders across chasms"),
            (Loops, 2. + c * 3., Run, "Land {} wingsuit loops"),
            (ComboSize, 5. + c * 2., Combo, "Bank a {}× multiplier"),
        ],
    };
    if level <= 10 {
        return row
            .into_iter()
            .enumerate()
            .map(|(i, (metric, target, scope, text))| goal(level, i, metric, target, scope, text))
            .collect();
    }
    // Later chapters recombine skill, collection and endurance goals rather
    // than replaying the same ten triplets with larger numbers.
    let n = (level - 11) as usize;
    let skills = [
        (Flips, 8. + c * 7., Run, "Land {} backflips"),
        (Grinds, 3. + c * 3., Run, "Land {} grinds"),
        (
            FlipStack,
            (2. + c).min(4.),
            Combo,
            "Land a {}-backflip stack",
        ),
        (Bounces, 2. + c * 2., Run, "Land {} rock bounces"),
        (ComboSize, 4. + c * 2., Combo, "Bank a {}× multiplier"),
        (Walls, 2. + c * 2., Run, "Land {} wall rides"),
        (Balloons, 2. + c * 3., Run, "Land {} balloon bounces"),
        (Loops, 2. + c * 3., Run, "Land {} wingsuit loops"),
        (LongGrinds, 1. + c * 3., Run, "Land {} long grind bonuses"),
        (
            Proximity,
            1. + c * 2.,
            Run,
            "Land {} proximity flight combos",
        ),
        (Rocks, 4. + c * 4., Run, "Smash {} rocks with boost"),
        (Chasms, 2. + c * 2., Run, "Clear {} chasms"),
        (Elders, c.min(5.), Run, "Escape {} elders across chasms"),
        (
            WallWingWall,
            1. + (c / 2.).floor(),
            Total,
            "Land {} wall → wingsuit → wall chains",
        ),
        (
            BalloonFlip,
            2. + c * 2.,
            Run,
            "Land {} balloon → backflip chains",
        ),
        (
            GrindFlip,
            2. + c * 2.,
            Run,
            "Land {} grind → backflip chains",
        ),
        (ComboScore, 5000. + c * 10000., Combo, "Bank {} points"),
    ];
    let (metric, target, scope, text) = skills[n % skills.len()];
    let mut skill = goal(level, 0, metric, target, scope, text);
    if [11, 21, 31, 41, 51].contains(&level) {
        let rider = [0, 1, 2, 3, 5, 4][(level / 10) as usize];
        skill = goal(level, 0, RiderFlips, 5. + c * 4., Run, "Land {} backflips");
        skill.rider = Some(rider);
        skill.label = format!(
            "{} using {}",
            skill.label,
            ["Basic", "Fast", "Tank", "Ranged", "Protector", "Vampire"][rider]
        );
    }
    let collections = [
        (Coins, 80. + c * 140., Run, "Collect {} coins"),
        (BigCoins, 2. + c * 3., Run, "Collect {} large coins"),
        (
            MagnetCoins,
            40. + c * 70.,
            Total,
            "Collect {} coins with a magnet",
        ),
    ];
    let (metric, target, scope, text) = collections[(n / skills.len()) % collections.len()];
    let collect = goal(level, 1, metric, target, scope, text);
    let endure = if (n / 5).is_multiple_of(2) {
        goal(level, 2, Distance, 1500. + c * 2200., Run, "Travel {} m")
    } else {
        goal(
            level,
            2,
            FlightDistance,
            500. + c * 400.,
            Run,
            "Wingsuit for {} m",
        )
    };
    vec![skill, collect, endure]
}
impl Missions {
    fn invalidate_definitions(&mut self) {
        self.cached_level = 0;
        self.cached_definitions.clear();
    }
    fn ensure_definitions(&mut self) {
        if self.cached_level != self.level {
            self.cached_definitions = if self.level <= 60 {
                definitions(self.level)
            } else {
                vec![]
            };
            self.cached_level = self.level;
        }
    }
    pub fn sanitize(&mut self) {
        self.invalidate_definitions();
        self.level = self.level.clamp(1, 61);
        for v in &mut self.values {
            if !v.is_finite() || *v < 0. {
                *v = 0.;
            }
        }
        self.history
            .retain(|v| *v > 0 && *v < self.level && *v <= 60);
        self.history.sort_unstable();
        self.history.dedup();
    }
    pub fn goals(&self) -> Vec<Goal> {
        if self.level > 60 {
            return vec![];
        }
        (if self.cached_level == self.level {
            self.cached_definitions.clone()
        } else {
            definitions(self.level)
        })
        .into_iter()
        .enumerate()
        .map(|(i, mut g)| {
            g.value = self.values[i].min(g.target);
            g.complete = self.completed[i];
            g
        })
        .collect()
    }
    pub fn start_run(&mut self) {
        self.ensure_definitions();
        for (i, g) in self.cached_definitions.iter().enumerate() {
            if g.scope != Scope::Total && !self.completed[i] {
                self.values[i] = 0.;
            }
        }
    }
    /// Record one coherent event batch. Advancing happens only after every
    /// metric is evaluated against the old level, so the bank cannot also
    /// satisfy the newly unlocked level with the same trick.
    pub fn record(&mut self, events: &[(Metric, f64)]) {
        self.record_for_rider(events, None);
    }
    pub fn record_for_rider(&mut self, events: &[(Metric, f64)], rider: Option<usize>) {
        if self.level > 60 {
            return;
        }
        self.ensure_definitions();
        for (i, g) in self.cached_definitions.iter().enumerate() {
            if self.completed[i] {
                continue;
            }
            if g.rider.is_some() && g.rider != rider {
                continue;
            }
            for &(metric, amount) in events {
                if metric != g.metric || !amount.is_finite() || amount <= 0. {
                    continue;
                }
                if matches!(
                    metric,
                    Metric::FlipStack | Metric::ComboSize | Metric::ComboScore
                ) {
                    self.values[i] = self.values[i].max(amount);
                } else {
                    self.values[i] += amount;
                }
            }
            if self.values[i] >= g.target {
                self.values[i] = g.target;
                self.completed[i] = true;
            }
        }
        if self.completed.into_iter().all(|v| v) {
            self.history.push(self.level);
            self.level += 1;
            self.invalidate_definitions();
            self.values = [0.; 3];
            self.completed = [false; 3];
        }
    }
    pub fn catalog(&self) -> serde_json::Value {
        let levels:Vec<_>=(1..=60).map(|level| {
            let complete=self.history.contains(&level);
            let mut goals=if level==self.level {self.goals()}else{definitions(level)};
            if complete { for g in &mut goals {g.complete=true;g.value=g.target;} }
            serde_json::json!({"level":level,"goals":goals,"complete":complete,"unlocks":if [10,20,30,40,50].contains(&level){Some([0,1,2,3,5,4][(level/10) as usize])}else{None}})
        }).collect();
        serde_json::json!({"level":self.level.min(60),"total":60,"complete":self.level>60,"current_goals":self.goals(),"levels":levels,"history":self.history,"migrated":self.migrated})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_definitions_follow_level_advance_and_save_restore() {
        let mut m = Missions::default();
        m.record(&[
            (Metric::Distance, 250.),
            (Metric::Coins, 10.),
            (Metric::Flips, 1.),
        ]);
        assert_eq!(m.level, 2);
        m.record(&[(Metric::Coins, 100.)]);
        assert_eq!(
            m.values, [0.; 3],
            "level two needs large coins, not the cached level-one coin goal"
        );
        m.record(&[(Metric::Flips, 2.)]);
        let saved = serde_json::to_string(&m).unwrap();
        assert!(!saved.contains("cached"));
        let mut restored: Missions = serde_json::from_str(&saved).unwrap();
        restored.sanitize();
        restored.record(&[(Metric::Flips, 1.)]);
        assert_eq!(restored.level, 2);
        assert!(restored.goals()[0].complete);
        assert_eq!(restored.goals()[0].id, "L2-1");
        restored.level = 11;
        restored.values = [0.; 3];
        restored.completed = [false; 3];
        restored.sanitize();
        restored.record_for_rider(&[(Metric::RiderFlips, 1.)], Some(1));
        assert_eq!(restored.goals()[0].id, "L11-1");
        assert_eq!(restored.values[0], 1.);
    }
    #[test]
    fn sixty_levels_require_three_independent_challenges() {
        for l in 1..=60 {
            let gs = definitions(l);
            assert_eq!(gs.len(), 3);
            assert!(gs.iter().all(|g| g.target > 0.));
            assert!(gs
                .iter()
                .any(|g| !matches!(g.metric, Metric::Distance | Metric::Coins | Metric::Score)));
        }
        let combinations: std::collections::BTreeSet<_> = (1..=60)
            .map(|l| {
                let mut metrics: Vec<_> = definitions(l).into_iter().map(|g| g.metric).collect();
                metrics.sort();
                metrics
            })
            .collect();
        assert!(
            combinations.len() >= 40,
            "only {} distinct metric triplets",
            combinations.len()
        );
    }
    #[test]
    fn distance_alone_never_advances_and_one_run_retries_reset_incomplete_counts() {
        let mut m = Missions::default();
        m.record(&[(Metric::Distance, 999999.)]);
        assert_eq!(m.level, 1);
        m.record(&[(Metric::Coins, 5.)]);
        m.start_run();
        assert_eq!(m.values[2], 0.);
        assert!(m.completed[1]);
        m.record(&[(Metric::Flips, 1.), (Metric::Coins, 10.)]);
        assert_eq!(m.level, 2);
        assert_eq!(m.values, [0.; 3]);
    }
    #[test]
    fn cumulative_is_only_active_level_and_completed_goals_survive_reload() {
        let mut m = Missions {
            level: 2,
            ..Missions::default()
        };
        m.record(&[(Metric::Flips, 2.)]);
        m.start_run();
        assert_eq!(m.values[0], 2.);
        let mut m: Missions = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        m.record(&[(Metric::Flips, 1.)]);
        assert!(m.completed[0]);
    }
    #[test]
    fn one_event_batch_cannot_complete_two_levels() {
        let mut m = Missions::default();
        m.record(&[
            (Metric::Distance, 9999.),
            (Metric::Flips, 999.),
            (Metric::Coins, 999.),
            (Metric::BigCoins, 999.),
            (Metric::Score, 99999.),
        ]);
        assert_eq!(m.level, 2);
        assert_eq!(m.values, [0.; 3]);
    }
}
