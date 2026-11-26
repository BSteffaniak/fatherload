//! The endgame of space (Wave 13 endgame track).
//!
//! Space materials, the space credit scale, the late goals a big purse still works toward,
//! the hunt list, completion and its achievements, and homes on alien worlds grown into
//! estates.
//!
//! * [`materials`]: belt ore and comet dust as their own minerals, with uses and prices.
//! * [`economy`]: the one credit scale every station figure is on, and the late purse a rig
//!   brings out of the Heart.
//! * [`works`]: the late goals (ship-line bills, alien tech marks, the Starwright ship, the
//!   Heartline Drive, the Halcyon Gate, estates, the outfitter) and what they do.
//! * [`hunt`]: every boss of the galaxy in one list (theme bosses registered by their tracks).
//! * [`completion`]: the ten categories of a whole game and their share.
//! * [`state`]: what the crew did, the orders, events and refusals.

pub mod completion;
pub mod economy;
pub mod hunt;
pub mod materials;
pub mod state;
pub mod works;

pub use state::{Career, EndgameEvent, EndgameRefusal, EndgameState, Estate, WorkOrder};
pub use works::StarwrightPart;
