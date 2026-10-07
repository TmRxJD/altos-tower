use super::Progress;
use serde::{Deserialize, Serialize};

/// Authored economy override: final upgrades are a post-campaign objective.
pub const POST_CAMPAIGN_METERS: f64 = 100000.;
pub const TIMER_PRICES: [u32; 6] = [500, 1500, 4000, 12000, 30000, 80000];
pub const WING_PRICES: [u32; 5] = [1500, 4500, 12000, 35000, 100000];
pub const SCARF_PRICES: [u32; 5] = [1000, 3000, 9000, 25000, 75000];
pub fn balance_report(p: &Progress) -> serde_json::Value {
    let permanent_upgrade_total: u32 = TIMER_PRICES.iter().sum::<u32>() * 2
        + WING_PRICES.iter().sum::<u32>()
        + SCARF_PRICES.iter().sum::<u32>();
    serde_json::json!({"permanent_upgrade_total":permanent_upgrade_total,
        "typical_run_coin_budget":[200,1000],"runs_to_max_all":[permanent_upgrade_total.div_ceil(1000),permanent_upgrade_total.div_ceil(200)],
        "price_basis":"Authored post-campaign balance requested by user; native prices retained in reference audit", "large_coin_value":10,
        "post_campaign_meters":p.post_campaign_meters,"post_campaign_target":POST_CAMPAIGN_METERS,"complete_campaign":p.missions.level>60})
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Upgrades {
    pub magnet: u8,
    pub feather: u8,
    pub wingsuit: u8,
    pub scarf: u8,
}
impl Upgrades {
    pub fn sanitize(&mut self) {
        self.magnet = self.magnet.min(6);
        self.feather = self.feather.min(6);
        self.wingsuit = self.wingsuit.min(5);
        self.scarf = self.scarf.min(5);
    }
}

#[derive(Serialize)]
pub struct Item {
    pub id: &'static str,
    pub name: &'static str,
    pub group: &'static str,
    pub level: u8,
    pub max: u8,
    pub price: Option<u32>,
    pub effect: String,
    pub affordable: bool,
    pub locked: bool,
    pub requirement: Option<String>,
}

// User-requested authored prices. Original serialized prices stay in reference/.
pub fn catalog(p: &Progress) -> Vec<Item> {
    let definitions = [
        ("magnet", "Magnet", p.upgrades.magnet, &TIMER_PRICES[..]),
        ("feather", "Feather", p.upgrades.feather, &TIMER_PRICES[..]),
        (
            "wingsuit",
            "Wingsuit lining",
            p.upgrades.wingsuit,
            &WING_PRICES[..],
        ),
        ("scarf", "Scarf weave", p.upgrades.scarf, &SCARF_PRICES[..]),
    ];
    let mut items: Vec<_> = definitions
        .into_iter()
        .map(|(id, name, level, prices)| {
            let price = prices.get(level as usize).copied();
            let max = prices.len() as u8;
            let next = (level + 1).min(max);
            let required_level = 1 + level as u32 * 10;
            let final_tier = next == max;
            let requirement = if price.is_none() {
                None
            } else if final_tier
                && (p.missions.level <= 60 || p.post_campaign_meters < POST_CAMPAIGN_METERS)
            {
                Some(format!(
                    "Complete level 60, then ride 100 km in scored play ({:.1}/100 km)",
                    p.post_campaign_meters / 1000.
                ))
            } else if !final_tier && p.missions.level < required_level {
                Some(format!("Reach level {required_level}"))
            } else {
                None
            };
            let locked = requirement.is_some();
            let effect = match (id, level == max) {
                ("magnet", true) => "20s duration".into(),
                ("feather", true) => "Feather lift: 20s duration".into(),
                ("wingsuit", true) => "35% less scarf drain".into(),
                ("scarf", true) => "+50% charge from tricks".into(),
                ("magnet", false) => format!(
                    "{}s → {}s duration",
                    5. + level as f64 * 2.5,
                    5. + next as f64 * 2.5
                ),
                ("feather", false) => format!(
                    "Feather lift: {:.1}s → {:.1}s",
                    5. + level as f64 * 2.5,
                    5. + next as f64 * 2.5
                ),
                ("wingsuit", false) => format!("{}% → {}% less scarf drain", level * 7, next * 7),
                _ => format!("+{}% → +{}% charge from tricks", level * 10, next * 10),
            };
            Item {
                id,
                name,
                group: "Upgrades",
                level,
                max,
                price,
                effect,
                affordable: !locked && price.is_some_and(|v| p.wallet >= v),
                locked,
                requirement,
            }
        })
        .collect();
    for (id, name, level, cost, effect) in [
        (
            "helmet",
            "Helmet",
            p.helmets,
            1500,
            "Saves one rock hit or bad landing",
        ),
        (
            "rescue",
            "Rescue pick",
            p.rescue_picks,
            3000,
            "Saves one missed chasm",
        ),
    ] {
        let price = (level < 3).then_some(cost);
        items.push(Item {
            id,
            name,
            group: "Safety",
            level,
            max: 3,
            price,
            effect: effect.into(),
            affordable: price.is_some_and(|v| p.wallet >= v),
            locked: false,
            requirement: None,
        });
    }
    items
}

pub fn buy(p: &mut Progress, id: &str) -> bool {
    let Some(item) = catalog(p).into_iter().find(|i| i.id == id) else {
        return false;
    };
    let Some(price) = item.price.filter(|_| item.affordable) else {
        return false;
    };
    p.wallet -= price;
    match id {
        "magnet" => p.upgrades.magnet += 1,
        "feather" => p.upgrades.feather += 1,
        "wingsuit" => p.upgrades.wingsuit += 1,
        "scarf" => p.upgrades.scarf += 1,
        "helmet" => p.helmets += 1,
        "rescue" => p.rescue_picks += 1,
        _ => unreachable!(),
    }
    true
}
