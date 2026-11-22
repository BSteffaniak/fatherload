//! Building counter panels: what each building sells or offers, drawn as item rows with icons,
//! prices and affordability, plus a details footer for the selected row and the confirmation
//! dialogs that act as receipts before a purchase.
//!
//! Every row list mirrors, index for index, the request the shell submits for that menu item
//! (see `ShellState::handle_modal`), so the labels always describe what Enter actually does.

use fatherload_simulation::{
    ExplosiveKind, RecipeKind,
    economy::{
        DeepClaimStatus, REFUEL_UNIT_COST, REPAIR_UNIT_COST, TownBuilding, UpgradeKind,
        UpgradeLock, shop_line_count, upgrade_effect, upgrade_lock_for, upgrade_maxed,
        upgrade_offers_for, upgrade_tier_name,
    },
    explosive::EXPLOSIVE_OFFERS,
    item::BlockKind,
    terrain::MineralKind,
    world::{
        BOMB_BUNDLES, MINING_ROCKETS_COST, MINING_ROCKETS_COUNT, MINING_ROCKETS_SHACK_LEVEL,
        drops::block_sale_value,
    },
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    layout::{ItemEntry, ModalContent, Section, SectionItem, UiLayout},
    pixel_art::{
        sprite::SpriteDef,
        sprites::{explosives as explosive_art, icons, menu_icons, rig, weapons as weapon_art},
    },
    prompts::{Hint, hint},
    screen::UiFrame,
};
use crate::{
    input::prompts::Prompt,
    shell::hopper::{HopperRow, hopper_rows},
};

mod people;
mod village;
pub(super) use people::{draw_person_gift, draw_person_trade, draw_village_talk};
pub(super) use village::draw_village_counter;

/// One counter row before it becomes a [`SectionItem`]: what it is, what it costs and why it
/// may be unavailable.
struct Offer {
    icon: &'static SpriteDef,
    label: String,
    detail: String,
    /// Right-aligned price or value; empty for none.
    value: String,
    value_color: Color,
    coin: bool,
    available: bool,
    /// Longer text shown in the details footer while the row is selected.
    about: String,
}

impl Offer {
    fn new(icon: &'static SpriteDef, label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            icon,
            label: label.into(),
            detail: detail.into(),
            value: String::new(),
            value_color: BONE,
            coin: false,
            available: true,
            about: String::new(),
        }
    }

    /// A price in credits, red when `credits` cannot cover it.
    fn price(mut self, cost: u32, credits: u32) -> Self {
        self.value = cost.to_string();
        self.coin = true;
        if credits < cost {
            self.value_color = RED;
            self.available = false;
        }
        self
    }

    /// A payout or credit gain.
    fn payout(mut self, amount: u32) -> Self {
        self.value = format!("+{amount}");
        self.value_color = LIME;
        self.coin = true;
        self
    }

    /// A short tag in place of a price ("FREE", "MAX", ...).
    fn tag(mut self, tag: &str, color: Color) -> Self {
        tag.clone_into(&mut self.value);
        self.value_color = color;
        self
    }

    const fn unavailable_if(mut self, condition: bool) -> Self {
        if condition {
            self.available = false;
        }
        self
    }

    fn about(mut self, about: impl Into<String>) -> Self {
        self.about = about.into();
        self
    }
}

/// Builds the row items for `offers` (selection from the shell) and the details footer text.
fn offer_items(offers: Vec<Offer>, selected: usize) -> (Vec<SectionItem>, Option<String>) {
    let about = offers
        .get(selected)
        .map(|offer| offer.about.clone())
        .filter(|about| !about.is_empty());
    let items = offers
        .into_iter()
        .enumerate()
        .map(|(index, offer)| {
            SectionItem::item(ItemEntry {
                icon: Some(offer.icon),
                label: offer.label,
                detail: offer.detail,
                value: offer.value,
                value_color: offer.value_color,
                coin: offer.coin,
                selected: index == selected,
                disabled: !offer.available,
            })
        })
        .collect();
    (items, about)
}

/// The wallet line every counter starts with.
fn credits_item(credits: u32) -> SectionItem {
    SectionItem::icon_stat(&icons::CREDITS, "Your credits", credits.to_string(), GOLD)
}

/// Draws a counter panel: a status section, the offers, and the selected row's details.
fn draw_counter(
    ui: &UiFrame<'_, '_>,
    draw: &mut RaylibDrawHandle<'_>,
    title: &str,
    hints: &[Hint],
    status: Vec<SectionItem>,
    heading: &str,
    offers: Vec<Offer>,
) {
    let (items, about) = offer_items(offers, ui.seat.selected_menu_item);
    let mut sections = Vec::new();
    if !status.is_empty() {
        sections.push(Section::new("Status", SKY_LIGHT, status));
    }
    sections.push(Section::new(heading, GOLD, items));
    UiLayout::screen(draw).modal_with_render_plan(
        title,
        &super::quests::quest_tab_subtitle(ui),
        &ModalContent::new(sections)
            .footer(about)
            .prompts(&super::quests::with_quest_tab(ui, hints)),
    );
}

const COUNTER_HELP: &[Hint] = &[
    hint(Prompt::Navigate, "select"),
    hint(Prompt::Confirm, "confirm"),
    hint(Prompt::Back, "close"),
];

/// What a fuel or repair counter bills: the whole missing amount, capped by the player's
/// credits, as `(units delivered, credits charged, units missing, full price)`.
///
/// This mirrors the simulation, which currently resolves every small/half/full choice as a full
/// service (`menu_item` is ignored), so a quote or receipt never promises a cheaper partial fill
/// than the one that will actually be billed.
fn service_charge(missing: f32, unit_cost: u32, credits: u32) -> (u32, u32, u32, u32) {
    let missing = missing.ceil().max(0.0) as u32;
    let full_cost = missing * unit_cost;
    let charge = full_cost.min(credits);
    (charge / unit_cost.max(1), charge, missing, full_cost)
}

/// Small / half / full service rows for the fuel station and repair garage, each quoting what
/// the counter will actually charge (see [`service_charge`]).
fn service_offers(
    icon: &'static SpriteDef,
    labels: [&str; 3],
    unit: &str,
    missing: f32,
    unit_cost: u32,
    credits: u32,
) -> Vec<Offer> {
    let (units, charge, missing, full_cost) = service_charge(missing, unit_cost, credits);
    labels
        .into_iter()
        .map(|label| {
            if missing == 0 {
                return Offer::new(icon, label, format!("{unit} already full"))
                    .tag("FULL", LIME)
                    .unavailable_if(true);
            }
            if units == 0 {
                return Offer::new(icon, label, format!("+{missing} {unit} to fill up"))
                    .price(full_cost, credits);
            }
            let detail = if units < missing {
                format!("+{units} of {missing} {unit}, all you can pay for")
            } else {
                format!("+{units} {unit}, fills it up")
            };
            Offer::new(icon, label, detail)
                .price(charge, credits)
                .about(format!(
                    "Every choice fills it up: {units} {unit} at {unit_cost} cr each. Short on credits, the counter sells as much as you can pay for."
                ))
        })
        .collect()
}

