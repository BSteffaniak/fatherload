//! Each person's unique offer (WP-PEOPLE "unique offers"), apprenticeship perks and gift tastes.
//!
//! [`offers_of`] rolls a short list of [`PersonOffer`]s from a character's role offer
//! ([`super::OfferKind`]) and voice seed: a smith's one rare part and a patch-up, a baker's trail
//! pie (a timed blessing), a trader's cosmetics, a prospector's map secrets and rumours, a
//! retired miner's apprenticeship (a permanent [`Perk`]), an innkeeper's gossip. Some need the
//! person to like you first ([`PersonOffer::needs`]); prices fall as they warm to you and rise
//! when they hold a grudge ([`price_for`]). Gifts land well or badly depending on the person
//! ([`gift_reaction`]).
//!
//! Offers are pure functions of the character, so host and clients list the same rows; the
//! purchase is validated and applied inside the tick (`world/people.rs`).

use serde::{Deserialize, Serialize};

use super::{
    Character, OfferKind, PeopleRng, Role, Standing,
    traits::{Value, Want},
};
use crate::{
    cosmetics::CosmeticItem,
    explosive::{EXPLOSIVE_OFFERS, ExplosiveKind},
    item::ItemKind,
    rig::RigPartKind,
    terrain::MineralKind,
    village::{BlessingKind, VillageDeal, services::part_price},
    worldgen::deposits::MINERALS,
};

const SALT_OFFERS: u64 = 0x5045_4F50_4C45_0020;
const SALT_TASTE: u64 = 0x5045_4F50_4C45_0021;

/// A small permanent perk learned through an apprenticeship.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Perk {
    /// Village counters and people charge [`HAGGLER_PERCENT`] of their price.
    Haggler,
    /// Rumours and map secrets reveal one more site each.
    Surveyor,
    /// Goodwill from talking, trading and gifts counts half again.
    Charmer,
}

/// Percent of a price a haggler pays.
pub const HAGGLER_PERCENT: u32 = 90;

impl Perk {
    pub const ALL: [Self; 3] = [Self::Haggler, Self::Surveyor, Self::Charmer];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Haggler => "Haggler",
            Self::Surveyor => "Surveyor",
            Self::Charmer => "Silver Tongue",
        }
    }

    #[must_use]
    pub const fn effect(self) -> &'static str {
        match self {
            Self::Haggler => "10% off at village counters and with traders",
            Self::Surveyor => "rumours and map secrets reveal an extra site",
            Self::Charmer => "people warm to you half again as fast",
        }
    }
}

/// What buying an offer does.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum PersonDeal {
    /// A village-style deal: a rare part, a cosmetic, a pack of charges, a tonic (a timed
    /// blessing) or a metered patch-up, resolved like the village counters resolve it.
    Goods(VillageDeal),
    /// A map secret: the nearest unknown sites near their home, identified on your map.
    MapSecret,
    /// Gossip: the nearest unknown site near their home, as an echo on your map.
    Rumour,
    /// A permanent perk.
    Apprenticeship(Perk),
    /// A piece of furniture in the style of their people (Wave 6: every culture's signature
    /// piece, [`signature_furniture`]), delivered to the buyer's furniture stock.
    Furniture(crate::home::Furniture),
}

/// One row of a person's trade list.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct PersonOffer {
    /// The kind of thing it is (the character's role offer, or a side line).
    pub kind: OfferKind,
    pub deal: PersonDeal,
    /// List price in credits (0 for metered deals); see [`price_for`].
    pub price: u32,
    /// How well they must know you to sell it.
    pub needs: Standing,
    /// One of a kind: each player can buy it from this person once.
    pub once: bool,
}

impl PersonOffer {
    /// The row title ("Titan Drill (one of a kind)").
    #[must_use]
    pub fn title(&self) -> String {
        match self.deal {
            PersonDeal::Goods(VillageDeal::Part(part)) => {
                format!("{} (one of a kind)", part.title())
            }
            PersonDeal::Goods(VillageDeal::Cosmetic(item)) => {
                format!("Garage unlock: {}", item.label())
            }
            PersonDeal::Goods(VillageDeal::Explosives { kind, count }) => {
                format!("{count} x {}", kind.name())
            }
            PersonDeal::Goods(VillageDeal::Blessing(kind)) => tonic_name(kind).to_owned(),
            PersonDeal::Goods(VillageDeal::Repair { percent }) => {
                format!("Patch the hull ({percent}% of town price)")
            }
            PersonDeal::Goods(VillageDeal::Refuel { percent }) => {
                format!("Top up the tank ({percent}% of town price)")
            }
            PersonDeal::Goods(other) => format!("{other:?}"),
            PersonDeal::MapSecret => "A map secret".to_owned(),
            PersonDeal::Rumour => "Local gossip".to_owned(),
            PersonDeal::Apprenticeship(perk) => format!("Apprenticeship: {}", perk.name()),
            PersonDeal::Furniture(piece) => format!("{} (handmade)", piece.name()),
        }
    }

