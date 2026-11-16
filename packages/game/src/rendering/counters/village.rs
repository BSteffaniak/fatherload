//! Village counters (a building's deals and a word with the keeper).

use fatherload_simulation::{
    Player,
    economy::{REFUEL_UNIT_COST, REPAIR_UNIT_COST},
    village::{BuildingKind, Village, VillageDeal, VillageOffer, VillageRelation},
    worldgen::WorldSettings,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::super::pixel_art::palette::*;
use super::{
    super::{
        layout::{ModalContent, Section, SectionItem, UiLayout},
        pixel_art::sprites::{explosives as explosive_art, icons, menu_icons},
        screen::UiFrame,
    },
    COUNTER_HELP, Offer, credits_item, offer_items,
};
use crate::{render_snapshot::RenderSnapshot, shell};

/// Stars for a prosperity of 1..=5.
fn prosperity(found: &Village) -> String {
    let filled = usize::from(found.prosperity.min(5));
    format!("{}{}", "*".repeat(filled), "-".repeat(5 - filled))
}

/// What a village and its counter know about the rig, for pricing rows.
#[allow(clippy::too_many_lines, reason = "one arm per kind of deal")]
fn quote(snapshot: &RenderSnapshot<'_>, player: &Player, offer: VillageOffer) -> Offer {
    let credits = player.credits;
    match offer.deal {
        VillageDeal::SellCargo { percent, only } => {
            let minerals: u32 = player
                .cargo_bay
                .minerals()
                .filter(|(mineral, _)| only.is_none_or(|wanted| wanted == *mineral))
                .map(|(mineral, count)| {
                    let value = snapshot
                        .market
                        .iter()
                        .find(|quote| quote.mineral == mineral)
                        .map_or_else(|| mineral.value(), |quote| quote.value);
                    value * count
                })
                .sum();
            let artifacts: u32 = if only.is_some() {
                0
            } else {
                player
                    .cargo_bay
                    .artifacts()
                    .map(|(artifact, count)| artifact.value() * count)
                    .sum()
            };
            let payout = (minerals + artifacts) * percent / 100;
            let icon = only.map_or(&icons::CARGO, icons::mineral);
            let label = only.map_or_else(
                || "Sell cargo".to_owned(),
                |mineral| format!("Sell {}", mineral.name()),
            );
            Offer::new(icon, label, format!("{percent}% of the depot price"))
                .payout(payout)
                .unavailable_if(payout == 0)
                .about(if only.is_some() {
                    "The assayer buys only this, and pays far over the town depot for it."
                } else {
                    "A trading post buys ore and relics at a little under the depot's price."
                })
        }
        VillageDeal::Refuel { percent } => {
            // Up in the sky, rigs that cannot cross the dead-air gap only top up partway
            // (the same rule the tick applies).
            let altitude = (-player.y / fatherload_simulation::TILE_SIZE).floor() as i32;
            let level = fatherload_simulation::milestone::sky_refuel_level(player, altitude);
            let capped = level < player.fuel_capacity;
            let missing = (level - player.fuel).ceil().max(0.0) as u32;
            let cost = (missing * REFUEL_UNIT_COST * percent).div_ceil(100);
            Offer::new(
                &icons::FUEL,
                if capped { "Top up the tank" } else { "Fill the tank" },
                format!("{missing} fuel at {percent}% of town price"),
            )
            .price(cost.min(credits.max(1)), credits)
            .unavailable_if(missing == 0)
            .about(if capped {
                "Fuel is rationed this high up: enough to hop the isles, never enough to brave the dead air."
            } else if percent < 100 {
                "Cheap fuel out here: the depot's whole reason for being."
            } else {
                "Fuel hauled all the way out here costs a little more than in town."
            })
        }
        VillageDeal::Repair { percent } => {
            let missing = (player.max_hull() - player.hull).ceil().max(0.0) as u32;
            let cost = (missing * REPAIR_UNIT_COST * percent).div_ceil(100);
            Offer::new(
                &icons::HULL,
                "Mend the hull",
                format!("{missing} hull at {percent}% of town price"),
            )
            .price(cost.min(credits.max(1)), credits)
            .unavailable_if(missing == 0)
            .about("Patched with whatever's in the scrap pile, but it holds.")
        }
        VillageDeal::Part(part) => {
            let owned = snapshot.progression.rig_part_inventory.contains(&part);
            let row = Offer::new(&menu_icons::GEAR, part.title(), "rare rig part")
                .about("A part the town shop never stocks. It goes in the crew's parts locker.");
            if owned {
                row.tag("OWNED", LIME).unavailable_if(true)
            } else {
                row.price(offer.price, credits)
            }
        }
        VillageDeal::Cosmetic(item) => {
            let owned = player.cosmetic_unlocks.has(item);
            let row = Offer::new(&menu_icons::CROWN, item.label(), "garage unlock")
                .about("Unlocks this look at the garage paint bay in town.");
            if owned {
                row.tag("OWNED", LIME).unavailable_if(true)
            } else {
                row.price(offer.price, credits)
            }
        }
        VillageDeal::Explosives { kind, count } => {
            Offer::new(explosive_art::icon(kind), format!("{count} x {}", kind.name()), "smith's pack")
                .about("Bigger packs than the town shack, whatever its stock level.")
                .price(offer.price, credits)
        }
        VillageDeal::ChartRegion => Offer::new(&menu_icons::GLOBE, "Chart the region", "nearby sites and villages")
            .about("The cartographer inks the nearest unknown sites and neighbouring villages onto your map.")
            .price(offer.price, credits),
        VillageDeal::Blessing(kind) => Offer::new(&menu_icons::FLAG, kind.name(), kind.effect())
            .about("A shrine blessing lasts ten minutes of play.")
            .price(offer.price, credits),
        VillageDeal::Rumour => Offer::new(&menu_icons::LETTER, "Buy a round", "ask for rumours")
            .about("The regulars always know of something buried nearby.")
            .price(offer.price, credits),
        VillageDeal::SkyCraft { recipe, tier } => {
            let cost = fatherload_simulation::skycraft::cost(recipe, tier);
            let have = recipe.level(&player.hazard_gear);
            let title = format!(
                "{} {}",
                recipe.name(),
                fatherload_simulation::village::services::tier_numeral(tier)
            );
            let ore = format!(
                "{} skystone, {} cloud silver",
                cost.skystone, cost.cloud_silver
            );
            let row = Offer::new(&menu_icons::GEAR, title, ore.clone()).about(format!(
                "Sky crafting: {}. Paid in credits and sky ore from the bay ({ore}).",
                recipe.effect()
            ));
            if tier <= have {
                row.tag("FITTED", LIME).unavailable_if(true)
            } else if tier > have + 1 {
                row.tag("LATER", LAVENDER).unavailable_if(true)
            } else {
                let short_of_ore = player
                    .cargo_bay
                    .mineral(fatherload_simulation::terrain::MineralKind::Skystone)
                    < cost.skystone
                    || player
                        .cargo_bay
                        .mineral(fatherload_simulation::terrain::MineralKind::CloudSilver)
                        < cost.cloud_silver;
                let row = row.price(offer.price, credits);
                if short_of_ore {
                    row.tag("NEED ORE", ORANGE).unavailable_if(true)
                } else {
                    row
                }
            }
        }
        VillageDeal::Room => {
            let row = Offer::new(&icons::MOON, "A room for the night", "sleep until dawn")
                .about("A warm bed upstairs. In a crew, everyone has to turn in before morning comes.");
            if snapshot.clock.phase().sleepable() {
                row.price(offer.price, credits)
            } else {
                row.tag("DUSK", LAVENDER).unavailable_if(true)
            }
        }
    }
}

/// Draws the counter of the village building seat `ui.seat` is inside.
pub(in crate::rendering) fn draw_village_counter(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
) {
    let settings = ui.snapshot.terrain.settings();
    let Some((found, building)) = ui
        .seat
        .village
        .interior
        .and_then(|interior| shell::interior_village(settings, interior))
    else {
        return;
    };
    let lot = &found.buildings[building];
    let mut offers: Vec<Offer> = found
        .offers(building)
        .into_iter()
        .map(|offer| quote(ui.snapshot, ui.player, offer))
        .collect();
    if let Some(keeper) = lot
        .keeper
        .and_then(|index| found.population.characters.get(usize::from(index)))
    {
        offers.push(
            Offer::new(
                &menu_icons::PERSON,
                format!("Talk to {}", keeper.name.short()),
                keeper.role.name(),
            )
            .about("Pass the time of day. Folk remember who stops to chat."),
        );
    }
    let mut status = vec![credits_item(ui.player.credits)];
    if lot.kind == BuildingKind::Hall {
        status.extend(village_news(settings, &found));
    } else {
        status.push(SectionItem::stat("Village", found.name.clone(), GOLD));
    }
    let (items, about) = offer_items(offers, ui.seat.selected_menu_item);
    let sections = vec![
        Section::new("Status", SKY_LIGHT, status),
        Section::new(
            if lot.kind == BuildingKind::Hall {
                "The elder"
            } else {
                "At the counter"
            },
            GOLD,
            items,
        ),
    ];
    UiLayout::screen(draw).modal_with_render_plan(
        &shell::building_title(&found, building),
        found.describe().as_str(),
        &ModalContent::new(sections)
            .footer(about)
            .prompts(COUNTER_HELP),
    );
}

/// The meeting hall's notice board: prosperity, the village's trouble and its neighbours.
fn village_news(settings: &WorldSettings, found: &Village) -> Vec<SectionItem> {
    let mut items = vec![
        SectionItem::stat("Prosperity", prosperity(found), GOLD),
        SectionItem::stat("Folk", found.population.characters.len().to_string(), BONE),
    ];
    let trouble = found
        .problem
        .map_or("none to speak of", |problem| match problem {
            fatherload_simulation::people::VillageProblem::SealedMine => "the old mine caved in",
            fatherload_simulation::people::VillageProblem::MissingPerson => "someone went missing",
            fatherload_simulation::people::VillageProblem::CaveSwarm => {
                "things in the caves at night"
            }
            fatherload_simulation::people::VillageProblem::DryWell => "the well ran dry",
            fatherload_simulation::people::VillageProblem::Feud => "two families feuding",
        });
    items.push(SectionItem::stat("Trouble", trouble, ORANGE));
    for (neighbour, relation) in &found.relations {
        let Some((name, _)) =
            fatherload_simulation::village::village_identity(settings, *neighbour)
        else {
            continue;
        };
        let word = match relation {
            VillageRelation::Friendly => "friends",
            VillageRelation::Trading => "trading partners",
            VillageRelation::Rival => "old rivals",
        };
        items.push(SectionItem::stat(name, word, SKY_LIGHT));
    }
    items
}