pub(super) fn draw_fuel(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let hud = ui.view.hud;
    let status = vec![
        SectionItem::meter("Fuel tank", hud.fuel, hud.fuel_capacity, AMBER, RED),
        credits_item(hud.credits),
    ];
    let offers = service_offers(
        &icons::FUEL,
        ["Top up", "Half refill", "Fill the tank"],
        "fuel",
        hud.fuel_capacity - hud.fuel,
        REFUEL_UNIT_COST,
        hud.credits,
    );
    draw_counter(
        ui,
        draw,
        "Fuel Station",
        COUNTER_HELP,
        status,
        "Pump",
        offers,
    );
}

pub(super) fn draw_repair(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let hud = ui.view.hud;
    let status = vec![
        SectionItem::meter("Hull integrity", hud.hull, hud.max_hull, LEAF, RED),
        credits_item(hud.credits),
    ];
    let mut offers = service_offers(
        &menu_icons::HAMMER,
        ["Patch job", "Half repair", "Full overhaul"],
        "hull",
        hud.max_hull - hud.hull,
        REPAIR_UNIT_COST,
        hud.credits,
    );
    // `shell::GARAGE_COUNTER_ROW`: the paint bay.
    offers.push(
        Offer::new(&rig::ICON_SPRAY_CAN, "Paint bay", "Paint, flames and attachments")
            .tag("FREE", LIME)
            .about("Iona's spray booth: repaint your rig, pick a flame colour, drill style, decal and attachments. Try anything on before you commit."),
    );
    draw_counter(
        ui,
        draw,
        "Repair Garage",
        COUNTER_HELP,
        status,
        "Repairs",
        offers,
    );
}

/// The icon for an upgrade line in the shop.
#[allow(
    clippy::match_same_arms,
    reason = "the rig systems borrow the kit's icons until their own art lands"
)]
pub(super) const fn upgrade_icon(kind: UpgradeKind) -> &'static SpriteDef {
    match kind {
        UpgradeKind::Drill => &rig::DRILL_DOWN,
        UpgradeKind::FuelTank => &icons::FUEL,
        UpgradeKind::CargoBay => &icons::CARGO,
        UpgradeKind::Engine => &menu_icons::GEAR,
        UpgradeKind::Hull => &icons::HULL,
        UpgradeKind::Radiator => &icons::HEAT,
        UpgradeKind::Scanner => &icons::SCANNER,
        UpgradeKind::BombPack => &icons::BOMB,
        UpgradeKind::DrillMount => &rig::DRILL_UP,
        UpgradeKind::HeatShield => &icons::HEAT_SHIELD,
        UpgradeKind::LavaPlating => &icons::LAVA_PLATING,
        UpgradeKind::GasScrubber => &icons::GAS_SCRUBBER,
        UpgradeKind::PressureBracing => &icons::PRESSURE_BRACING,
        UpgradeKind::BoulderDeflector => &icons::BOULDER_DEFLECTOR,
        UpgradeKind::AltitudeIntake => &icons::ALTITUDE_INTAKE,
        UpgradeKind::DeIcer => &icons::DE_ICER,
        UpgradeKind::PressurizedHull => {
            &crate::rendering::pixel_art::sprites::moon::PRESSURIZED_HULL
        }
        UpgradeKind::LifeSupport => &crate::rendering::pixel_art::sprites::moon::LIFE_SUPPORT,
        // The Deep Descent's rig systems (Part III F10), placeholder icons from the kit.
        UpgradeKind::DescentPressureHull => &icons::PRESSURE_BRACING,
        UpgradeKind::BioFilter => &icons::GAS_SCRUBBER,
        UpgradeKind::LampII => &icons::SCANNER,
        UpgradeKind::SonarSuite => &icons::SCANNER,
        UpgradeKind::CryoHull => &icons::DE_ICER,
        UpgradeKind::GravityAnchor => &menu_icons::GEAR,
        UpgradeKind::PhaseDrill => &rig::DRILL_DOWN,
        UpgradeKind::HeartKey => &icons::HEAT_SHIELD,
        UpgradeKind::PressureRating => {
            &crate::rendering::pixel_art::sprites::deep_gear::PRESSURE_RATING
        }
        UpgradeKind::Headlamp => &crate::rendering::pixel_art::sprites::deep_gear::HEADLAMP,
    }
}

fn shop_offers(ui: &UiFrame<'_, '_>) -> Vec<Offer> {
    let credits = ui.player.credits;
    let settings = ui.snapshot.terrain.settings();
    let deepest = ui.snapshot.progression.deepest_tile_reached;
    let rules = ui.snapshot.logistics.rules;
    // Classic worlds list their own lines only: the rig systems are Deep Descent gear.
    upgrade_offers_for(ui.player, rules)
        .iter()
        .take(shop_line_count(rules))
        .map(|offer| {
            let icon = upgrade_icon(offer.kind);
            let max = offer.max_level;
            if upgrade_maxed(offer) {
                return Offer::new(
                    icon,
                    offer.name,
                    format!("Tier {}/{max} fitted", offer.level),
                )
                .tag("MAX", LIME)
                .unavailable_if(true)
                .about(offer.description);
            }
            let tier = if offer.kind == UpgradeKind::BombPack {
                upgrade_tier_name(offer.kind, offer.level).to_owned()
            } else {
                format!(
                    "Tier {}/{max} {}",
                    offer.level + 1,
                    upgrade_tier_name(offer.kind, offer.level)
                )
            };
            // The measured headline of what the next tier changes (the showcase card beside
            // the panel shows the rest), then the line's description.
            let about = fatherload_simulation::upgrade_preview::upgrade_preview_for(
                ui.player,
                offer.kind,
                settings.generator.sky_bands().as_ref(),
                rules,
            )
            .and_then(|preview| preview.headline().map(str::to_owned))
            .or_else(|| upgrade_effect(ui.player, offer.kind))
            .map_or_else(
                || offer.description.to_owned(),
                |effect| format!("{effect}. {}.", offer.description),
            );
            if let Some(lock) = upgrade_lock_for(
                offer,
                settings,
                deepest,
                rules,
                ui.snapshot.logistics.felt_systems,
            ) {
                let tag = match lock {
                    UpgradeLock::Band(_)
                    | UpgradeLock::Descent(_)
                    | UpgradeLock::Depth { .. }
                    | UpgradeLock::NotStockedHere => "LOCKED",
                    UpgradeLock::Unfelt(_) => "UNFELT",
                    UpgradeLock::NoSky => "NO SKY",
                    UpgradeLock::DescentOnly => "DEEP",
                };
                return Offer::new(icon, offer.name, tier)
                    .tag(tag, RED)
                    .unavailable_if(true)
                    .about(format!(
                        "{} ({} cr). {}.",
                        lock.reason(),
                        offer.cost,
                        offer.description
                    ));
            }
            Offer::new(icon, offer.name, tier)
                .price(offer.cost, credits)
                .about(about)
        })
        .collect()
}