    /// A one-line description for the trade panel.
    #[must_use]
    pub fn describe(&self) -> String {
        match self.deal {
            PersonDeal::Goods(VillageDeal::Part(_)) => {
                "A rig part they built themselves. It goes in the crew's parts locker.".to_owned()
            }
            PersonDeal::Goods(VillageDeal::Cosmetic(_)) => {
                "Unlocks this look at the garage paint bay.".to_owned()
            }
            PersonDeal::Goods(VillageDeal::Explosives { .. }) => {
                "No questions asked about where they came from.".to_owned()
            }
            PersonDeal::Goods(VillageDeal::Blessing(kind)) => {
                format!("{} for ten minutes of play.", capitalize(kind.effect()))
            }
            PersonDeal::Goods(VillageDeal::Repair { .. }) => {
                "Hammered back into shape on the spot.".to_owned()
            }
            PersonDeal::Goods(_) => String::new(),
            PersonDeal::MapSecret => {
                "Marks the nearest sites you don't know of on your map, identified.".to_owned()
            }
            PersonDeal::Rumour => {
                "Marks the nearest unknown site on your map as an echo.".to_owned()
            }
            PersonDeal::Apprenticeship(perk) => format!("Permanent: {}.", perk.effect()),
            PersonDeal::Furniture(_) => {
                "Made the way their people make it. Delivered to your furniture stock.".to_owned()
            }
        }
    }
}

/// The furniture piece people of `culture` are known for (Wave 6): makers and traders sell it
/// ([`sells_signature`]), nobody else does.
#[must_use]
pub const fn signature_furniture(culture: super::Culture) -> crate::home::Furniture {
    use crate::home::Furniture as F;
    match culture {
        super::Culture::Frontier => F::RockingChair,
        super::Culture::Highland => F::Loom,
        super::Culture::Mesa => F::ClayOven,
        super::Culture::Riverside => F::FishTank,
        super::Culture::Scrapper => F::JunkSculpture,
        super::Culture::Deepfolk => F::GlowcapLamp,
        super::Culture::Aeronaut => F::ModelAirship,
        super::Culture::Cloudborn => F::WindChime,
        super::Culture::Lunar => F::MoonGlobe,
    }
}

/// Whether someone in `role` makes or trades their culture's signature piece.
#[must_use]
pub const fn sells_signature(role: Role) -> bool {
    matches!(
        role,
        Role::Smith
            | Role::Trader
            | Role::Tinkerer
            | Role::Mechanic
            | Role::Farmer
            | Role::Apprentice
            | Role::Aeronaut
            | Role::Monk
            | Role::Smuggler
            | Role::Quartermaster
            | Role::Hydroponist
    )
}

/// Percent of the land office's catalogue price a person asks for their culture's piece.
pub const SIGNATURE_PRICE_PERCENT: u32 = 110;

