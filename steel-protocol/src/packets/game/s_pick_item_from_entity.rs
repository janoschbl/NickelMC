use steel_macros::{ReadFrom, ServerPacket};

/// Serverbound pick-item action targeting an entity or multipart hitbox.
#[derive(ReadFrom, ServerPacket, Clone, Debug)]
pub struct SPickItemFromEntity {
    #[read(as = VarInt)]
    pub id: i32,
    pub include_data: bool,
}