pub(super) fn draw_shop(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let status = vec![credits_item(ui.player.credits)];
    draw_counter(
        ui,
        draw,
        "Upgrade Shop",
        &[
            hint(Prompt::Navigate, "select"),
            hint(Prompt::Tab, "browse"),
            hint(Prompt::Confirm, "buy"),
            hint(Prompt::Back, "close"),
        ],
        status,
        "Upgrades",
        shop_offers(ui),
    );
    super::rewards::draw_shop_showcase(draw, ui);
}

/// The receipt-style confirmation for a pending purchase: the item, its price and the balance
/// after paying.
fn draw_purchase_confirm(
    draw: &mut RaylibDrawHandle<'_>,
    title: &str,
    offer: Option<Offer>,
    credits: u32,
) {
    let mut items = Vec::new();
    let mut cost = None;
    if let Some(offer) = offer {
        cost = offer.value.parse::<u32>().ok().filter(|_| offer.coin);
        let about = offer.about.clone();
        let (rows, _) = offer_items(vec![offer], 0);
        items.extend(rows);
        if !about.is_empty() {
            items.push(SectionItem::text(about));
        }
    }
    let mut ledger = vec![SectionItem::icon_stat(
        &icons::CREDITS,
        "Credits now",
        credits.to_string(),
        GOLD,
    )];
    if let Some(cost) = cost {
        ledger.push(SectionItem::stat("Price", format!("-{cost}"), ORANGE));
        let (label, value, color) = if credits >= cost {
            ("Balance after", (credits - cost).to_string(), BONE)
        } else {
            ("Short by", (cost - credits).to_string(), RED)
        };
        ledger.push(SectionItem::stat(label, value, color));
    }
    UiLayout::screen(draw).modal_with_render_plan(
        title,
        "",
        &ModalContent::new(vec![
            Section::new("Order", GOLD, items),
            Section::new("Receipt", SKY_LIGHT, ledger),
        ])
        .prompts(&[hint(Prompt::Confirm, "confirm"), hint(Prompt::Back, "back")]),
    );
}

pub(super) fn draw_shop_confirm(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let offer = shop_offers(ui).into_iter().nth(ui.seat.selected_menu_item);
    draw_purchase_confirm(draw, "Confirm Upgrade", offer, ui.player.credits);
    super::rewards::draw_shop_showcase(draw, ui);
}

pub(super) fn draw_fuel_confirm(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let hud = ui.view.hud;
    let offer = service_offers(
        &icons::FUEL,
        ["Top up", "Half refill", "Fill the tank"],
        "fuel",
        hud.fuel_capacity - hud.fuel,
        REFUEL_UNIT_COST,
        hud.credits,
    )
    .into_iter()
    .nth(ui.seat.selected_menu_item);
    draw_purchase_confirm(draw, "Confirm Refuel", offer, hud.credits);
}

pub(super) fn draw_repair_confirm(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let hud = ui.view.hud;
    let offer = service_offers(
        &menu_icons::HAMMER,
        ["Patch job", "Half repair", "Full overhaul"],
        "hull",
        hud.max_hull - hud.hull,
        REPAIR_UNIT_COST,
        hud.credits,
    )
    .into_iter()
    .nth(ui.seat.selected_menu_item);
    draw_purchase_confirm(draw, "Confirm Repair", offer, hud.credits);
}

/// The loan row shared by the bank and HQ: borrow when debt-free, otherwise repay.
fn finance_offer(ui: &UiFrame<'_, '_>) -> Offer {
    let player = ui.player;
    if player.loan_debt == 0 {
        let level = ui.snapshot.progression.town_development.bank_level;
        let advance = 250 + u32::from(level) * 150;
        let premium = 50 + u32::from(level) * 25;
        Offer::new(
            &menu_icons::BANK,
            "Take out a loan",
            format!("Repay {} cr later", advance + premium),
        )
        .payout(advance)
        .about(format!(
            "The bank advances {advance} cr now; you owe {} cr, paid back from this counter.",
            advance + premium
        ))
    } else {
        let payment = player.loan_debt.min(player.credits);
        Offer::new(
            &menu_icons::BANK,
            "Repay loan",
            format!("{} cr still owed", player.loan_debt),
        )
        .price(payment, player.credits)
        .unavailable_if(payment == 0)
        .about("Pays off as much of the debt as your credits cover.")
    }
}

pub(super) fn draw_bank(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let player = ui.player;
    let town = &ui.snapshot.progression.town_development;
    let max_tier = 1_u8.saturating_add(town.bank_level).min(4);
    let next_tier = player.insurance_tier.saturating_add(1).min(max_tier);
    let insurance_cost = (70 + u32::from(next_tier) * 55)
        .saturating_sub(u32::from(town.bank_level).saturating_mul(15));
    let insurance = if player.insured {
        Offer::new(
            &menu_icons::INSURANCE,
            "Rig insurance",
            format!("Tier {} policy in force", player.insurance_tier),
        )
        .tag("ACTIVE", LIME)
        .unavailable_if(true)
    } else if player.insurance_tier >= max_tier {
        Offer::new(
            &menu_icons::INSURANCE,
            "Rig insurance",
            "Highest tier this bank writes",
        )
        .tag("MAX", FOG)
        .unavailable_if(true)
    } else {
        Offer::new(
            &menu_icons::INSURANCE,
            "Rig insurance",
            format!("Tier {next_tier} policy"),
        )
        .price(insurance_cost, player.credits)
        .about("Rescue insurance: higher tiers cut rescue fees and cargo loss. A bigger bank sells higher tiers for less.")
    };
    let contracts = ui.snapshot.progression.active_side_contracts.len();
    let side = Offer::new(
        &menu_icons::CONTRACT,
        "Take a side contract",
        format!("{contracts}/3 side contracts active"),
    )
    .tag("JOB", SKY_LIGHT)
    .unavailable_if(contracts >= 3)
    .about("Adds a side job for the depot to pay out. Up to three at once.");
    let mut status = vec![credits_item(player.credits)];
    status.push(SectionItem::stat(
        "Loan debt",
        format!("{} cr", player.loan_debt),
        if player.loan_debt > 0 { ORANGE } else { FOG },
    ));
    draw_counter(
        ui,
        draw,
        "Iron Ledger Bank",
        COUNTER_HELP,
        status,
        "Teller",
        vec![finance_offer(ui), insurance, side],
    );
}

