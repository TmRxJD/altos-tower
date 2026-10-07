use super::*;
fn funded(coins: u32) -> Game {
    Game::new(&format!(r#"{{"save_version":2,"coins":{coins}}}"#))
}
fn pickup(g: &mut Game, kind: &'static str, big: bool) {
    g.features.clear();
    g.features.push(Feature {
        appearance: None,
        id: 90001,
        kind,
        x: g.x + 4.,
        y: g.y,
        y2: g.y,
        width: 30.,
        sag: 0.,
        variant: u8::from(big),
        anchored: false,
        magnetized: false,
        offset: 0.,
        offset2: 0.,
        active: true,
    });
}
#[test]
fn big_coin_awards_ten_once_to_run_wallet_and_lifetime() {
    let mut g = funded(20);
    g.start_seeded(false, 1);
    pickup(&mut g, "coin", true);
    g.tick(1. / 120.);
    assert_eq!((g.coins, g.progress.wallet, g.progress.coins), (10, 30, 30));
    g.tick(1. / 120.);
    assert_eq!(g.coins, 10);
    pickup(&mut g, "coin", false);
    g.tick(1. / 120.);
    assert_eq!((g.coins, g.progress.wallet), (11, 31));
}

#[test]
fn practice_pickups_and_recovery_neither_earn_currency_nor_spend_safety_inventory() {
    for zen in [false, true] {
        let mut g = funded(1000);
        g.progress.helmets = 2;
        g.progress.rescue_picks = 2;
        g.start_seeded(zen, 7);
        assert_eq!((g.progress.helmets, g.progress.rescue_picks), (2, 2));
        pickup(&mut g, "coin", true);
        g.tick(1. / 120.);
        assert_eq!(g.coins, 10);
        assert_eq!(
            (g.progress.wallet, g.progress.coins),
            if zen { (1000, 1000) } else { (1010, 1010) }
        );
        if zen {
            // Practice does not equip paid stock. A shield found during
            // practice is also consumed without touching purchased helmets.
            assert!(!g.shop_helmet);
            pickup(&mut g, "shield", false);
            g.tick(1. / 120.);
            g.bad_landing();
            g.course.gaps = vec![(0., 1000.)];
            g.x = 300.;
            g.y = g.course.sample(g.x).0 + 300.;
            g.grounded = false;
            g.immune = 0.;
            g.tick(1. / 120.);
            assert!(g.alive && g.x >= 1000.);
            assert_eq!((g.progress.helmets, g.progress.rescue_picks), (2, 2));
            assert_eq!(g.progress.missions.values, [0.; 3]);
        } else {
            assert!(g.shop_helmet);
            g.bad_landing();
            assert_eq!(
                g.progress.helmets, 1,
                "paid helmet must still work in scored play"
            );
        }
        let restored = Game::new(&g.save());
        assert_eq!(restored.progress.wallet, g.progress.wallet);
        assert_eq!(restored.progress.rescue_picks, 2);
    }
}

#[test]
fn permanent_workshop_completion_requires_a_long_post_campaign_economy() {
    let mut p = Progress {
        coins: 1000000,
        wallet: 1000000,
        missions: progression::Missions {
            level: 61,
            ..Default::default()
        },
        post_campaign_meters: shop::POST_CAMPAIGN_METERS,
        ..Default::default()
    };
    let mut spent = 0;
    for id in ["magnet", "feather", "wingsuit", "scarf"] {
        loop {
            let before = p.wallet;
            if !shop::buy(&mut p, id) {
                break;
            }
            spent += before - p.wallet;
        }
    }
    assert_eq!(spent, 522000);
    for coins_per_run in [200_u32, 500, 1000] {
        let runs = spent.div_ceil(coins_per_run);
        assert!((522..=2610).contains(&runs));
    }
}
#[test]
fn magnet_catches_big_coins_and_keeps_ten_value() {
    let mut g = funded(0);
    g.start_seeded(false, 1);
    g.magnet = 5.;
    pickup(&mut g, "coin", true);
    g.features[0].x = g.x - 110.;
    for _ in 0..120 {
        g.tick(1. / 120.);
    }
    assert_eq!(g.coins, 10);
}
#[test]
fn buying_spends_wallet_preserves_goals_and_persists() {
    let mut g = funded(550);
    let original_level = level(&g.progress);
    assert!(g.purchase("magnet"));
    assert_eq!((g.progress.wallet, g.progress.coins), (50, 550));
    assert_eq!(level(&g.progress), original_level);
    assert!(!g.purchase("magnet"));
    assert!(!g.purchase("unknown"));
    let mut restored = Game::new(&g.save());
    assert_eq!(restored.progress.wallet, 50);
    assert_eq!(restored.progress.upgrades.magnet, 1);
    restored.start_seeded(false, 1);
    pickup(&mut restored, "magnet", false);
    restored.tick(1. / 120.);
    assert!(restored.magnet > 7.4);
    assert!(!restored.purchase("feather"));
}
#[test]
fn upgrade_caps_and_consumable_stock_cannot_overspend() {
    let mut g = funded(1000000);
    g.progress.missions.level = 61;
    g.progress.post_campaign_meters = shop::POST_CAMPAIGN_METERS;
    for _ in 0..5 {
        assert!(g.purchase("scarf"));
    }
    let balance = g.progress.wallet;
    assert!(!g.purchase("scarf"));
    assert_eq!(g.progress.wallet, balance);
    for _ in 0..3 {
        assert!(g.purchase("helmet"));
    }
    let balance = g.progress.wallet;
    assert!(!g.purchase("helmet"));
    assert_eq!(g.progress.wallet, balance);
}
#[test]
fn helmet_stock_is_consumed_only_on_protection_and_never_in_zen() {
    let mut g = funded(2000);
    assert!(g.purchase("helmet"));
    g.start_seeded(false, 1);
    assert!(g.shield);
    assert_eq!(g.progress.helmets, 1);
    g.bad_landing();
    assert!(g.alive);
    assert_eq!(g.progress.helmets, 0);
    assert!(!g.shield);
    let mut zen = funded(2000);
    assert!(zen.purchase("helmet"));
    zen.start_seeded(true, 1);
    zen.bad_landing();
    assert!(zen.alive);
    assert_eq!(zen.progress.helmets, 1);
}
#[test]
fn rescue_stock_saves_a_missed_chasm_and_preserves_free_rescues() {
    let mut g = funded(3000);
    assert!(g.purchase("rescue"));
    g.start_seeded(false, 1);
    g.features.clear();
    g.course.gaps.push((g.x - 100., g.x + 100.));
    g.grounded = false;
    g.y = g.course.sample_at(g.x, g.terrain_time).0 + 300.;
    g.rescues_left = 1;
    g.tick(1. / 120.);
    assert!(g.alive);
    assert!(g.grounded);
    assert_eq!(g.rescues_left, 0);
    assert_eq!(g.progress.rescue_picks, 1);
    g.course.gaps.push((g.x - 100., g.x + 100.));
    g.grounded = false;
    g.y = g.course.sample_at(g.x, g.terrain_time).0 + 300.;
    g.tick(1. / 120.);
    assert!(g.alive);
    assert!(g.grounded);
    assert_eq!(g.progress.rescue_picks, 0);
}
#[test]
fn upgrade_effects_change_charge_and_drain_without_changing_velocity() {
    let mut base = funded(0);
    base.start_seeded(false, 1);
    base.features.clear();
    base.wing = 4.;
    base.scarf_length = 4.;
    let mut upgraded = base.clone();
    upgraded.progress.upgrades.wingsuit = 5;
    base.tick(1. / 120.);
    upgraded.tick(1. / 120.);
    assert!(upgraded.wing > base.wing);
    assert_eq!(upgraded.vx, base.vx);
    base.combo = 1;
    base.combo_points = 10;
    push_trick(&mut base.tricks, "Backflip", 1, 10);
    let mut upgraded = base.clone();
    upgraded.progress.upgrades.scarf = 5;
    base.bank();
    upgraded.bank();
    assert!(upgraded.wing > base.wing);
}
#[test]
fn big_coin_spacing_is_sparse_seeded_and_on_existing_coin_routes() {
    let mut g = funded(0);
    g.start_seeded(false, 7);
    let mut same = funded(0);
    same.start_seeded(false, 7);
    let mut observed = std::collections::BTreeMap::new();
    let mut repeated = std::collections::BTreeMap::new();
    for x in (0..200000).step_by(1000) {
        g.course.extend(x as f64, &mut g.features);
        same.course.extend(x as f64, &mut same.features);
        for f in g
            .features
            .iter()
            .filter(|f| f.kind == "coin" && f.variant == 1)
        {
            observed.insert(f.id, f.x);
        }
        for f in same
            .features
            .iter()
            .filter(|f| f.kind == "coin" && f.variant == 1)
        {
            repeated.insert(f.id, f.x);
        }
    }
    assert_eq!(
        observed, repeated,
        "same seed must keep static large-coin placement"
    );
    let mut positions: Vec<_> = observed.into_values().collect();
    positions.sort_by(f64::total_cmp);
    assert!(positions.len() >= 3);
    assert!(positions.windows(2).all(|p| p[1] - p[0] >= 16000.));
}
#[test]
fn workshop_uses_authored_escalating_prices_and_preserves_safety_prices() {
    let mut g = funded(1000000);
    g.progress.missions.level = 61;
    g.progress.post_campaign_meters = shop::POST_CAMPAIGN_METERS;
    for cost in shop::TIMER_PRICES {
        let before = g.progress.wallet;
        assert!(g.purchase("magnet"));
        assert_eq!(before - g.progress.wallet, cost);
    }
    assert!(!g.purchase("magnet"));
    assert_eq!(g.progress.upgrades.magnet, 6);
    let items = shop::catalog(&g.progress);
    assert_eq!(
        items.iter().find(|i| i.id == "helmet").unwrap().price,
        Some(1500)
    );
    assert_eq!(
        items.iter().find(|i| i.id == "rescue").unwrap().price,
        Some(3000)
    );
}

#[test]
fn wealth_cannot_bypass_tier_progression_or_post_campaign_endgame_requirement() {
    let mut g = funded(2000000);
    assert!(g.purchase("magnet"));
    let wallet = g.progress.wallet;
    assert!(!g.purchase("magnet"));
    assert_eq!(g.progress.wallet, wallet);
    assert!(
        shop::catalog(&g.progress)
            .iter()
            .find(|i| i.id == "magnet")
            .unwrap()
            .locked
    );
    g.progress.missions.level = 11;
    assert!(g.purchase("magnet"));
    for id in ["magnet", "feather", "wingsuit", "scarf"] {
        let max = if ["magnet", "feather"].contains(&id) {
            6
        } else {
            5
        };
        match id {
            "magnet" => g.progress.upgrades.magnet = max - 1,
            "feather" => g.progress.upgrades.feather = max - 1,
            "wingsuit" => g.progress.upgrades.wingsuit = max - 1,
            _ => g.progress.upgrades.scarf = max - 1,
        }
        g.progress.missions.level = 60;
        g.progress.post_campaign_meters = 100000.;
        let before = g.progress.wallet;
        assert!(!g.purchase(id));
        assert_eq!(g.progress.wallet, before);
        g.progress.missions.level = 61;
        g.progress.post_campaign_meters = 99999.;
        assert!(!g.purchase(id));
        assert_eq!(g.progress.wallet, before);
        g.progress.post_campaign_meters = 100000.;
        assert!(g.purchase(id));
        assert!(g.progress.wallet < before);
    }
    let restored = Game::new(&g.save());
    assert_eq!(restored.progress.wallet, g.progress.wallet);
    assert_eq!(restored.progress.upgrades.magnet, 6);
}

#[test]
fn endgame_distance_begins_after_campaign_and_never_counts_practice() {
    for (level, zen) in [(60, false), (61, false), (61, true)] {
        let mut g = funded(1000);
        g.progress.missions.level = level;
        g.start_seeded(zen, 7);
        let before = g.x;
        g.tick(1. / 120.);
        if level == 61 && !zen {
            assert!((g.progress.post_campaign_meters - (g.x - before) / WORLD_SCALE).abs() < 1e-6);
        } else {
            assert_eq!(g.progress.post_campaign_meters, 0.);
        }
    }
}
