use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use steel_registry::activity::ActivityRef;
use steel_registry::vanilla_activities;
use steel_utils::locks::SyncMutex;

use steel_utils::Identifier;

use super::{ActivityData, Brain, BrainProvider};
use crate::entity::PathfinderMob;
use crate::entity::ai::brain::behavior::{
    Behavior, BehaviorControl, BehaviorDuration, BehaviorExt,
};
use crate::entity::ai::brain::context::BrainContext;
use crate::entity::ai::brain::memory::PackedMemories;
use crate::entity::ai::brain::memory::{
    MemoryModuleType, MemoryModuleTypeRef, MemoryModuleTypeRegistry, MemoryStatus, RememberedEntity,
};
use crate::entity::ai::brain::sensor::{Sensor, SensorType};
use crate::entity::ai::brain::test_support::{
    TestBrain, Ticks, test_memory, test_persistent_memory,
};

static TARGET: MemoryModuleType<Ticks> = test_memory("target");
static SIGHTING: MemoryModuleType<Ticks> = test_memory("sighting");
static SCRATCH: MemoryModuleType<RememberedEntity> = test_memory("scratch");

const NEEDS_TARGET: &[(MemoryModuleTypeRef, MemoryStatus)] =
    &[(TARGET.entry(), MemoryStatus::ValuePresent)];
const NEEDS_SIGHTING: &[(MemoryModuleTypeRef, MemoryStatus)] =
    &[(SIGHTING.entry(), MemoryStatus::ValuePresent)];

/// Records the order behaviors start in, shared across every probe in a brain.
#[derive(Default)]
struct Journal {
    started: SyncMutex<Vec<&'static str>>,
    ticks: AtomicU32,
    long_running: AtomicBool,
}

impl Journal {
    fn started(&self) -> Vec<&'static str> {
        self.started.lock().clone()
    }
}

struct Probe {
    name: &'static str,
    journal: Arc<Journal>,
    entry_condition: &'static [(MemoryModuleTypeRef, MemoryStatus)],
}

impl Probe {
    fn new(name: &'static str, journal: &Arc<Journal>) -> Self {
        Self {
            name,
            journal: Arc::clone(journal),
            entry_condition: &[],
        }
    }

    fn requiring(
        mut self,
        entry_condition: &'static [(MemoryModuleTypeRef, MemoryStatus)],
    ) -> Self {
        self.entry_condition = entry_condition;
        self
    }
}

impl Behavior for Probe {
    fn entry_condition(&self) -> &[(MemoryModuleTypeRef, MemoryStatus)] {
        self.entry_condition
    }

    fn duration(&self) -> BehaviorDuration {
        BehaviorDuration::fixed(1_000)
    }

    fn start(&mut self, _context: &mut BrainContext<'_>) {
        self.journal.started.lock().push(self.name);
    }

    fn can_still_use(&mut self, _context: &mut BrainContext<'_>) -> bool {
        self.journal.long_running.load(Ordering::Relaxed)
    }

    fn tick(&mut self, _context: &mut BrainContext<'_>) {
        self.journal.ticks.fetch_add(1, Ordering::Relaxed);
    }
}

/// Writes `SIGHTING` on every scan.
struct SightingSensor;

impl Sensor for SightingSensor {
    fn requires(&self) -> &[MemoryModuleTypeRef] {
        const WRITES: &[MemoryModuleTypeRef] = &[SIGHTING.entry()];
        WRITES
    }

    fn scan_rate(&self) -> i32 {
        1
    }

    fn do_tick(&mut self, context: &mut BrainContext<'_>) {
        context.memories.set(&SIGHTING, Ticks(1));
    }
}

fn mob_of(mob: &TestBrain) -> &dyn PathfinderMob {
    mob.mob()
}