/// The name of the tonic a blessing comes as from a person.
#[must_use]
pub const fn tonic_name(kind: BlessingKind) -> &'static str {
    match kind {
        BlessingKind::Ward => "Herbal poultice",
        BlessingKind::Thrift => "Trail pie",
        BlessingKind::Fortune => "Lucky stout",
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The trade list of `character` (best first). Never empty.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one arm per kind of offer")]
pub fn offers_of(character: &Character) -> Vec<PersonOffer> {
    let mut rng = PeopleRng::new(character.voice_seed, SALT_OFFERS, 0, 0);
    let row = |kind, deal, price, needs, once| PersonOffer {
        kind,
        deal,
        price,
        needs,
        once,
    };
    let mut list = Vec::new();
    match character.offer {
        OfferKind::PartVariant => {
            let part = *rng.pick(&RigPartKind::ALL);
            list.push(row(
                OfferKind::PartVariant,
                PersonDeal::Goods(VillageDeal::Part(part)),
                part_price(part) * 85 / 100,
                Standing::Acquainted,
                true,
            ));
            list.push(row(
                OfferKind::PartVariant,
                PersonDeal::Goods(VillageDeal::Repair { percent: 85 }),
                0,
                Standing::Stranger,
                false,
            ));
        }
        OfferKind::Recipe => {
            let kind = match character.role {
                Role::Baker => BlessingKind::Thrift,
                Role::Brewer => BlessingKind::Fortune,
                Role::Healer => BlessingKind::Ward,
                _ => *rng.pick(&BlessingKind::ALL),
            };
            list.push(row(
                OfferKind::Recipe,
                PersonDeal::Goods(VillageDeal::Blessing(kind)),
                75 + u32::try_from(rng.below(4)).unwrap_or(0) * 10,
                Standing::Stranger,
                false,
            ));
            list.push(row(
                OfferKind::Rumours,
                PersonDeal::Rumour,
                20,
                Standing::Acquainted,
                false,
            ));
        }
        OfferKind::Cosmetic => {
            let mut items: Vec<CosmeticItem> = CosmeticItem::all()
                .into_iter()
                .filter(|item| !item.is_starter())
                .collect();
            for needs in [Standing::Acquainted, Standing::Friendly] {
                if items.is_empty() {
                    break;
                }
                let item = items.remove(rng.below(items.len()));
                let price = 520 + u32::try_from(rng.below(5)).unwrap_or(0) * 120;
                list.push(row(
                    OfferKind::Cosmetic,
                    PersonDeal::Goods(VillageDeal::Cosmetic(item)),
                    if needs == Standing::Friendly {
                        price + 300
                    } else {
                        price
                    },
                    needs,
                    true,
                ));
            }
        }
        OfferKind::MapSecret => {
            list.push(row(
                OfferKind::MapSecret,
                PersonDeal::MapSecret,
                160 + u32::try_from(rng.below(5)).unwrap_or(0) * 20,
                Standing::Acquainted,
                false,
            ));
            list.push(row(
                OfferKind::Rumours,
                PersonDeal::Rumour,
                25,
                Standing::Stranger,
                false,
            ));
        }
        OfferKind::Apprenticeship => {
            let perk = match character.role {
                Role::RetiredMiner => Perk::Surveyor,
                Role::Warden => Perk::Haggler,
                Role::ShrineKeeper => Perk::Charmer,
                _ => *rng.pick(&Perk::ALL),
            };
            list.push(row(
                OfferKind::Apprenticeship,
                PersonDeal::Apprenticeship(perk),
                1_100 + u32::try_from(rng.below(4)).unwrap_or(0) * 100,
                Standing::Friendly,
                true,
            ));
            list.push(row(
                OfferKind::Rumours,
                PersonDeal::Rumour,
                30,
                Standing::Acquainted,
                false,
            ));
        }
        OfferKind::Rumours => {
            list.push(row(
                OfferKind::Rumours,
                PersonDeal::Rumour,
                15,
                Standing::Stranger,
                false,
            ));
            if matches!(character.role, Role::Courier | Role::Apprentice) {
                // Specialist goods: a pack of charges off the back of a wagon.
                let picks: Vec<_> = EXPLOSIVE_OFFERS
                    .iter()
                    .filter(|offer| offer.kind != ExplosiveKind::Flare)
                    .collect();
                let offer = rng.pick(&picks);
                list.push(row(
                    OfferKind::Rumours,
                    PersonDeal::Goods(VillageDeal::Explosives {
                        kind: offer.kind,
                        count: offer.count,
                    }),
                    offer.cost * 110 / 100,
                    Standing::Acquainted,
                    false,
                ));
            } else {
                list.push(row(
                    OfferKind::MapSecret,
                    PersonDeal::MapSecret,
                    200,
                    Standing::Friendly,
                    false,
                ));
            }
        }
    }
    // Wave 6: makers and traders sell their culture's signature piece (appended, so the rows
    // before it keep their indices), and the colony's hydroponists their grow racks.
    if sells_signature(character.role) {
        let piece = signature_furniture(character.culture);
        list.push(row(
            OfferKind::Recipe,
            PersonDeal::Furniture(piece),
            piece.catalogue_price() * SIGNATURE_PRICE_PERCENT / 100,
            Standing::Acquainted,
            false,
        ));
    }
    if character.role == Role::Hydroponist {
        let rack = crate::home::Furniture::HydroponicRack;
        list.push(row(
            OfferKind::Recipe,
            PersonDeal::Furniture(rack),
            rack.catalogue_price() * SIGNATURE_PRICE_PERCENT / 100,
            Standing::Friendly,
            false,
        ));
    }
    list
}

/// What `offer` costs someone they regard with `standing`, with or without the [`Perk::Haggler`]
/// perk: grudges add a quarter, friends get a tenth off, close friends a fifth.
#[must_use]
pub const fn price_for(offer: &PersonOffer, standing: Standing, haggler: bool) -> u32 {
    let percent = match standing {
        Standing::Cold => 125,
        Standing::Stranger | Standing::Acquainted => 100,
        Standing::Friendly => 90,
        Standing::Close => 80,
    };
    let percent = if haggler {
        percent * HAGGLER_PERCENT / 100
    } else {
        percent
    };
    (offer.price * percent).div_ceil(100)
}

/// A standing's rank, coldest first.
const fn rank(standing: Standing) -> u8 {
    match standing {
        Standing::Cold => 0,
        Standing::Stranger => 1,
        Standing::Acquainted => 2,
        Standing::Friendly => 3,
        Standing::Close => 4,
    }
}

/// Whether someone with `standing` may buy an offer that needs `needs`. A grudge closes every
/// door but the cheapest.
#[must_use]
pub const fn standing_allows(standing: Standing, needs: Standing) -> bool {
    if matches!(standing, Standing::Cold) {
        return matches!(needs, Standing::Stranger);
    }
    rank(standing) >= rank(needs)
}

/// How a gift lands.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum GiftReaction {
    /// Their favourite: a big boost.
    Loved,
    Liked,
    /// "Oh. A rock. Thank you?"
    Indifferent,
}

