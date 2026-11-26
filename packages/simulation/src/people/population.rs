//! A village's residents generated together: families and a relationship graph.

use serde::{Deserialize, Serialize};

use super::{
    PeopleRng,
    character::{AbsentFate, Character, CharacterId, Pronouns, Role, VillageId, WantFocus},
    generate_character,
    names::{self, Culture},
    salt,
    traits::Want,
};

/// How one character relates to another. Edges are directed: `from` sees `to` as this.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum RelationshipKind {
    /// `to` is `from`'s parent.
    Parent,
    /// `to` is `from`'s child.
    Child,
    Sibling,
    Spouse,
    Friend,
    Rival,
    /// `from` owes `to` money.
    Owes,
    /// `to` owes `from` money.
    Lender,
    /// `from` is sweet on `to` (not always returned).
    Sweetheart,
    /// `from` fell out with `to` and regrets it.
    Estranged,
}

impl RelationshipKind {
    /// The share of a reputation change about someone that this relation of theirs takes on
    /// (family half, friends and sweethearts a third, rivals a quarter the other way).
    #[must_use]
    pub const fn reputation_share(self, delta: i16) -> i16 {
        match self {
            kind if kind.is_family() => delta / 2,
            Self::Friend | Self::Sweetheart => delta / 3,
            Self::Rival => -delta / 4,
            _ => 0,
        }
    }

    pub const ALL: [Self; 10] = [
        Self::Parent,
        Self::Child,
        Self::Sibling,
        Self::Spouse,
        Self::Friend,
        Self::Rival,
        Self::Owes,
        Self::Lender,
        Self::Sweetheart,
        Self::Estranged,
    ];

    /// A neutral noun for `to` from `from`'s side.
    #[must_use]
    pub const fn noun(self) -> &'static str {
        match self {
            Self::Parent => "parent",
            Self::Child => "child",
            Self::Sibling => "sibling",
            Self::Spouse => "spouse",
            Self::Friend => "friend",
            Self::Rival => "rival",
            Self::Owes => "creditor",
            Self::Lender => "debtor",
            Self::Sweetheart => "sweetheart",
            Self::Estranged => "old friend",
        }
    }

    /// The noun for `to` given their pronouns ("mother", "brother", "husband", ...).
    #[must_use]
    pub const fn word(self, to: Pronouns) -> &'static str {
        match (self, to) {
            (Self::Parent, Pronouns::She) => "mother",
            (Self::Parent, Pronouns::He) => "father",
            (Self::Child, Pronouns::She) => "daughter",
            (Self::Child, Pronouns::He) => "son",
            (Self::Child, Pronouns::They) => "kid",
            (Self::Sibling, Pronouns::She) => "sister",
            (Self::Sibling, Pronouns::He) => "brother",
            (Self::Spouse, Pronouns::She) => "wife",
            (Self::Spouse, Pronouns::He) => "husband",
            (Self::Spouse, Pronouns::They) => "partner",
            _ => self.noun(),
        }
    }

    /// The kind `to` has towards `from` when the relationship is mutual.
    #[must_use]
    pub const fn reverse(self) -> Option<Self> {
        match self {
            Self::Parent => Some(Self::Child),
            Self::Child => Some(Self::Parent),
            Self::Owes => Some(Self::Lender),
            Self::Lender => Some(Self::Owes),
            Self::Sibling | Self::Spouse | Self::Friend | Self::Rival => Some(self),
            Self::Sweetheart | Self::Estranged => None,
        }
    }

    #[must_use]
    pub const fn is_family(self) -> bool {
        matches!(
            self,
            Self::Parent | Self::Child | Self::Sibling | Self::Spouse
        )
    }
}

/// One directed edge of the relationship graph.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Relationship {
    pub from: CharacterId,
    pub to: CharacterId,
    pub kind: RelationshipKind,
}

/// A village's residents and how they relate.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Population {
    pub village: VillageId,
    pub culture: Culture,
    pub village_name: String,
    pub characters: Vec<Character>,
    /// Sorted, without duplicates.
    pub relationships: Vec<Relationship>,
}