pub(super) fn draw_explosives(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let credits = ui.player.credits;
    let shack = ui
        .snapshot
        .progression
        .town_development
        .explosives_shack_level;
    let bonus = u32::from(shack / 2);
    let [(small_count, small_cost), (big_count, big_cost)] = BOMB_BUNDLES;
    // Named for the seat's own device (B, Num0, RB...), never a hard-coded key.
    let bomb_key = crate::input::prompts::inline_text(Prompt::Bomb, super::prompts::menu_device())
        .unwrap_or_else(|| "the bomb button".to_owned());
    let bundle = |label: &str, count: u32, cost: u32| {
        Offer::new(&icons::BOMB, label, format!("+{} bombs", count + bonus))
            .price(cost, credits)
            .about(if bonus > 0 {
                format!(
                    "{count} bombs plus {bonus} extra from the shack's upgrades. Press {bomb_key} \
                     underground to drop one."
                )
            } else {
                format!("{count} bombs. Press {bomb_key} underground to drop one.")
            })
    };
    let rockets_locked = shack < MINING_ROCKETS_SHACK_LEVEL;
    let rockets = Offer::new(
        &menu_icons::ROCKET,
        "Mining rockets",
        if rockets_locked {
            format!("Shack level {MINING_ROCKETS_SHACK_LEVEL} needed")
        } else {
            format!("+{MINING_ROCKETS_COUNT} shaped charges")
        },
    )
    .price(MINING_ROCKETS_COST, credits)
    .unavailable_if(rockets_locked)
    .about(
        "Shaped charges for hard rock. Upgrade the shack through town development to stock them.",
    );
    let test = Offer::new(
        &menu_icons::GIFT,
        "Free test charge",
        "+1 bomb, on the house",
    )
    .tag("FREE", LIME)
    .about("Nix hands out a sample charge to anyone who asks nicely.");
    let mut status = vec![
        credits_item(credits),
        SectionItem::icon_stat(
            &icons::BOMB,
            "Bombs carried",
            ui.player.bombs.to_string(),
            ORANGE,
        ),
    ];
    for (kind, count) in ui.player.carried_explosives() {
        if kind == fatherload_simulation::ExplosiveKind::Bomb {
            continue;
        }
        status.push(SectionItem::icon_stat(
            explosive_art::icon(kind),
            kind.name(),
            count.to_string(),
            ORANGE,
        ));
    }
    let mut offers = vec![
        bundle("Small bomb pack", small_count, small_cost),
        bundle("Standard bomb pack", big_count, big_cost),
        rockets,
        test,
    ];
    offers.extend(special_offers(shack, credits, &bomb_key));
    let sky_content = ui.snapshot.terrain.settings().generator.has_sky_content();
    let has_sky = ui.snapshot.terrain.settings().generator.has_sky();
    offers.extend(arms_offers(ui.player, shack, credits, sky_content, has_sky));
    if let Some(mounted) = ui.player.mounted_weapon {
        status.push(SectionItem::icon_stat(
            weapon_art::icon(mounted.kind),
            mounted.kind.name(),
            crate::shell::arms::mk_name(mounted.mk).to_owned(),
            MINT,
        ));
    }
    status.push(SectionItem::icon_stat(
        &weapon_art::ICON_BATTERY,
        "Capacitor",
        format!("{:.0}", ui.player.battery_capacity()),
        MINT,
    ));
    draw_counter(
        ui,
        draw,
        "Explosives Shack",
        COUNTER_HELP,
        status,
        "For sale",
        offers,
    );
}

/// The shack's arms rack: each weapon (buy, then upgrade), the capacitor and rocket ammo.
#[allow(clippy::too_many_lines, reason = "one arm per arms rack row")]
fn arms_offers(
    player: &fatherload_simulation::Player,
    shack: u8,
    credits: u32,
    sky_content: bool,
    has_sky: bool,
) -> Vec<Offer> {
    use crate::shell::arms::{ARMS_ROWS, ArmsRow, mk_name};
    use fatherload_simulation::weapon::{
        MAX_BATTERY_LEVEL, MAX_WEAPON_MK, battery_capacity, battery_upgrade_cost,
    };
    let fire =
        crate::input::prompts::inline_text(Prompt::FireWeapon, super::prompts::menu_device())
            .unwrap_or_else(|| "the trigger".to_owned());
    let cycle =
        crate::input::prompts::inline_text(Prompt::CycleWeapon, super::prompts::menu_device())
            .unwrap_or_else(|| "the next-weapon button".to_owned());
    ARMS_ROWS
        .iter()
        .map(|row| match *row {
            ArmsRow::Weapon(weapon) => {
                // Act II weapons (T21): a world with sky content, and sky materials.
                let missing = weapon
                    .materials()
                    .iter()
                    .find(|(material, count)| player.cargo_bay.material(*material) < *count)
                    .map(|(material, count)| format!("Needs {count} {}", material.name()));
                let no_sky = weapon.act_two() && !sky_content;
                let locked = shack < weapon.shack_level() || no_sky || missing.is_some();
                match player.weapon_rack.mk(weapon) {
                    None => Offer::new(
                        weapon_art::icon(weapon),
                        weapon.name(),
                        if no_sky {
                            "Sky worlds only".to_owned()
                        } else if shack < weapon.shack_level() {
                            format!("Shack level {} needed", weapon.shack_level())
                        } else if let Some(missing) = missing {
                            missing
                        } else if weapon.act_two() {
                            "Act II hardpoint weapon".to_owned()
                        } else {
                            "Hardpoint weapon".to_owned()
                        },
                    )
                    .price(weapon.cost(), credits)
                    .unavailable_if(locked)
                    .about(format!(
                        "{} Hold {fire} to fire; {cycle} swaps weapons.",
                        weapon.description()
                    )),
                    Some(mk) if mk >= MAX_WEAPON_MK => Offer::new(
                        weapon_art::icon(weapon),
                        format!("{} {}", weapon.name(), mk_name(mk)),
                        "Fully upgraded",
                    )
                    .unavailable_if(true)
                    .about(weapon.description()),
                    Some(mk) => {
                        let now = weapon.stats(mk);
                        let next = weapon.stats(mk + 1);
                        Offer::new(
                            weapon_art::icon(weapon),
                            format!("{} {}", weapon.name(), mk_name(mk + 1)),
                            if weapon == fatherload_simulation::weapon::WeaponKind::TetherHarpoon {
                                // The line does no damage: its upgrades reach farther.
                                format!(
                                    "Line {:.0} -> {:.0} tiles",
                                    now.range / fatherload_simulation::terrain::TILE_SIZE,
                                    next.range / fatherload_simulation::terrain::TILE_SIZE
                                )
                            } else {
                                format!("Damage {:.0} -> {:.0}", now.damage, next.damage)
                            },
                        )
                        .price(weapon.upgrade_cost(mk + 1), credits)
                        .about(format!(
                            "Harder hits, faster cycling and less energy per shot. {}",
                            weapon.description()
                        ))
                    }
                }
            }
            ArmsRow::Battery => {
                let level = player.weapon_rack.battery_level;
                if level >= MAX_BATTERY_LEVEL {
                    Offer::new(&weapon_art::ICON_BATTERY, "Capacitor", "Fully upgraded")
                        .unavailable_if(true)
                } else {
                    Offer::new(
                        &weapon_art::ICON_BATTERY,
                        format!("Capacitor level {}", level + 1),
                        format!(
                            "{:.0} -> {:.0} energy",
                            battery_capacity(level),
                            battery_capacity(level + 1)
                        ),
                    )
                    .price(battery_upgrade_cost(level + 1), credits)
                    .about(
                        "Energy weapons draw from the capacitor, which recharges from your fuel. \
                         A bigger cell fires longer and recharges faster.",
                    )
                }
            }
            ArmsRow::Ammo(ammo) => {
                let (count, cost) = ammo.pack();
                Offer::new(
                    weapon_art::ammo_icon(ammo),
                    format!("{} pack", ammo.name()),
                    format!(
                        "+{count} {} (carrying {})",
                        ammo.plural(),
                        player.ammo(ammo)
                    ),
                )
                .price(cost, credits)
                .about("Ammunition for the rocket launcher. Takes no cargo space.")
            }
            ArmsRow::Gear(gear) => gear_offer(player, gear, credits, has_sky),
            ArmsRow::PilotArms(purchase) => pilot_arms_offer(player, purchase, credits),
        })
        .collect()
}

