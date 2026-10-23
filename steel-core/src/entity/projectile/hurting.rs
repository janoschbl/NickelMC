//! Accelerating, non-gravity projectile movement shared by fireballs and skulls.

use std::sync::Arc;

use glam::DVec3;
use simdnbt::{borrow::NbtCompound as BorrowedNbtCompoundView, owned::NbtCompound};
use steel_utils::{ChunkPos, locks::SyncMutex};

use crate::entity::{RemovalReason, projectile::Projectile};

/// Mutable acceleration shared by hurting projectiles.
pub struct HurtingProjectileBase {
    acceleration_power: SyncMutex<f64>,
}

impl Default for HurtingProjectileBase {
    fn default() -> Self {
        Self {
            acceleration_power: SyncMutex::new(0.1),
        }
    }
}

/// Behavior for vanilla's `AbstractHurtingProjectile` hierarchy.
pub trait AbstractHurtingProjectile: Projectile {
    /// Returns this projectile's acceleration state.
    fn hurting_projectile_base(&self) -> &HurtingProjectileBase;

    /// Returns the current acceleration magnitude.
    fn acceleration_power(&self) -> f64 {
        *self.hurting_projectile_base().acceleration_power.lock()
    }

    /// Assigns the initial normalized direction and synchronizes velocity.
    fn assign_directional_movement(&self, direction: DVec3) {
        self.set_velocity(normalize(direction) * self.acceleration_power());
        self.mark_velocity_sync();
    }

    /// Air inertia, widened from vanilla's float.
    fn hurting_inertia(&self) -> f32 {
        0.95
    }

    /// Water inertia, widened from vanilla's float.
    fn hurting_liquid_inertia(&self) -> f32 {
        0.8
    }

    /// Whether this projectile ignites itself during flight.
    fn should_burn(&self) -> bool {
        true
    }

    /// Adjusts acceleration after deflection.
    fn hurting_on_deflection(&self, by_attack: bool) {
        let mut acceleration = self.hurting_projectile_base().acceleration_power.lock();
        *acceleration = if by_attack { 0.1 } else { *acceleration * 0.5 };
    }

    /// Advances inertia before the raycast, then resolves movement and impact.
    fn hurting_projectile_tick(self: Arc<Self>) {
        self.set_old_position_to_current();
        self.base().set_old_rotation_to_current();
        let movement = self.velocity();
        let inertia = if self.is_in_water() {
            self.hurting_liquid_inertia()
        } else {
            self.hurting_inertia()
        };
        self.set_velocity(
            (movement + normalize(movement) * self.acceleration_power()) * f64::from(inertia),
        );

        let Some(world) = self.level() else { return };
        if self.get_owner().is_some_and(|owner| owner.is_removed())
            || world
                .chunk_map
                .with_full_chunk(ChunkPos::from_block_pos(self.block_position()), |_| ())
                .is_none()
        {
            self.set_removed(RemovalReason::Discarded);
            return;
        }
        let hit = self.get_hit_result_on_move_vector();
        let position = hit
            .as_ref()
            .map_or_else(|| self.position() + self.velocity(), |hit| hit.location());
        self.rotate_towards_movement(0.2);
        if let Err(error) = self.try_set_position(position) {
            log::debug!(
                "failed to advance hurting projectile {}: {error}",
                self.id()
            );
            self.set_removed(RemovalReason::Discarded);
            return;
        }
        self.apply_effects_from_blocks();
        self.projectile_base_tick();
        if self.should_burn() {
            self.ignite_for_ticks(20);
        }
        if let Some(hit) = hit
            && self.is_alive()
        {
            self.hit_target_or_deflect_self(&hit);
        }
        // Bubbles and trail particles are client-local in vanilla.
    }

    /// Uses the distinct `ProjectileUtil` facing convention for hurting projectiles.
    fn rotate_towards_movement(&self, speed: f32) {
        let movement = self.velocity();
        if movement.length_squared() == 0.0 {
            return;
        }
        let yaw = (steel_math::trig::atan2(movement.z, movement.x) * 180.0
            / f64::from(std::f32::consts::PI)) as f32
            + 90.0;
        let horizontal = (movement.x * movement.x + movement.z * movement.z).sqrt();
        let pitch = (steel_math::trig::atan2(horizontal, movement.y) * 180.0
            / f64::from(std::f32::consts::PI)) as f32
            - 90.0;
        let (mut old_yaw, mut old_pitch) = self.base().old_rotation();
        while pitch - old_pitch < -180.0 {
            old_pitch -= 360.0;
        }
        while pitch - old_pitch >= 180.0 {
            old_pitch += 360.0;
        }
        while yaw - old_yaw < -180.0 {
            old_yaw -= 360.0;
        }
        while yaw - old_yaw >= 180.0 {
            old_yaw += 360.0;
        }
        self.base().set_old_rotation((old_yaw, old_pitch));
        self.set_rotation((
            old_yaw + speed * (yaw - old_yaw),
            old_pitch + speed * (pitch - old_pitch),
        ));
    }

    /// Saves the inherited projectile fields and acceleration.
    fn save_hurting_projectile(&self, nbt: &mut NbtCompound) {
        self.save_projectile(nbt);
        nbt.insert("acceleration_power", self.acceleration_power());
    }

    /// Loads the inherited projectile fields and acceleration.
    fn load_hurting_projectile(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_projectile(nbt);
        *self.hurting_projectile_base().acceleration_power.lock() =
            nbt.double("acceleration_power").unwrap_or(0.1);
    }
}

fn normalize(vector: DVec3) -> DVec3 {
    let length = vector.length();
    if length < 1.0e-5 {
        DVec3::ZERO
    } else {
        vector / length
    }
}
