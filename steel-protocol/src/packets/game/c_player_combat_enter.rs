use steel_macros::{ClientPacket, WriteTo};
use steel_registry::packets::play::C_PLAYER_COMBAT_ENTER;

/// Tells the client that the player has entered combat.
#[derive(ClientPacket, WriteTo, Clone, Copy, Debug, Default)]
#[packet_id(Play = C_PLAYER_COMBAT_ENTER)]
pub struct CPlayerCombatEnter;