impl Population {
    #[must_use]
    pub fn get(&self, id: CharacterId) -> Option<&Character> {
        self.characters.iter().find(|character| character.id == id)
    }

    /// Everyone `id` has a relationship with, as edges from `id`.
    pub fn relationships_of(&self, id: CharacterId) -> impl Iterator<Item = &Relationship> {
        self.relationships
            .iter()
            .filter(move |relationship| relationship.from == id)
    }

    /// How `from` sees `to`, if they have a relationship (family first).
    #[must_use]
    pub fn relationship(&self, from: CharacterId, to: CharacterId) -> Option<&Relationship> {
        self.relationships_of(from)
            .find(|relationship| relationship.to == to)
    }

    /// How `to` relates to `from`, as a word ("sister", "rival").
    #[must_use]
    pub fn relation_word(&self, from: CharacterId, to: CharacterId) -> Option<&'static str> {
        let relationship = self.relationship(from, to)?;
        let pronouns = self.get(to)?.pronouns;
        Some(relationship.kind.word(pronouns))
    }

    /// How a change in a character's opinion of a player ripples to their circle: family
    /// shares half, friends a third, sweethearts a third, and rivals take a quarter the other
    /// way. WP-PEOPLE applies the result to the [`super::CharacterState`]s.
    #[must_use]
    pub fn reputation_ripple(&self, subject: CharacterId, delta: i16) -> Vec<(CharacterId, i16)> {
        let mut ripple = Vec::new();
        for relationship in self.relationships.iter().filter(|edge| edge.to == subject) {
            let share = relationship.kind.reputation_share(delta);
            if share != 0 {
                ripple.push((relationship.from, share));
            }
        }
        ripple
    }

    fn link(&mut self, from: usize, to: usize, kind: RelationshipKind) {
        let a = self.characters[from].id;
        let b = self.characters[to].id;
        if a == b || self.relationship(a, b).is_some() || self.relationship(b, a).is_some() {
            return;
        }
        self.relationships.push(Relationship {
            from: a,
            to: b,
            kind,
        });
        if let Some(reverse) = kind.reverse() {
            self.relationships.push(Relationship {
                from: b,
                to: a,
                kind: reverse,
            });
        }
    }

    fn related(&self, a: usize, b: usize) -> bool {
        let (a, b) = (self.characters[a].id, self.characters[b].id);
        self.relationship(a, b).is_some() || self.relationship(b, a).is_some()
    }
}

/// How many residents village `village` has (4..=12), deterministic under `seed`.
#[must_use]
pub fn population_size(seed: u64, village: VillageId) -> usize {
    let mut rng = PeopleRng::new(seed, salt::POPULATION, u64::from(village.0), 1);
    usize::try_from(rng.range(4, 12)).unwrap_or(6)
}

/// Generates `count` residents of `village` (a `culture` village) under `seed`, with families,
/// friendships, rivalries, debts and sweethearts.
#[must_use]
pub fn generate_population(
    seed: u64,
    village: VillageId,
    culture: Culture,
    count: usize,
) -> Population {
    let mut rng = PeopleRng::new(seed, salt::POPULATION, u64::from(village.0), 0);
    let characters = (0..count)
        .map(|index| {
            let index = u16::try_from(index).unwrap_or(u16::MAX);
            generate_character(seed, CharacterId { village, index }, culture)
        })
        .collect();
    let mut population = Population {
        village,
        culture,
        village_name: names::village_name(seed, village.0, culture),
        characters,
        relationships: Vec::new(),
    };
    unique_given_names(&mut population, &mut rng);
    add_families(&mut population, &mut rng);
    bind_wants(&mut population, &mut rng);
    add_social_ties(&mut population, &mut rng);
    for index in 0..population.characters.len() {
        let history = super::character::history(seed, &population.characters[index]);
        population.characters[index].history = history;
    }
    population.relationships.sort();
    population.relationships.dedup();
    population
}