/// A gunsmith row (WP-PILOT): a personal weapon (owned once) or a box of personal ammo.
fn pilot_arms_offer(
    player: &fatherload_simulation::Player,
    purchase: fatherload_simulation::pilot::arsenal::PilotArmsPurchase,
    credits: u32,
) -> Offer {
    use fatherload_simulation::pilot::arsenal::{MAX_PILOT_AMMO, PilotArmsPurchase};
    match purchase {
        PilotArmsPurchase::Weapon(weapon) => {
            let owned = player.pilot_arsenal.owned.contains(&weapon);
            let stats = weapon.stats(0);
            let detail = stats.ammo.map_or_else(
                || format!("Pilot's arm, {:.0} damage", stats.damage),
                |ammo| format!("Pilot's arm, uses {}", ammo.plural()),
            );
            let row = Offer::new(weapon_art::icon(weapon), weapon.name(), detail).about(format!(
                "{} Swap what you hold on foot with {}.",
                weapon.description(),
                crate::input::prompts::inline_text(
                    Prompt::CycleWeapon,
                    super::prompts::menu_device()
                )
                .unwrap_or_else(|| "the next-weapon button".to_owned())
            ));
            if owned {
                row.tag("OWNED", MINT).unavailable_if(true)
            } else {
                row.price(purchase.price(), credits)
            }
        }
        PilotArmsPurchase::Ammo(ammo) => {
            let (count, cost) = ammo.pack();
            let carried = player.pilot_arsenal.ammo(ammo);
            Offer::new(
                weapon_art::ammo_icon(ammo),
                format!("{} box", ammo.name()),
                format!("+{count} (carrying {carried})"),
            )
            .price(cost, credits)
            .unavailable_if(carried >= MAX_PILOT_AMMO)
            .about("Personal ammunition for the pilot's guns, kept apart from the rig's.")
        }
    }
}

/// The icon of a piece of pilot gear.
#[must_use]
pub const fn gear_icon(
    gear: fatherload_simulation::pilot::PilotGearKind,
) -> &'static crate::rendering::pixel_art::sprite::SpriteDef {
    use fatherload_simulation::pilot::PilotGearKind;
    match gear {
        PilotGearKind::Jetpack => &icons::JETPACK,
        PilotGearKind::Suit | PilotGearKind::Thermal | PilotGearKind::HeatWeave => &icons::HELMET,
        PilotGearKind::Armor => &icons::PILOT_ARMOR,
        PilotGearKind::Headlamp => &icons::HEADLAMP,
        PilotGearKind::Boots => &icons::BOOTS,
        PilotGearKind::Glider => &icons::GLIDER,
        PilotGearKind::Grapple => &icons::GRAPPLE,
        PilotGearKind::RigLink => &icons::RIG_LINK,
    }
}

/// A pilot gear row (Part II F5): the next tier of a piece of pilot gear, or "top tier".
fn gear_offer(
    player: &fatherload_simulation::Player,
    gear: fatherload_simulation::pilot::PilotGearKind,
    credits: u32,
    has_sky: bool,
) -> Offer {
    let owned = gear.tier(player.pilot_gear);
    let icon = gear_icon(gear);
    if gear.needs_sky() && !has_sky {
        // The simulation refuses it too: this world has no high cold to keep out.
        return Offer::new(icon, gear.name(), "No sky on this world")
            .tag("NO SKY", RED)
            .unavailable_if(true)
            .about(format!(
                "{}. This world has no high sky, so there is no cold to keep out.",
                gear.blurb()
            ));
    }
    gear.price(owned).map_or_else(
        || {
            Offer::new(icon, format!("{} tier {owned}", gear.name()), "Top tier")
                .unavailable_if(true)
                .about(gear.blurb())
        },
        |price| {
            Offer::new(
                icon,
                format!("{} tier {}", gear.name(), owned + 1),
                "Pilot gear (on foot)",
            )
            .price(price, credits)
            .about(gear.blurb())
        },
    )
}

/// The shack's special charge packs (C4, sticky bombs, flares...), locked below their level.
fn special_offers(shack: u8, credits: u32, bomb_key: &str) -> Vec<Offer> {
    let cycle_key =
        crate::input::prompts::inline_text(Prompt::CycleExplosive, super::prompts::menu_device())
            .unwrap_or_else(|| "the next-charge button".to_owned());
    let detonate_key =
        crate::input::prompts::inline_text(Prompt::Detonate, super::prompts::menu_device())
            .unwrap_or_else(|| "the detonator".to_owned());
    EXPLOSIVE_OFFERS
        .iter()
        .map(|offer| {
        let locked = shack < offer.shack_level;
        let how = match offer.kind {
            ExplosiveKind::C4 => format!(
                " Press {cycle_key} to select it, {bomb_key} to place it and {detonate_key} to fire every charge."
            ),
            _ => format!(" Press {cycle_key} to select it and {bomb_key} to use it."),
        };
        Offer::new(
            explosive_art::icon(offer.kind),
            format!("{} pack", offer.kind.name()),
            if locked {
                format!("Shack level {} needed", offer.shack_level)
            } else {
                format!("+{} {}", offer.count, offer.kind.plural())
            },
        )
        .price(offer.cost, credits)
        .unavailable_if(locked)
        .about(format!("{}{how}", offer.kind.description()))
        })
        .collect()
}

