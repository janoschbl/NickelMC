use steel_macros::{ClientPacket, WriteTo};
use steel_registry::packets::play::C_PLAYER_COMBAT_END;

/// Tells the client that the player has left combat.
#[derive(ClientPacket, WriteTo, Clone, Copy, Debug)]
#[packet_id(Play = C_PLAYER_COMBAT_END)]
pub struct CPlayerCombatEnd {
    /// Combat duration in ticks.
    #[write(as = VarInt)]
    pub duration: i32,
}