/// Re-rolls given names (and nicknames) so nobody in a village shares one, absent people
/// included, which keeps "my brother Elspet" from meaning the neighbour.
fn unique_given_names(population: &mut Population, rng: &mut PeopleRng) {
    let culture = population.culture;
    let mut taken = std::collections::BTreeSet::new();
    for character in &mut population.characters {
        for _ in 0..12 {
            if taken.insert(character.name.given.clone()) {
                break;
            }
            character.name.given = names::given_name(culture, Some(character.pronouns), rng);
        }
        if let Some(nickname) = &character.name.nickname
            && !taken.insert(nickname.clone())
        {
            character.name.nickname = None;
        }
    }
    for character in &mut population.characters {
        for person in &mut character.absent {
            for _ in 0..12 {
                if taken.insert(person.name.given.clone()) {
                    break;
                }
                person.name.given = names::given_name(
                    culture,
                    super::character::relation_pronouns(&person.relation),
                    rng,
                );
            }
        }
    }
}

/// Groups some residents into households of two or three (shared surname) and links them as
/// a coherent family: a parent and child, siblings, a couple, a couple and their child, or a
/// parent of two siblings. A trio whose ages fit none of those (a grandparent, say) is cut
/// back to a pair, so nobody ends up with two parents who are siblings.
fn add_families(population: &mut Population, rng: &mut PeopleRng) {
    let count = population.characters.len();
    let mut order: Vec<usize> = (0..count).collect();
    for index in (1..order.len()).rev() {
        order.swap(index, rng.below(index + 1));
    }
    let mut cursor = 0;
    while cursor + 1 < order.len() {
        if !rng.chance(55) {
            cursor += 1;
            continue;
        }
        let want_three = cursor + 2 < order.len() && rng.chance(35);
        let couple_roll = rng.chance(50);
        let mut members: Vec<usize> =
            order[cursor..cursor + if want_three { 3 } else { 2 }].to_vec();
        members.sort_by_key(|member| std::cmp::Reverse(population.characters[*member].age));
        let ages: Vec<u8> = members
            .iter()
            .map(|member| population.characters[*member].age)
            .collect();
        let links = if want_three { trio_links(&ages) } else { None };
        let links = if let Some(links) = links {
            links
        } else {
            // A pair: the first two of the group (the third goes back to the pool).
            members = order[cursor..cursor + 2].to_vec();
            members.sort_by_key(|member| std::cmp::Reverse(population.characters[*member].age));
            let (senior, junior) = (
                population.characters[members[0]].age,
                population.characters[members[1]].age,
            );
            vec![(0, 1, pair_kind(senior, junior, couple_roll))]
        };
        cursor += members.len();
        let head = members[0];
        let family = population.characters[head].name.family.clone();
        for member in &members[1..] {
            let character = &mut population.characters[*member];
            let old = std::mem::replace(&mut character.name.family, family.clone());
            for absent in &mut character.absent {
                if absent.name.family == old {
                    absent.name.family.clone_from(&family);
                }
            }
        }
        for (senior, junior, kind) in links {
            population.link(members[senior], members[junior], kind);
        }
    }
}

/// How the older of two household members (`senior`, `junior` ages) relates to the younger.
const fn pair_kind(senior: u8, junior: u8, couple_roll: bool) -> RelationshipKind {
    let gap = senior.saturating_sub(junior);
    if gap >= 18 {
        RelationshipKind::Child
    } else if gap <= 10 && junior >= 22 && couple_roll {
        RelationshipKind::Spouse
    } else {
        RelationshipKind::Sibling
    }
}