fn probes(names: &[&'static str], journal: &Arc<Journal>) -> Vec<Box<dyn BehaviorControl>> {
    names
        .iter()
        .map(|&name| Probe::new(name, journal).control())
        .collect()
}

fn tick(brain: &mut Brain, mob: &mut TestBrain) {
    let context = mob.context();
    brain.tick(context.mob, context.level);
}

#[test]
fn behaviors_and_sensors_register_the_memories_they_declare() {
    let journal = Arc::new(Journal::default());
    let mut brain = Brain::new();
    brain.add_sensor(Box::new(SightingSensor));
    brain.add_activity(ActivityData::new(
        &vanilla_activities::IDLE,
        0,
        vec![
            Probe::new("idle", &journal)
                .requiring(NEEDS_TARGET)
                .control(),
        ],
    ));

    assert!(
        brain.memories().is_registered(TARGET.entry()),
        "a behavior's entry condition should register its memory"
    );
    assert!(
        brain.memories().is_registered(SIGHTING.entry()),
        "a sensor's output memory should register itself"
    );
    assert!(!brain.memories().is_registered(SCRATCH.entry()));
}

#[test]
fn behaviors_start_in_priority_order_and_share_priorities() {
    let journal = Arc::new(Journal::default());
    let mut mob = TestBrain::new();
    let mut brain = Brain::new();
    brain.add_activity(ActivityData::prioritized(
        &vanilla_activities::IDLE,
        vec![
            (5, Probe::new("late", &journal).control()),
            (0, Probe::new("early", &journal).control()),
            (5, Probe::new("late_twin", &journal).control()),
        ],
    ));

    tick(&mut brain, &mut mob);

    assert_eq!(
        journal.started(),
        vec!["early", "late", "late_twin"],
        "lower priorities start first, and a shared priority starts both"
    );
}

#[test]
fn leaving_an_activity_does_not_stop_its_running_behaviors() {
    let journal = Arc::new(Journal::default());
    journal.long_running.store(true, Ordering::Relaxed);
    let mut mob = TestBrain::new();
    let mut brain = Brain::new();
    brain.add_activity(ActivityData::new(
        &vanilla_activities::IDLE,
        0,
        probes(&["idle"], &journal),
    ));
    brain.add_activity(ActivityData::new(
        &vanilla_activities::PANIC,
        0,
        probes(&["panic"], &journal),
    ));

    tick(&mut brain, &mut mob);
    let ticks_while_idle = journal.ticks.load(Ordering::Relaxed);

    brain.set_active_activity_if_possible(&vanilla_activities::PANIC);
    tick(&mut brain, &mut mob);

    assert!(
        journal.ticks.load(Ordering::Relaxed) > ticks_while_idle + 1,
        "the idle behavior should keep ticking alongside the panic one"
    );
    assert_eq!(journal.started(), vec!["idle", "panic"]);
}

#[test]
fn leaving_an_activity_erases_the_memories_it_declared() {
    let journal = Arc::new(Journal::default());
    let mut brain = Brain::new();
    brain.register_memory(TARGET.entry());
    brain.add_activity(ActivityData::new(
        &vanilla_activities::IDLE,
        0,
        probes(&["idle"], &journal),
    ));
    brain.add_activity(
        ActivityData::new(&vanilla_activities::PANIC, 0, probes(&["panic"], &journal))
            .requiring(NEEDS_TARGET.to_vec())
            .erasing_when_stopped(vec![TARGET.entry()]),
    );
    brain.memories_mut().set(&TARGET, Ticks(1));

    brain.set_active_activity_if_possible(&vanilla_activities::PANIC);
    assert!(brain.is_active(&vanilla_activities::PANIC));
    assert!(brain.memories().has_value(TARGET.entry()));

    brain.use_default_activity();

    assert!(
        !brain.memories().has_value(TARGET.entry()),
        "switching away should erase the activity's declared memories"
    );
}

#[test]
fn unmet_requirements_fall_back_to_the_default_activity() {
    let journal = Arc::new(Journal::default());
    let mut brain = Brain::new();
    brain.add_activity(ActivityData::new(
        &vanilla_activities::IDLE,
        0,
        probes(&["idle"], &journal),
    ));
    brain.add_activity(
        ActivityData::new(&vanilla_activities::PANIC, 0, probes(&["panic"], &journal))
            .requiring(NEEDS_TARGET.to_vec()),
    );
    brain.register_memory(TARGET.entry());

    brain.set_active_activity_if_possible(&vanilla_activities::PANIC);
    assert!(brain.is_active(&vanilla_activities::IDLE));

    brain.memories_mut().set(&TARGET, Ticks(1));
    brain.set_active_activity_if_possible(&vanilla_activities::PANIC);
    assert!(brain.is_active(&vanilla_activities::PANIC));

    brain.set_active_activity_if_possible(&vanilla_activities::MEET);
    assert!(
        brain.is_active(&vanilla_activities::IDLE),
        "an activity with no behaviors is unreachable, not silently available"
    );
}

#[test]
fn core_activities_stay_active_across_switches() {
    let journal = Arc::new(Journal::default());
    let mut brain = Brain::new();
    let cores: Vec<ActivityRef> = vec![&vanilla_activities::CORE, &vanilla_activities::SWIM];
    brain.set_core_activities(cores);
    brain.add_activity(ActivityData::new(
        &vanilla_activities::PANIC,
        0,
        probes(&["panic"], &journal),
    ));

    brain.set_active_activity_if_possible(&vanilla_activities::PANIC);

    assert!(brain.is_active(&vanilla_activities::CORE));
    assert!(brain.is_active(&vanilla_activities::SWIM));
    assert_eq!(
        brain.active_non_core_activity(),
        Some(&vanilla_activities::PANIC as ActivityRef)
    );
}

#[test]
fn memories_expire_before_behaviors_read_them() {
    let journal = Arc::new(Journal::default());
    let mut mob = TestBrain::new();
    let mut brain = Brain::new();
    brain.add_activity(ActivityData::new(
        &vanilla_activities::IDLE,
        0,
        vec![
            Probe::new("idle", &journal)
                .requiring(NEEDS_TARGET)
                .control(),
        ],
    ));
    brain.memories_mut().set_with_expiry(&TARGET, Ticks(1), 0);

    tick(&mut brain, &mut mob);

    assert!(
        journal.started().is_empty(),
        "a memory expiring this tick must not let a behavior start"
    );
}

#[test]
fn sensors_write_memories_that_behaviors_start_on_the_same_tick() {
    let journal = Arc::new(Journal::default());
    let mut mob = TestBrain::new();
    let mut brain = Brain::new();
    brain.add_sensor(Box::new(SightingSensor));
    brain.add_activity(ActivityData::new(
        &vanilla_activities::IDLE,
        0,
        vec![
            Probe::new("idle", &journal)
                .requiring(NEEDS_SIGHTING)
                .control(),
        ],
    ));

    tick(&mut brain, &mut mob);

    assert_eq!(
        journal.started(),
        vec!["idle"],
        "a behavior should see the memory its sensor wrote this tick"
    );
}

/// Activities whose behaviors all start under the one priority, in the order
/// Java's `HashMap<Activity, _>` iterates them.
fn start_order(activities: &[(ActivityRef, &'static str)]) -> Vec<&'static str> {
    let journal = Arc::new(Journal::default());
    let mut mob = TestBrain::new();
    let mut brain = Brain::new();
    brain.set_core_activities(activities.iter().map(|&(activity, _)| activity).collect());
    for &(activity, name) in activities {
        brain.add_activity(ActivityData::prioritized(
            activity,
            vec![(0, Probe::new(name, &journal).control())],
        ));
    }
    brain.set_active_activity_if_possible(&vanilla_activities::PANIC);

    tick(&mut brain, &mut mob);

    journal.started()
}

#[test]
fn equal_priority_activities_start_in_java_hash_map_order() {
    // Expected orders were produced by iterating a JDK 21 `HashMap<String, _>`
    // filled in the same order, which hashes exactly as `Activity` does.
    let few = [
        (&vanilla_activities::CORE, "core"),
        (&vanilla_activities::IDLE, "idle"),
        (&vanilla_activities::WORK, "work"),
        (&vanilla_activities::PLAY, "play"),
        (&vanilla_activities::REST, "rest"),
        (&vanilla_activities::MEET, "meet"),
        (&vanilla_activities::PANIC, "panic"),
    ];
    assert_eq!(
        start_order(&few),
        ["play", "core", "rest", "meet", "idle", "work", "panic"]
    );

    // Thirteen entries outgrow the initial 16 buckets, which reorders them.
    let many = [
        (&vanilla_activities::WORK, "work"),
        (&vanilla_activities::REST, "rest"),
        (&vanilla_activities::FIGHT, "fight"),
        (&vanilla_activities::CELEBRATE, "celebrate"),
        (&vanilla_activities::HIDE, "hide"),
        (&vanilla_activities::IDLE, "idle"),
        (&vanilla_activities::CORE, "core"),
        (&vanilla_activities::PANIC, "panic"),
        (&vanilla_activities::RAID, "raid"),
        (&vanilla_activities::PRE_RAID, "pre_raid"),
        (&vanilla_activities::PLAY, "play"),
        (&vanilla_activities::MEET, "meet"),
        (&vanilla_activities::ADMIRE_ITEM, "admire_item"),
    ];
    assert_eq!(
        start_order(&many),
        [
            "play",
            "rest",
            "admire_item",
            "celebrate",
            "idle",
            "work",
            "panic",
            "core",
            "hide",
            "meet",
            "pre_raid",
            "fight",
            "raid"
        ]
    );
}

#[test]
fn readding_an_activity_replaces_the_memories_it_erases() {
    let journal = Arc::new(Journal::default());
    let mut brain = Brain::new();
    brain.register_memory(TARGET.entry());
    brain.add_activity(ActivityData::new(
        &vanilla_activities::IDLE,
        0,
        probes(&["idle"], &journal),
    ));
    brain.add_activity(
        ActivityData::new(&vanilla_activities::PANIC, 0, probes(&["panic"], &journal))
            .erasing_when_stopped(vec![TARGET.entry()]),
    );
    brain.add_activity(ActivityData::new(
        &vanilla_activities::PANIC,
        1,
        probes(&["panic_again"], &journal),
    ));
    brain.add_activity(
        ActivityData::new(&vanilla_activities::PANIC, 2, Vec::new())
            .erasing_when_stopped(vec![SIGHTING.entry()]),
    );
    brain.register_memory(SIGHTING.entry());
    brain.set_active_activity_if_possible(&vanilla_activities::PANIC);
    brain.memories_mut().set(&TARGET, Ticks(1));
    brain.memories_mut().set(&SIGHTING, Ticks(1));

    brain.use_default_activity();

    assert!(
        brain.memories().has_value(TARGET.entry()),
        "the earlier erase set is replaced, not kept beside the new one"
    );
    assert!(!brain.memories().has_value(SIGHTING.entry()));
}

/// Switches the brain to panic as soon as it starts.
struct PanicSwitch;

impl Behavior for PanicSwitch {
    fn start(&mut self, context: &mut BrainContext<'_>) {
        context
            .activities
            .set_active_activity_if_possible(context.memories, &vanilla_activities::PANIC);
    }
}

#[test]
fn behaviors_switch_activity_through_their_context() {
    let journal = Arc::new(Journal::default());
    let mut mob = TestBrain::new();
    let mut brain = Brain::new();
    brain.add_activity(ActivityData::new(
        &vanilla_activities::IDLE,
        0,
        vec![PanicSwitch.control()],
    ));
    brain.add_activity(ActivityData::new(
        &vanilla_activities::PANIC,
        1,
        probes(&["panic"], &journal),
    ));

    tick(&mut brain, &mut mob);

    assert!(brain.is_active(&vanilla_activities::PANIC));
    assert_eq!(
        journal.started(),
        vec!["panic"],
        "the switch lands before the panic group is considered this tick"
    );
}

#[test]
fn schedule_is_consulted_at_most_every_twenty_ticks() {
    let mut brain = Brain::new();
    let journal = Arc::new(Journal::default());
    brain.add_activity(ActivityData::new(
        &vanilla_activities::IDLE,
        0,
        probes(&["idle"], &journal),
    ));
    brain.add_activity(ActivityData::new(
        &vanilla_activities::REST,
        0,
        probes(&["rest"], &journal),
    ));
    let mut asked = Vec::new();
    let mut update = |brain: &mut Brain, game_time: i64| {
        brain.update_activity_from_schedule(game_time, || {
            asked.push(game_time);
            &vanilla_activities::REST
        });
    };

    update(&mut brain, 100);
    assert!(brain.is_active(&vanilla_activities::REST));
    brain.use_default_activity();
    update(&mut brain, 120);
    update(&mut brain, 121);

    assert_eq!(asked, [100, 121]);
    assert!(brain.is_active(&vanilla_activities::REST));
}

/// Remembers that something was seen.
struct SawSomething;

impl Behavior for SawSomething {
    fn entry_condition(&self) -> &[(MemoryModuleTypeRef, MemoryStatus)] {
        NEEDS_SIGHTING
    }

    fn start(&mut self, context: &mut BrainContext<'_>) {
        context.memories.set(&TARGET, Ticks(2));
    }
}

fn saw_something_sensor() -> Box<dyn Sensor> {
    Box::new(SightingSensor)
}

fn idle_activities(_mob: &dyn PathfinderMob) -> Vec<ActivityData> {
    vec![ActivityData::new(
        &vanilla_activities::IDLE,
        0,
        vec![SawSomething.control()],
    )]
}

static SIGHTING_SENSOR: SensorType = SensorType::new(
    Identifier::new_static("steel_test", "sighting"),
    saw_something_sensor,
);
static SAVED_TARGET: MemoryModuleType<Ticks> = test_persistent_memory("saved_target");

#[test]
fn ticked_brain_turns_a_sensed_memory_into_a_behavior_result() {
    let mut mob = TestBrain::new();
    let provider = BrainProvider::new(
        vec![TARGET.entry()],
        vec![&SIGHTING_SENSOR],
        idle_activities,
    );
    let mut brain = provider.make_brain(mob.context().mob, &PackedMemories::default());
    assert!(!brain.memories().has_value(TARGET.entry()));

    tick(&mut brain, &mut mob);

    assert_eq!(
        brain.memories().get(&SIGHTING),
        Some(&Ticks(1)),
        "the sensor wrote what it saw"
    );
    assert_eq!(
        brain.memories().get(&TARGET),
        Some(&Ticks(2)),
        "the behavior started on the sensed memory and wrote its result"
    );
}

#[test]
fn provider_builds_independent_brains_and_restores_packed_memories() {
    let mob = TestBrain::new();
    let provider = BrainProvider::new(
        vec![SAVED_TARGET.entry()],
        vec![&SIGHTING_SENSOR],
        idle_activities,
    );
    let mut original = provider.make_brain(mob_of(&mob), &PackedMemories::default());
    original
        .memories_mut()
        .set_with_expiry(&SAVED_TARGET, Ticks(9), 30);
    original.memories_mut().set(&SIGHTING, Ticks(1));

    let mut registry = MemoryModuleTypeRegistry::new();
    registry.register(&SAVED_TARGET);
    registry.register(&SIGHTING);
    let saved = PackedMemories::from_nbt(&original.pack().to_nbt(), &registry);
    let restored = provider.make_brain(mob_of(&mob), &saved);

    assert_eq!(restored.memories().get(&SAVED_TARGET), Some(&Ticks(9)));
    assert_eq!(
        restored.memories().time_until_expiry(SAVED_TARGET.entry()),
        30
    );
    assert!(
        !restored.memories().has_value(SIGHTING.entry()),
        "a memory that is not persistent is not carried over"
    );
    assert!(restored.memories().is_registered(SIGHTING.entry()));
}
