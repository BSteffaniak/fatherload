use serde::{Deserialize, Serialize};

use crate::{
    player::Player,
    terrain::{ArtifactKind, MineralKind},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContractLog {
    pub active: Contract,
    pub completed: u32,
    #[serde(default)]
    pub story_complete: bool,
}

impl ContractLog {
    #[must_use]
    pub fn new() -> Self {
        Self {
            active: contract_for_index(0),
            completed: 0,
            story_complete: false,
        }
    }

    pub fn try_complete(&mut self, player: &mut Player) -> Option<ContractCompletion> {
        if self.story_complete || !self.active.is_satisfied(player) {
            return None;
        }

        self.active.consume(player);
        let reward = self.active.reward;
        player.credits += reward;
        let completed_title = self.active.title.clone();
        let finished_story = self.active.final_objective;
        self.completed += 1;
        self.story_complete = finished_story;
        if !finished_story {
            self.active = contract_for_index(self.completed);
        }

        Some(ContractCompletion {
            reward,
            completed_title,
            finished_story,
        })
    }
    #[must_use]
    pub const fn story_for_completed(completed: u32) -> &'static str {
        match completed {
            1 => "HQ: Copper confirms the claim. Push toward silver-bearing clay.",
            2 => "HQ: Silver assay approved. Fossils imply old tunnels below.",
            3 => "HQ: Relic recovered. Corporate wants gold calibration samples.",
            4 => "HQ: Gold readings are stable. Heat shielding tests begin.",
            5 => "HQ: Ruby survived high heat. Seek idol chambers deeper down.",
            6 => "HQ: Idol telemetry points to emerald conductors.",
            7 => "HQ: Emerald circuitry is live. A diamond lens can focus the scan.",
            8 => "HQ: Diamond lens aligned. Platinum resonance should stabilize the receiver.",
            9 => "HQ: Platinum receiver is clean. Uranium traces mark the hot path.",
            10 => "HQ: Uranium signature confirmed. Mythril lattice should survive the core field.",
            11 => "HQ: Mythril lattice locked. Recover the old circuit to triangulate the core.",
            _ => "HQ: Star Core coordinates locked. This is the final descent.",
        }
    }

    pub fn migrate_after_load(&mut self) {
        let canonical = contract_for_index(self.completed);
        if self.active.title.is_empty() {
            self.active.title = canonical.title;
            self.active.final_objective = canonical.final_objective;
        }
    }
}

impl Default for ContractLog {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractCompletion {
    pub reward: u32,
    pub completed_title: String,
    pub finished_story: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Contract {
    #[serde(default)]
    pub title: String,
    pub target: ContractTarget,
    pub required: u32,
    pub reward: u32,
    #[serde(default)]
    pub final_objective: bool,
}

impl Contract {
    #[must_use]
    pub fn new(
        title: &str,
        target: ContractTarget,
        required: u32,
        reward: u32,
        final_objective: bool,
    ) -> Self {
        Self {
            title: title.to_owned(),
            target,
            required,
            reward,
            final_objective,
        }
    }

    #[must_use]
    pub fn progress(&self, player: &Player) -> u32 {
        match self.target {
            ContractTarget::Mineral(mineral) => player.cargo.get(&mineral).copied().unwrap_or(0),
            ContractTarget::Artifact(artifact) => {
                player.artifacts.get(&artifact).copied().unwrap_or(0)
            }
        }
    }

    #[must_use]
    pub fn is_satisfied(&self, player: &Player) -> bool {
        self.progress(player) >= self.required
    }

    fn consume(&self, player: &mut Player) {
        match self.target {
            ContractTarget::Mineral(mineral) => {
                consume_count(&mut player.cargo, &mineral, self.required);
            }
            ContractTarget::Artifact(artifact) => {
                consume_count(&mut player.artifacts, &artifact, self.required);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum ContractTarget {
    Mineral(MineralKind),
    Artifact(ArtifactKind),
}

impl ContractTarget {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mineral(mineral) => mineral.name(),
            Self::Artifact(artifact) => artifact.name(),
        }
    }
}

fn contract_for_index(index: u32) -> Contract {
    match index {
        0 => Contract::new(
            "Starter Stockpile",
            ContractTarget::Mineral(MineralKind::Copper),
            4,
            90,
            false,
        ),
        1 => Contract::new(
            "Silver Survey",
            ContractTarget::Mineral(MineralKind::Silver),
            3,
            180,
            false,
        ),
        2 => Contract::new(
            "First Relic",
            ContractTarget::Artifact(ArtifactKind::Fossil),
            1,
            280,
            false,
        ),
        3 => Contract::new(
            "Gold Calibration",
            ContractTarget::Mineral(MineralKind::Gold),
            4,
            360,
            false,
        ),
        4 => Contract::new(
            "Ruby Heat Test",
            ContractTarget::Mineral(MineralKind::Ruby),
            2,
            520,
            false,
        ),
        5 => Contract::new(
            "Idol Below",
            ContractTarget::Artifact(ArtifactKind::BuriedIdol),
            1,
            700,
            false,
        ),
        6 => Contract::new(
            "Emerald Conductors",
            ContractTarget::Mineral(MineralKind::Emerald),
            2,
            860,
            false,
        ),
        7 => Contract::new(
            "Diamond Lens",
            ContractTarget::Mineral(MineralKind::Diamond),
            1,
            1_100,
            false,
        ),
        8 => Contract::new(
            "Platinum Receiver",
            ContractTarget::Mineral(MineralKind::Platinum),
            1,
            1_350,
            false,
        ),
        9 => Contract::new(
            "Uranium Trace",
            ContractTarget::Mineral(MineralKind::Uranium),
            1,
            1_650,
            false,
        ),
        10 => Contract::new(
            "Mythril Lattice",
            ContractTarget::Mineral(MineralKind::Mythril),
            1,
            2_050,
            false,
        ),
        11 => Contract::new(
            "Ancient Machine",
            ContractTarget::Artifact(ArtifactKind::OldCircuit),
            1,
            2_350,
            false,
        ),
        _ => Contract::new(
            "The Star Core",
            ContractTarget::Artifact(ArtifactKind::StarCore),
            1,
            1_500,
            true,
        ),
    }
}

fn consume_count<K: Ord>(items: &mut std::collections::BTreeMap<K, u32>, key: &K, count: u32) {
    let Some(available) = items.get_mut(key) else {
        return;
    };

    *available = available.saturating_sub(count);
    if *available == 0 {
        items.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use crate::{contract::ContractLog, player::Player, terrain::MineralKind};

    #[test]
    fn contract_completion_consumes_items_and_pays_reward() {
        let mut player = Player::new(0.0, 0.0);
        for _ in 0..4 {
            assert!(player.add_cargo(MineralKind::Copper));
        }

        let mut contracts = ContractLog::new();
        let completion = contracts
            .try_complete(&mut player)
            .expect("contract complete");

        assert_eq!(completion.reward, 90);
        assert_eq!(player.credits, 90);
        assert_eq!(player.cargo_used(), 0);
        assert_eq!(contracts.active.title, "Silver Survey");
    }
}