/// Links for a household of three, oldest first (`ages` descending), as (senior, junior,
/// kind) edges; `None` when their ages make no simple family.
fn trio_links(ages: &[u8]) -> Option<Vec<(usize, usize, RelationshipKind)>> {
    let (a, b, c) = (ages[0], ages[1], ages[2]);
    let (ab, bc, ac) = (a - b, b - c, a - c);
    if ab >= 18 {
        // The oldest is the parent of both; the other two are siblings.
        (bc < 18).then(|| {
            vec![
                (0, 1, RelationshipKind::Child),
                (0, 2, RelationshipKind::Child),
                (1, 2, RelationshipKind::Sibling),
            ]
        })
    } else if bc >= 18 {
        // A couple and their child.
        (ab <= 12).then(|| {
            vec![
                (0, 1, RelationshipKind::Spouse),
                (0, 2, RelationshipKind::Child),
                (1, 2, RelationshipKind::Child),
            ]
        })
    } else {
        // Three siblings.
        (ac < 18).then(|| {
            vec![
                (0, 1, RelationshipKind::Sibling),
                (0, 2, RelationshipKind::Sibling),
                (1, 2, RelationshipKind::Sibling),
            ]
        })
    }
}

/// Points wants about a person at a neighbour where it makes a better story.
fn bind_wants(population: &mut Population, rng: &mut PeopleRng) {
    let count = population.characters.len();
    for index in 0..count {
        let want = population.characters[index].want;
        let kind = match want {
            Want::BeatRival => RelationshipKind::Rival,
            Want::PayOffDebt => RelationshipKind::Owes,
            Want::MakeAmends => RelationshipKind::Estranged,
            _ => continue,
        };
        if !rng.chance(60) {
            continue;
        }
        let candidates: Vec<usize> = (0..count)
            .filter(|other| *other != index && !population.related(index, *other))
            .collect();
        if candidates.is_empty() {
            continue;
        }
        // Debts are usually owed to whoever has money: traders and innkeepers.
        let lenders: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|other| {
                matches!(
                    population.characters[*other].role,
                    Role::Trader | Role::Innkeeper | Role::Smith
                )
            })
            .collect();
        let other = if kind == RelationshipKind::Owes && !lenders.is_empty() {
            *rng.pick(&lenders)
        } else {
            *rng.pick(&candidates)
        };
        population.link(index, other, kind);
        let target = population.characters[other].id;
        let character = &mut population.characters[index];
        if let WantFocus::Absent(slot) = character.want_focus {
            // The neighbour takes the place of the absent stand-in.
            let slot = usize::from(slot);
            if slot < character.absent.len() && character.absent[slot].fate != AbsentFate::WentDeep
            {
                character.absent.remove(slot);
            }
        }
        character.want_focus = WantFocus::Neighbour(target);
    }
}