/// The mineral `character` loves to be given.
#[must_use]
pub fn favourite_mineral(character: &Character) -> MineralKind {
    let mut rng = PeopleRng::new(character.voice_seed, SALT_TASTE, 0, 0);
    // Folk who prize wealth fancy the shiny ones; everyone else anything goes.
    if character.values.contains(&Value::Fortune) || character.want == Want::StrikeItRich {
        *rng.pick(&MINERALS[5..])
    } else {
        *rng.pick(&MINERALS)
    }
}

/// How `character` takes a gift of one `item`.
#[must_use]
pub fn gift_reaction(character: &Character, item: ItemKind) -> GiftReaction {
    match item {
        ItemKind::Mineral(mineral) if mineral == favourite_mineral(character) => {
            GiftReaction::Loved
        }
        ItemKind::Artifact(_)
            if matches!(
                character.role,
                Role::Scholar | Role::Storyteller | Role::ShrineKeeper | Role::Hermit
            ) || character.values.contains(&Value::Knowledge) =>
        {
            GiftReaction::Loved
        }
        ItemKind::Artifact(_) => GiftReaction::Liked,
        ItemKind::Mineral(mineral) if mineral.value() >= 40 => GiftReaction::Liked,
        ItemKind::Material(_)
            if matches!(
                character.role,
                Role::Smith | Role::Mechanic | Role::Tinkerer
            ) =>
        {
            GiftReaction::Liked
        }
        _ => GiftReaction::Indifferent,
    }
}

/// Whether `item` can be handed over as a gift (ore, relics and strategic materials).
#[must_use]
pub const fn giftable(item: ItemKind) -> bool {
    matches!(
        item,
        ItemKind::Mineral(_) | ItemKind::Artifact(_) | ItemKind::Material(_)
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{GiftReaction, gift_reaction, offers_of, price_for, standing_allows};
    use crate::item::ItemKind;
    use crate::people::{CharacterId, OfferKind, Standing, generate_character, names::Culture};

    #[test]
    fn every_person_offers_something_and_offers_vary() {
        let mut kinds = BTreeSet::new();
        let mut titles = BTreeSet::new();
        for index in 0..80 {
            let culture = Culture::ALL[index % Culture::ALL.len()];
            let character = generate_character(
                3,
                CharacterId::new(2, u16::try_from(index).unwrap_or(0)),
                culture,
            );
            let offers = offers_of(&character);
            assert!(!offers.is_empty());
            assert_eq!(offers, offers_of(&character), "deterministic");
            for offer in &offers {
                kinds.insert(offer.kind);
                titles.insert(offer.title());
                assert!(!offer.describe().is_empty(), "{offer:?}");
            }
        }
        for kind in OfferKind::ALL {
            assert!(kinds.contains(&kind), "{kind:?} never offered");
        }
        assert!(titles.len() >= 20, "{titles:?}");
    }

    #[test]
    fn friends_pay_less_and_grudges_close_doors() {
        let character = generate_character(3, CharacterId::new(2, 0), Culture::Frontier);
        let offer = offers_of(&character)[0];
        let base = price_for(&offer, Standing::Acquainted, false);
        assert!(price_for(&offer, Standing::Close, false) < base);
        assert!(price_for(&offer, Standing::Cold, false) > base);
        assert!(price_for(&offer, Standing::Acquainted, true) < base);
        assert!(!standing_allows(Standing::Cold, Standing::Acquainted));
        assert!(standing_allows(Standing::Close, Standing::Friendly));
        assert!(!standing_allows(Standing::Acquainted, Standing::Friendly));
    }

    #[test]
    fn some_gifts_land_better_than_others() {
        let character = generate_character(3, CharacterId::new(2, 1), Culture::Mesa);
        let loved = ItemKind::Mineral(super::favourite_mineral(&character));
        assert_eq!(gift_reaction(&character, loved), GiftReaction::Loved);
        assert_eq!(
            gift_reaction(&character, ItemKind::Block(crate::item::BlockKind::Dirt)),
            GiftReaction::Indifferent
        );
    }
}