pub(super) fn draw_salvage(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let level = ui.snapshot.progression.town_development.salvage_yard_level;
    let lost = ui.view.lost_cargo.is_some();
    let locked = |needed: u8| format!("Yard level {needed} needed");
    let recover = Offer::new(
        &icons::CARGO,
        "Recover lost cargo",
        if lost {
            "Haul back what you dropped"
        } else {
            "No lost cargo out there"
        },
    )
    .tag("FEE", AMBER)
    .unavailable_if(!lost)
    .about(format!(
        "A crew hauls your dropped cargo home for {} cr per item.",
        12_u32.saturating_sub(u32::from(level) * 3)
    ));
    let hull_full = ui.player.hull >= ui.player.max_hull();
    let patch = Offer::new(
        &menu_icons::HAMMER,
        "Patch hull",
        "+12% hull from scrap plate",
    )
    .tag("FREE", LIME)
    .unavailable_if(hull_full)
    .about("Mara welds scrap plate over the worst dents, free of charge.");
    let drone = Offer::new(
        &menu_icons::DRONE,
        "Launch salvage drone",
        if level < 2 {
            locked(2)
        } else if lost {
            "Fetch cargo and map the site".to_owned()
        } else {
            "No lost cargo to fetch".to_owned()
        },
    )
    .tag("FEE", AMBER)
    .unavailable_if(level < 2 || !lost)
    .about("Like a recovery run, and the drone also maps the tiles around the wreck.");
    let part = Offer::new(
        &menu_icons::GEAR,
        "Recover wrecked part",
        if level < 3 {
            locked(3)
        } else {
            "Pull a rig part from the wreck pile".to_owned()
        },
    )
    .tag("FREE", LIME)
    .unavailable_if(level < 3)
    .about("Strips a usable rig part from rescued wrecks.");
    let collapse = Offer::new(
        &menu_icons::COLLAPSE,
        "Clear collapse zones",
        if level < 4 {
            locked(4)
        } else {
            "45 cr per cleared warning".to_owned()
        },
    )
    .tag("PAYS", LIME)
    .unavailable_if(level < 4)
    .about("Crews shore up flagged collapse zones and the town pays a bounty for each.");
    let tip = Offer::new(
        &menu_icons::SCRAP,
        "Sell a scrap tip",
        "Point the crew at a wreck",
    )
    .payout(35)
    .about("Mara pays for leads on scrap worth hauling.");
    let status = vec![
        credits_item(ui.player.credits),
        SectionItem::meter("Hull", ui.player.hull, ui.player.max_hull(), LEAF, RED),
    ];
    draw_counter(
        ui,
        draw,
        "Salvage Yard",
        COUNTER_HELP,
        status,
        "Services",
        vec![recover, patch, drone, part, collapse, tip],
    );
}

/// The active contract as one line: title, progress and reward.
fn contract_line(ui: &UiFrame<'_, '_>) -> (String, bool) {
    let contract = &ui.snapshot.progression.contracts.active;
    let progress = contract.progress(ui.player);
    (
        format!(
            "{}: {}/{} {}",
            contract.title,
            progress.min(contract.required),
            contract.required,
            contract.target.name()
        ),
        progress >= contract.required,
    )
}

pub(super) fn draw_headquarters(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let progression = ui.snapshot.progression;
    let contract = &progression.contracts.active;
    let (line, done) = contract_line(ui);
    let mut offers = vec![
        Offer::new(&menu_icons::CONTRACT, "Complete depot work", line)
            .tag(
                if done { "READY" } else { "OPEN" },
                if done { LIME } else { FOG },
            )
            .about("Hands in the active contract, or a finished side contract if that is all you have."),
        Offer::new(&menu_icons::LETTER, "Read briefing", "Director Vale's latest orders")
            .about("The briefing appears as a message at the bottom of the screen."),
        finance_offer(ui),
    ];
    let unlocked = progression.deep_claim_status == DeepClaimStatus::Unlocked;
    if unlocked {
        offers.extend([
            Offer::new(
                &menu_icons::HOUSE,
                "Town development",
                "Invest in the settlement",
            )
            .about("Upgrade town buildings to unlock better services and stock."),
            Offer::new(
                &menu_icons::FLAG,
                "Expedition board",
                format!(
                    "{} offers, {} active",
                    progression.expedition_offers.len(),
                    progression.active_expeditions.len()
                ),
            ),
            Offer::new(
                &menu_icons::BOOK,
                "Research log",
                "Discoveries and warnings",
            ),
            Offer::new(
                &menu_icons::ANVIL,
                "Crafting bench",
                "Build kits from materials",
            ),
        ]);
    }
    offers.push(
        Offer::new(
            &menu_icons::HOUSE,
            "Land office",
            "Building plots and furniture",
        )
        .about("Buy a plot of land, hire a builder and make a home of it."),
    );
    let (items, about) = offer_items(offers, ui.seat.selected_menu_item);
    let mut sections = vec![
        Section::new(
            "Contract",
            SKY_LIGHT,
            vec![
                SectionItem::icon_stat(
                    &menu_icons::CONTRACT,
                    contract.title.clone(),
                    format!("{} cr", contract.reward),
                    GOLD,
                ),
                SectionItem::meter(
                    contract.target.name(),
                    contract.progress(ui.player).min(contract.required) as f32,
                    contract.required.max(1) as f32,
                    LEAF,
                    LEAF,
                ),
            ],
        ),
        Section::new("Desk", GOLD, items),
    ];
    if !unlocked {
        sections.push(Section::new(
            "Locked",
            FOG,
            vec![SectionItem::text(
                "Secure the Deep Claim to open town development, expeditions, research and crafting.",
            )],
        ));
    }
    UiLayout::screen(draw).modal_with_render_plan(
        "Borealis HQ",
        &super::quests::quest_tab_subtitle(ui),
        &ModalContent::new(sections)
            .footer(about)
            .prompts(&super::quests::with_quest_tab(ui, COUNTER_HELP)),
    );
}

