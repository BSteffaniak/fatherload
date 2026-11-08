//! Building counter panels: what each building sells or offers, drawn as item rows with icons,
//! prices and affordability, plus a details footer for the selected row and the confirmation
//! dialogs that act as receipts before a purchase.
//!
//! Every row list mirrors, index for index, the request the shell submits for that menu item
//! (see `ShellState::handle_modal`), so the labels always describe what Enter actually does.

use drillgame_simulation::{
    RecipeKind,
    economy::{
        DeepClaimStatus, REFUEL_UNIT_COST, REPAIR_UNIT_COST, TownBuilding, UpgradeKind,
        upgrade_offers, upgrade_tier_name,
    },
    terrain::MineralKind,
    world::{BOMB_BUNDLES, MINING_ROCKETS_COST, MINING_ROCKETS_COUNT, MINING_ROCKETS_SHACK_LEVEL},
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
        sprites::{icons, menu_icons, rig},
    },
    screen::UiFrame,
};

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
    help: &str,
    status: Vec<SectionItem>,
    heading: &str,
    offers: Vec<Offer>,
) {
    let (items, about) = offer_items(offers, ui.shell.selected_menu_item);
    let mut sections = Vec::new();
    if !status.is_empty() {
        sections.push(Section::new("Status", SKY_LIGHT, status));
    }
    sections.push(Section::new(heading, GOLD, items));
    UiLayout::screen(draw).modal_with_render_plan(
        title,
        help,
        &ModalContent::new(sections).footer(about),
    );
}

const COUNTER_HELP: &str = "Up/Down select | Enter/E confirm | Esc close";

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
    let offers = service_offers(
        &menu_icons::HAMMER,
        ["Patch job", "Half repair", "Full overhaul"],
        "hull",
        hud.max_hull - hud.hull,
        REPAIR_UNIT_COST,
        hud.credits,
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
const fn upgrade_icon(kind: UpgradeKind) -> &'static SpriteDef {
    match kind {
        UpgradeKind::Drill => &rig::DRILL_DOWN,
        UpgradeKind::FuelTank => &icons::FUEL,
        UpgradeKind::CargoBay => &icons::CARGO,
        UpgradeKind::Engine => &menu_icons::GEAR,
        UpgradeKind::Hull => &icons::HULL,
        UpgradeKind::Radiator => &icons::HEAT,
        UpgradeKind::Scanner => &icons::SCANNER,
        UpgradeKind::BombPack => &icons::BOMB,
    }
}

fn shop_offers(ui: &UiFrame<'_, '_>) -> Vec<Offer> {
    let credits = ui.player.credits;
    upgrade_offers(ui.player)
        .iter()
        .map(|offer| {
            Offer::new(
                upgrade_icon(offer.kind),
                offer.name,
                format!(
                    "Tier {} {}",
                    offer.level + 1,
                    upgrade_tier_name(offer.kind, offer.level)
                ),
            )
            .price(offer.cost, credits)
            .about(offer.description)
        })
        .collect()
}

pub(super) fn draw_shop(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let status = vec![credits_item(ui.player.credits)];
    draw_counter(
        ui,
        draw,
        "Upgrade Shop",
        "Up/Down select | Enter/E buy | Esc close",
        status,
        "Upgrades",
        shop_offers(ui),
    );
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
        "Enter/E confirms | Esc goes back",
        &ModalContent::new(vec![
            Section::new("Order", GOLD, items),
            Section::new("Receipt", SKY_LIGHT, ledger),
        ]),
    );
}

