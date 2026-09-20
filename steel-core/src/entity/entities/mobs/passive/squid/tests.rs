use steel_registry::{init_vanilla_registry, vanilla_entities};

use super::*;

fn test_squid() -> SquidEntity {
    init_vanilla_registry();
    SquidEntity::new(&vanilla_entities::SQUID, 1, DVec3::ZERO, Weak::new())
}

#[test]
fn squid_initializes_vanilla_attributes_and_health() {
    let squid = test_squid();

    assert_eq!(squid.get_health().to_bits(), 10.0_f32.to_bits());
    assert_eq!(
        squid
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MAX_HEALTH)
            .to_bits(),
        10.0_f64.to_bits()
    );
    assert_eq!(squid.get_default_gravity().to_bits(), 0.08_f64.to_bits());
    assert_eq!(
        LivingEntity::sound_volume(&squid).to_bits(),
        0.4_f32.to_bits()
    );
}

/// Vanilla inherits 120 from `AgeableWaterCreature`; Steel's default is 80.
#[test]
fn squid_uses_the_water_creature_ambient_sound_interval() {
    let squid = test_squid();

    assert_eq!(Mob::ambient_sound_interval(&squid), 120);
}

#[test]
fn baby_squid_uses_vanilla_baby_dimensions() {
    let squid = test_squid();

    let adult = squid.dimensions_for_pose(EntityPose::Standing);
    AgeableMob::set_baby(&squid, true);
    let baby = squid.dimensions_for_pose(EntityPose::Standing);

    assert_eq!(baby.width.to_bits(), SQUID_BABY_WIDTH.to_bits());
    assert_eq!(baby.height.to_bits(), SQUID_BABY_HEIGHT.to_bits());
    assert_ne!(baby.width.to_bits(), adult.width.to_bits());
}

/// A nose-down squid inks behind itself, so Z comes out positive. Reaching for
/// `DVec3::rotate_x` mirrors it and the ink squirts forwards instead.
#[test]
fn rotate_vector_trails_ink_behind_a_pitched_squid() {
    let squid = test_squid();
    squid.state.lock().x_body_rot_old = 30.0;

    let rotated = squid.rotate_vector(DVec3::new(0.0, -1.0, 0.0));

    assert!(
        (rotated - DVec3::new(0.0, -0.866_025, 0.5)).length() < 1e-4,
        "ink must trail behind the squid, got {rotated:?}"
    );
}

/// Pins that the entity flag reaches the entity, not the `true` trait default.
#[test]
fn squid_is_not_pushed_by_fluid() {
    let squid = test_squid();

    assert!(!squid.is_pushed_by_fluid());
}