/// Short material names for recipe costs.
fn material_list(cost: &[(fatherload_simulation::terrain::StrategicResourceKind, u32)]) -> String {
    cost.iter()
        .map(|(material, count)| format!("{count} {}", material.name()))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn draw_crafting(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let bay = &ui.player.cargo_bay;
    let offers = RecipeKind::ALL
        .into_iter()
        .map(|recipe| {
            let can_craft = recipe
                .cost()
                .iter()
                .all(|(material, needed)| bay.material(*material) >= *needed);
            Offer::new(
                &menu_icons::ANVIL,
                recipe.name(),
                material_list(recipe.cost()),
            )
            .tag(
                if can_craft { "CRAFT" } else { "NEED" },
                if can_craft { LIME } else { RED },
            )
            .unavailable_if(!can_craft)
            .about(recipe.description())
        })
        .collect();
    let status = if bay.materials().next().is_none() {
        vec![SectionItem::text(
            "No strategic materials yet. Process ore or find relics deep down.",
        )]
    } else {
        bay.materials()
            .map(|(material, count)| SectionItem::stat(material.name(), count.to_string(), MINT))
            .collect()
    };
    draw_counter(
        ui,
        draw,
        "Crafting Bench",
        COUNTER_HELP,
        status,
        "Recipes",
        offers,
    );
}

pub(super) fn draw_town_development(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let town = &ui.snapshot.progression.town_development;
    let alloy = ui
        .player
        .cargo_bay
        .material(fatherload_simulation::terrain::StrategicResourceKind::AncientAlloy);
    let offers = TownBuilding::ALL
        .into_iter()
        .map(|building| {
            let level = town.level(building);
            let needs_alloy = level >= 1;
            let detail = if needs_alloy {
                format!("Level {level} to {} + 1 Ancient Alloy", level + 1)
            } else {
                format!("Level {level} to {}", level + 1)
            };
            Offer::new(&menu_icons::HOUSE, building.name(), detail)
                .price(town.upgrade_cost(building), ui.player.credits)
                .unavailable_if(needs_alloy && alloy == 0)
        })
        .collect();
    let status = vec![
        credits_item(ui.player.credits),
        SectionItem::stat("Ancient Alloy", alloy.to_string(), MINT),
    ];
    draw_counter(
        ui,
        draw,
        "Town Development",
        COUNTER_HELP,
        status,
        "Projects",
        offers,
    );
}

pub(super) fn draw_expedition_board(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let progression = ui.snapshot.progression;
    let mut offers: Vec<Offer> = progression
        .expedition_offers
        .iter()
        .map(|expedition| {
            Offer::new(
                &menu_icons::FLAG,
                expedition.title(),
                format!("Accept | expires day {}", expedition.expires_day),
            )
            .payout(expedition.reward)
        })
        .collect();
    offers.extend(progression.active_expeditions.iter().map(|expedition| {
        Offer::new(
            &menu_icons::OFFLINE,
            format!("Abandon: {}", expedition.title()),
            format!("In progress | due day {}", expedition.expires_day),
        )
        .tag("DROP", ORANGE)
    }));
    let status = vec![SectionItem::stat(
        "Expeditions completed",
        progression.expeditions_completed.to_string(),
        GOLD,
    )];
    if offers.is_empty() {
        UiLayout::screen(draw).modal_with_render_plan(
            "Expedition Board",
            "",
            &ModalContent::new(vec![Section::new(
                "Board",
                GOLD,
                vec![SectionItem::text(
                    "No expeditions posted. Check back tomorrow.",
                )],
            )])
            .prompts(&[hint(Prompt::Back, "close")]),
        );
        return;
    }
    draw_counter(
        ui,
        draw,
        "Expedition Board",
        COUNTER_HELP,
        status,
        "Postings",
        offers,
    );
}

#[allow(
    clippy::too_many_lines,
    reason = "one row list mirroring the depot's menu, index for index"
)]
pub(super) fn draw_depot(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let player = ui.player;
    let progression = ui.snapshot.progression;
    let contract = &progression.contracts.active;
    let (line, done) = contract_line(ui);
    let cargo_value: u32 = player
        .cargo_bay
        .minerals()
        .map(|(mineral, count)| ui.snapshot.market_value(mineral).saturating_mul(count))
        .sum::<u32>()
        + player
            .cargo_bay
            .artifacts()
            .map(|(artifact, count)| artifact.value().saturating_mul(count))
            .sum::<u32>();
    let low_grade: u32 = [MineralKind::Copper, MineralKind::Iron]
        .into_iter()
        .map(|mineral| {
            ui.snapshot
                .market_value(mineral)
                .saturating_mul(player.cargo_bay.mineral(mineral))
        })
        .sum();
    let lab = progression.town_development.scanner_lab_level;
    let offers = vec![
        Offer::new(&menu_icons::CONTRACT, "Complete active contract", line)
            .tag(
                if done { "READY" } else { "OPEN" },
                if done { LIME } else { FOG },
            )
            .about(format!(
                "Pays {} cr when the contract's cargo is in the hold.",
                contract.reward
            )),
        Offer::new(
            &icons::CARGO,
            "Sell loose cargo",
            format!(
                "{}/{} slots in the hold",
                player.cargo_used(),
                player.cargo_capacity
            ),
        )
        .payout(cargo_value)
        .unavailable_if(cargo_value == 0)
        .about("Sells every mineral and artifact on board at today's prices."),
        Offer::new(
            &menu_icons::SORT,
            "Auto-sort low grade",
            "Sell copper and iron only",
        )
        .payout(low_grade)
        .unavailable_if(low_grade == 0)
        .about("Clears cheap ore from the hold and keeps the valuable finds."),
        Offer::new(
            &icons::SCANNER,
            "Sell scan data",
            if lab == 0 {
                "Needs a scanner lab".to_owned()
            } else {
                format!("{} cr per scan marker", 12 + u32::from(lab) * 4)
            },
        )
        .tag("DATA", MINT)
        .unavailable_if(lab == 0)
        .about("Sells your scanner markers to the survey office."),
        Offer::new(
            &menu_icons::RECEIPT,
            "Receipt history",
            format!("{} saved receipts", ui.seat.depot_receipts.len()),
        )
        .about("Your recent sales and payouts."),
        Offer::new(
            icons::block(BlockKind::Stone),
            "Sell block hopper",
            format!(
                "{}/{} blocks aboard",
                player.block_hopper.total(),
                player.block_hopper_capacity
            ),
        )
        .payout(hopper_value(player))
        .unavailable_if(player.block_hopper.is_empty())
        .about(
            "Kade buys plain blocks for fill: 1 cr dirt, 2 cr clay or stone, 4 cr hard rock. \
             Keep them instead to build with.",
        ),
    ];
    let (items, about) = offer_items(offers, ui.seat.selected_menu_item);
    let cargo_items = cargo_items(ui);
    let market_items = market_items(ui);
    let sections = vec![
        Section::new("Counter", GOLD, items),
        Section::new("In the hold", SKY_LIGHT, cargo_items),
        Section::new("Today's prices", LIME, market_items),
    ];
    UiLayout::screen(draw).modal_with_render_plan(
        "Ore Depot",
        &super::quests::quest_tab_subtitle(ui),
        &ModalContent::new(sections)
            .footer(about)
            .prompts(&super::quests::with_quest_tab(ui, COUNTER_HELP)),
    );
}

/// What the depot pays for the whole block hopper.
fn hopper_value(player: &fatherload_simulation::Player) -> u32 {
    player
        .block_hopper
        .blocks()
        .map(|(block, count)| block_sale_value(block).saturating_mul(count))
        .sum()
}

/// The inventory's block hopper section (WP-DROPS): fill, the "collect blocks" toggle and one
/// dump row per kind, index for index with [`hopper_rows`] (what confirm submits), plus the
/// selected row's details.
pub(super) fn hopper_section(ui: &UiFrame<'_, '_>) -> (Section, Option<String>) {
    let player = ui.player;
    let offers = hopper_rows(player)
        .into_iter()
        .filter(|row| !matches!(row, HopperRow::Pack { .. }))
        .map(|row| match row {
            HopperRow::Pack { .. } => unreachable!("pack rows are drawn in the pack section"),
            HopperRow::ToggleCollect => Offer::new(
                &icons::MAGNET,
                "Collect blocks",
                if player.collect_blocks {
                    "The magnet picks up plain blocks"
                } else {
                    "Blocks are left where they fall"
                },
            )
            .tag(
                if player.collect_blocks { "ON" } else { "OFF" },
                if player.collect_blocks { LIME } else { FOG },
            )
            .about(
                "Ore and artifacts are always collected. Turn blocks off when you only want \
                 loot; they still drop for later (or for your crew).",
            ),
            HopperRow::DumpAll => Offer::new(
                &menu_icons::SORT,
                "Dump every block",
                format!("{} blocks out the hatch", player.block_hopper.total()),
            )
            .tag("DUMP", ORANGE)
            .about("Tips the hopper out beside the rig. Your magnet will not take them back."),
            HopperRow::Dump(block) => Offer::new(
                icons::block(block),
                format!("Dump {}", block.name()),
                format!("x{}", player.block_hopper.count(block)),
            )
            .tag("DUMP", ORANGE)
            .about(format!(
                "Tips every {} out beside the rig; the depot pays {} cr each instead.",
                block.name(),
                block_sale_value(block)
            )),
        })
        .collect();
    let (mut items, about) = offer_items(offers, ui.seat.selected_menu_item);
    items.insert(
        0,
        SectionItem::meter(
            "Hopper",
            player.block_hopper.total() as f32,
            player.block_hopper_capacity as f32,
            SAND,
            LOAM,
        ),
    );
    (Section::new("Block Hopper", SAND, items), about)
}