fn add_social_ties(population: &mut Population, rng: &mut PeopleRng) {
    let count = population.characters.len();
    for index in 0..count {
        let agreeable = population.characters[index].personality.agreeableness;
        let friends = match agreeable {
            70..=u8::MAX => 2,
            35..=69 => 1,
            _ => usize::from(rng.chance(40)),
        };
        for _ in 0..friends {
            let other = rng.below(count);
            if other != index && !population.related(index, other) {
                population.link(index, other, RelationshipKind::Friend);
            }
        }
    }
    let rivalries = count / 4 + usize::from(rng.chance(50));
    for _ in 0..rivalries {
        let a = rng.below(count);
        // Prickly people and people in the same trade make the best rivals.
        let b = (0..count)
            .filter(|other| *other != a && !population.related(a, *other))
            .max_by_key(|other| {
                let character = &population.characters[*other];
                let same_trade = i32::from(character.role == population.characters[a].role) * 60;
                same_trade + 100 - i32::from(character.personality.agreeableness)
                    + i32::try_from(rng.below(40)).unwrap_or(0)
            });
        if let Some(b) = b {
            population.link(a, b, RelationshipKind::Rival);
        }
    }
    let debts = usize::from(count >= 6 && rng.chance(60));
    for _ in 0..debts {
        let (a, b) = (rng.below(count), rng.below(count));
        if a != b && !population.related(a, b) {
            population.link(a, b, RelationshipKind::Owes);
        }
    }
    let sweethearts = count / 5 + usize::from(rng.chance(40));
    for _ in 0..sweethearts {
        let (a, b) = (rng.below(count), rng.below(count));
        let adults = population.characters[a].age >= 18 && population.characters[b].age >= 18;
        let close_in_age = population.characters[a]
            .age
            .abs_diff(population.characters[b].age)
            <= 15;
        if a != b && adults && close_in_age && !population.related(a, b) {
            population.link(a, b, RelationshipKind::Sweetheart);
            if rng.chance(40) {
                let (from, to) = (population.characters[b].id, population.characters[a].id);
                population.relationships.push(Relationship {
                    from,
                    to,
                    kind: RelationshipKind::Sweetheart,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{RelationshipKind, generate_population, population_size};
    use crate::people::{
        character::{VillageId, WantFocus},
        names::Culture,
    };

    #[test]
    fn populations_are_deterministic_and_connected() {
        for village in 0..30 {
            let culture = Culture::ALL[village as usize % Culture::ALL.len()];
            let size = population_size(7, VillageId(village));
            assert!((4..=12).contains(&size));
            let a = generate_population(7, VillageId(village), culture, size);
            let b = generate_population(7, VillageId(village), culture, size);
            assert_eq!(a, b);
            assert_eq!(a.characters.len(), size);
            let given: BTreeSet<&str> = a
                .characters
                .iter()
                .map(|character| character.name.given.as_str())
                .collect();
            assert_eq!(given.len(), size, "village {village} reuses a given name");
            for relationship in &a.relationships {
                assert_ne!(relationship.from, relationship.to);
                assert!(a.get(relationship.from).is_some() && a.get(relationship.to).is_some());
                if let Some(reverse) = relationship.kind.reverse() {
                    assert!(
                        a.relationships
                            .iter()
                            .any(|edge| edge.from == relationship.to
                                && edge.to == relationship.from
                                && edge.kind == reverse),
                        "{relationship:?} has no reverse"
                    );
                }
                if relationship.kind == RelationshipKind::Child {
                    let parent = a.get(relationship.from).unwrap();
                    let child = a.get(relationship.to).unwrap();
                    assert!(parent.age >= child.age + 18);
                }
                if relationship.kind.is_family() {
                    assert_eq!(
                        a.get(relationship.from).unwrap().name.family,
                        a.get(relationship.to).unwrap().name.family
                    );
                }
            }
            for character in &a.characters {
                // Families are coherent: at most two parents, and two parents are a couple.
                let parents: Vec<_> = a
                    .relationships_of(character.id)
                    .filter(|edge| edge.kind == RelationshipKind::Parent)
                    .map(|edge| edge.to)
                    .collect();
                assert!(parents.len() <= 2, "village {village}: {parents:?}");
                if let [first, second] = parents[..] {
                    assert_eq!(
                        a.relationship(first, second).map(|edge| edge.kind),
                        Some(RelationshipKind::Spouse),
                        "village {village}: {} has two parents who are not a couple",
                        character.name.full()
                    );
                }
                if let WantFocus::Neighbour(id) = character.want_focus {
                    assert!(a.relationship(character.id, id).is_some());
                }
            }
        }
    }

    #[test]
    fn villages_have_social_lives() {
        let mut kinds = BTreeSet::new();
        for village in 0..40 {
            let population = generate_population(1, VillageId(village), Culture::Frontier, 8);
            kinds.extend(population.relationships.iter().map(|edge| edge.kind));
        }
        for kind in RelationshipKind::ALL {
            assert!(kinds.contains(&kind), "no {kind:?} in 40 villages");
        }
    }

    #[test]
    fn helping_someone_ripples_to_their_circle() {
        let population = generate_population(1, VillageId(2), Culture::Riverside, 10);
        let subject = population
            .characters
            .iter()
            .find(|character| population.relationships_of(character.id).count() >= 2)
            .unwrap()
            .id;
        let ripple = population.reputation_ripple(subject, 12);
        assert_ne!(ripple, []);
        assert!(ripple.iter().all(|(id, _)| *id != subject));
    }
}