pub(super) fn draw_shop_confirm(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let offer = shop_offers(ui).into_iter().nth(ui.shell.selected_menu_item);
    draw_purchase_confirm(draw, "Confirm Upgrade", offer, ui.player.credits);
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
    .nth(ui.shell.selected_menu_item);
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
    .nth(ui.shell.selected_menu_item);
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
    let bundle = |label: &str, count: u32, cost: u32| {
        Offer::new(&icons::BOMB, label, format!("+{} bombs", count + bonus))
            .price(cost, credits)
            .about(if bonus > 0 {
                format!("{count} bombs plus {bonus} extra from the shack's upgrades. Press B underground to drop one.")
            } else {
                format!("{count} bombs. Press B underground to drop one.")
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
    let status = vec![
        credits_item(credits),
        SectionItem::icon_stat(
            &icons::BOMB,
            "Bombs carried",
            ui.player.bombs.to_string(),
            ORANGE,
        ),
    ];
    draw_counter(
        ui,
        draw,
        "Explosives Shack",
        COUNTER_HELP,
        status,
        "For sale",
        vec![
            bundle("Small bomb pack", small_count, small_cost),
            bundle("Standard bomb pack", big_count, big_cost),
            rockets,
            test,
        ],
    );
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
    let (items, about) = offer_items(offers, ui.shell.selected_menu_item);
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
        COUNTER_HELP,
        &ModalContent::new(sections).footer(about),
    );
}

/// Short material names for recipe costs.
fn material_list(cost: &[(drillgame_simulation::terrain::StrategicResourceKind, u32)]) -> String {
    cost.iter()
        .map(|(material, count)| format!("{count} {}", material.name()))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn draw_crafting(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let materials = &ui.player.materials;
    let offers = RecipeKind::ALL
        .into_iter()
        .map(|recipe| {
            let can_craft = recipe
                .cost()
                .iter()
                .all(|(material, needed)| materials.get(material).copied().unwrap_or(0) >= *needed);
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
    let status = if materials.is_empty() {
        vec![SectionItem::text(
            "No strategic materials yet. Process ore or find relics deep down.",
        )]
    } else {
        materials
            .iter()
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
        .materials
        .get(&drillgame_simulation::terrain::StrategicResourceKind::AncientAlloy)
        .copied()
        .unwrap_or(0);
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
            "Esc closes",
            &ModalContent::new(vec![Section::new(
                "Board",
                GOLD,
                vec![SectionItem::text(
                    "No expeditions posted. Check back tomorrow.",
                )],
            )]),
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

pub(super) fn draw_depot(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let player = ui.player;
    let progression = ui.snapshot.progression;
    let contract = &progression.contracts.active;
    let (line, done) = contract_line(ui);
    let cargo_value: u32 = player
        .cargo
        .iter()
        .map(|(mineral, count)| ui.snapshot.market_value(*mineral).saturating_mul(*count))
        .sum::<u32>()
        + player
            .artifacts
            .iter()
            .map(|(artifact, count)| artifact.value().saturating_mul(*count))
            .sum::<u32>();
    let low_grade: u32 = [MineralKind::Copper, MineralKind::Iron]
        .into_iter()
        .map(|mineral| {
            ui.snapshot
                .market_value(mineral)
                .saturating_mul(player.cargo.get(&mineral).copied().unwrap_or(0))
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
            format!("{} saved receipts", ui.shell.depot_receipts.len()),
        )
        .about("Your recent sales and payouts."),
    ];
    let (items, about) = offer_items(offers, ui.shell.selected_menu_item);
    let cargo_items = cargo_items(ui);
    let market_items = market_items(ui);
    let sections = vec![
        Section::new("Counter", GOLD, items),
        Section::new("In the hold", SKY_LIGHT, cargo_items),
        Section::new("Today's prices", LIME, market_items),
    ];
    UiLayout::screen(draw).modal_with_render_plan(
        "Ore Depot",
        COUNTER_HELP,
        &ModalContent::new(sections).footer(about),
    );
}

/// The hold's minerals and artifacts at today's prices.
fn cargo_items(ui: &UiFrame<'_, '_>) -> Vec<SectionItem> {
    let player = ui.player;
    if player.cargo_used() == 0 && player.artifacts.is_empty() {
        vec![SectionItem::text("Cargo hold empty.")]
    } else {
        player
            .cargo
            .iter()
            .map(|(mineral, count)| {
                SectionItem::icon_stat(
                    icons::mineral(*mineral),
                    format!("{} x{count}", mineral.name()),
                    format!(
                        "{} cr",
                        ui.snapshot.market_value(*mineral).saturating_mul(*count)
                    ),
                    GOLD,
                )
            })
            .chain(player.artifacts.iter().map(|(artifact, count)| {
                SectionItem::stat(
                    format!("{} x{count}", artifact.name()),
                    format!("{} cr", artifact.value().saturating_mul(*count)),
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
    let sections = if ui.shell.depot_receipts.is_empty() {
        vec![Section::new(
            "Receipts",
            GOLD,
            vec![SectionItem::text("No receipts yet. Sell some cargo first.")],
        )]
    } else {
        ui.shell
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
        "Recent sales and payouts | Enter/E or Esc returns",
        &ModalContent::new(sections),
    );
}

#[cfg(test)]
mod tests {
    use drillgame_simulation::{
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