/// The pilot's personal pack (Part II F5): its meter, then a row per stack to move into the
/// rig and per rig stack to pack (the selection continues from the hopper rows).
pub(super) fn pack_section(ui: &UiFrame<'_, '_>) -> (Section, Option<String>) {
    use crate::shell::hopper::pack_rows;
    let player = ui.player;
    let first = hopper_rows(player).len() - pack_rows(player).len();
    let icon = |item: fatherload_simulation::item::ItemKind| match item {
        fatherload_simulation::item::ItemKind::Mineral(mineral) => icons::mineral(mineral),
        fatherload_simulation::item::ItemKind::Block(block) => icons::block(block),
        _ => &icons::CARGO,
    };
    let offers = pack_rows(player)
        .into_iter()
        .filter_map(|row| match row {
            HopperRow::Pack { item, to_rig: true } => Some(
                Offer::new(
                    icon(item),
                    format!("{} to the rig", item.name()),
                    format!("x{} in the pack", player.pilot_pack.count(item)),
                )
                .tag("STOW", LIME)
                .about("Moves the whole stack into the rig's bay or hopper (whatever fits)."),
            ),
            HopperRow::Pack {
                item,
                to_rig: false,
            } => Some(
                Offer::new(
                    icon(item),
                    format!("Pack {}", item.name()),
                    format!("x{} in the rig", player.carried(item)),
                )
                .tag("TAKE", SKY_LIGHT)
                .about(
                    "Takes as many as the pack holds, to carry on foot (blocks to build with, \
                     ore to hand over).",
                ),
            ),
            _ => None,
        })
        .collect();
    let selected = ui
        .seat
        .selected_menu_item
        .checked_sub(first)
        .unwrap_or(usize::MAX);
    let (mut items, about) = offer_items(offers, selected);
    items.insert(
        0,
        SectionItem::meter(
            "Pack",
            player.pilot_pack.total() as f32,
            player.pilot_gear.pack_capacity() as f32,
            SKY_LIGHT,
            SKY,
        ),
    );
    (Section::new("Pilot Pack", SKY_LIGHT, items), about)
}

/// The hold's minerals and artifacts at today's prices.
fn cargo_items(ui: &UiFrame<'_, '_>) -> Vec<SectionItem> {
    let player = ui.player;
    if player.cargo_used() == 0 && !player.cargo_bay.has_artifacts() {
        vec![SectionItem::text("Cargo hold empty.")]
    } else {
        player
            .cargo_bay
            .minerals()
            .map(|(mineral, count)| {
                SectionItem::icon_stat(
                    icons::mineral(mineral),
                    format!("{} x{count}", mineral.name()),
                    format!(
                        "{} cr",
                        ui.snapshot.market_value(mineral).saturating_mul(count)
                    ),
                    GOLD,
                )
            })
            .chain(player.cargo_bay.artifacts().map(|(artifact, count)| {
                SectionItem::stat(
                    format!("{} x{count}", artifact.name()),
                    format!("{} cr", artifact.value().saturating_mul(count)),
                    PINK,
                )
            }))
            .collect()
    }
}

/// Today's price per mineral with its trend arrow.
fn market_items(ui: &UiFrame<'_, '_>) -> Vec<SectionItem> {
    crate::render_snapshot::MINERALS
        .into_iter()
        .map(|mineral| {
            let quote = ui
                .snapshot
                .market
                .iter()
                .find(|quote| quote.mineral == mineral)
                .copied();
            let current = quote.map_or(100, |quote| quote.factor);
            let previous = quote
                .and_then(|quote| quote.previous_factor)
                .unwrap_or(current);
            let trend = match current.cmp(&previous) {
                std::cmp::Ordering::Greater => "\u{2191}",
                std::cmp::Ordering::Less => "\u{2193}",
                std::cmp::Ordering::Equal => "\u{2192}",
            };
            let color = if current >= 120 {
                LIME
            } else if current <= 90 {
                ORANGE
            } else {
                MIST
            };
            SectionItem::icon_stat(
                icons::mineral(mineral),
                mineral.name(),
                format!("{} cr {trend}", ui.snapshot.market_value(mineral)),
                color,
            )
        })
        .collect()
}

pub(super) fn draw_receipts(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let sections = if ui.seat.depot_receipts.is_empty() {
        vec![Section::new(
            "Receipts",
            GOLD,
            vec![SectionItem::text("No receipts yet. Sell some cargo first.")],
        )]
    } else {
        ui.seat
            .depot_receipts
            .iter()
            .rev()
            .take(10)
            .enumerate()
            .map(|(index, receipt)| {
                let items = receipt
                    .lines()
                    .take(5)
                    .map(|line| SectionItem::text(line.to_owned()))
                    .collect();
                Section::new(
                    if index == 0 {
                        "Latest receipt".to_owned()
                    } else {
                        format!("Receipt {}", index + 1)
                    },
                    if index == 0 { GOLD } else { FOG },
                    items,
                )
            })
            .collect()
    };
    UiLayout::screen(draw).modal_with_render_plan(
        "Depot Receipts",
        "Recent sales and payouts.",
        &ModalContent::new(sections)
            .prompts(&[hint(Prompt::Confirm, ""), hint(Prompt::Back, "return")]),
    );
}

#[cfg(test)]
mod tests {
    use fatherload_simulation::{
        economy::{REFUEL_UNIT_COST, REPAIR_UNIT_COST, refuel_amount, repair_amount},
        player::Player,
    };

    use super::service_charge;

    /// The fuel and repair quotes must equal what the simulation's full service bills, which is
    /// what every small/half/full choice resolves to today.
    #[test]
    fn service_quotes_match_the_simulated_charge() {
        for credits in [0, 3, 12, 45, 4850] {
            let mut player = Player::new(0.0, 0.0);
            player.fuel = player.fuel_capacity - 45.0;
            player.hull = player.max_hull() - 30.0;
            player.credits = credits;
            let (_, fuel_quote, _, _) = service_charge(
                player.fuel_capacity - player.fuel,
                REFUEL_UNIT_COST,
                credits,
            );
            assert_eq!(
                fuel_quote,
                refuel_amount(&mut player, 1.0),
                "fuel, credits {credits}"
            );

            player.credits = credits;
            let (_, repair_quote, _, _) =
                service_charge(player.max_hull() - player.hull, REPAIR_UNIT_COST, credits);
            assert_eq!(
                repair_quote,
                repair_amount(&mut player, 1.0),
                "repair, credits {credits}"
            );
        }
    }
}
