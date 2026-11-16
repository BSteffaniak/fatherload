//! The Explosives Shack's arms rack: hardpoint weapons, their upgrades, the capacitor and
//! rocket ammo. The shack's counter lists these rows after the explosive packs; the shell and
//! the counter panel both read [`ARMS_ROWS`], so a row's label always matches its request.

use fatherload_simulation::{
    AmmoKind, Player, SessionServiceRequest, WeaponKind, explosive::EXPLOSIVE_OFFERS,
    pilot::PilotGearKind,
};

/// One arms rack row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArmsRow {
    /// Buy the weapon, or upgrade it once owned.
    Weapon(WeaponKind),
    /// The next capacitor level.
    Battery,
    /// A pack of ammunition.
    Ammo(AmmoKind),
    /// The next tier of the pilot's own gear (Part II F5: jetpack, suit).
    Gear(PilotGearKind),
}

/// The arms rack, in counter order.
pub const ARMS_ROWS: [ArmsRow; 14] = [
    ArmsRow::Weapon(WeaponKind::RivetGun),
    ArmsRow::Weapon(WeaponKind::Harpoon),
    ArmsRow::Weapon(WeaponKind::ScatterBlaster),
    ArmsRow::Weapon(WeaponKind::PlasmaCutter),
    ArmsRow::Weapon(WeaponKind::RocketLauncher),
    ArmsRow::Weapon(WeaponKind::Railgun),
    // Act II weapons (T21): sky worlds only, paid partly in sky materials.
    ArmsRow::Weapon(WeaponKind::TetherHarpoon),
    ArmsRow::Weapon(WeaponKind::ArcRifle),
    ArmsRow::Weapon(WeaponKind::FlakCannon),
    ArmsRow::Weapon(WeaponKind::GravityGun),
    ArmsRow::Battery,
    ArmsRow::Ammo(AmmoKind::Rocket),
    ArmsRow::Gear(PilotGearKind::Jetpack),
    ArmsRow::Gear(PilotGearKind::Suit),
];

/// Shack counter rows before the arms rack: four bomb rows, then the explosive packs.
pub const ARMS_FIRST_ROW: usize = 4 + EXPLOSIVE_OFFERS.len();

/// The arms row at shack menu item `item`, if it is one.
#[must_use]
pub fn row_at(item: usize) -> Option<ArmsRow> {
    item.checked_sub(ARMS_FIRST_ROW)
        .and_then(|index| ARMS_ROWS.get(index).copied())
}

/// The request a row submits for `player`.
#[must_use]
pub fn request(row: ArmsRow, player: Option<&Player>) -> SessionServiceRequest {
    match row {
        ArmsRow::Weapon(weapon) => {
            if player.is_some_and(|player| player.weapon_rack.owned.contains_key(&weapon)) {
                SessionServiceRequest::UpgradeWeapon { weapon }
            } else {
                SessionServiceRequest::BuyWeapon { weapon }
            }
        }
        ArmsRow::Battery => SessionServiceRequest::UpgradeBattery,
        ArmsRow::Ammo(ammo) => SessionServiceRequest::BuyAmmo { ammo },
        ArmsRow::Gear(gear) => SessionServiceRequest::BuyPilotGear { gear },
    }
}

/// "Mk II" for upgrade tier 1.
#[must_use]
pub const fn mk_name(mk: u8) -> &'static str {
    match mk {
        0 => "Mk I",
        1 => "Mk II",
        2 => "Mk III",
        _ => "Mk IV",
    }
}
