<img src="assets/icon.svg" width="64" height="64" alt="">

# NickelMC Slop

NickelMC is a fork of SteelMC, a Minecraft Java server written in Rust. I was bored, so I gave it to AI.

Agents write the code. Other agents review it. I read the commit messages, sometimes.

## Goal

First I merge every open SteelMC pull request and fix the important issues. Then I make survival work. Survival means a player can gather, craft, smelt, fight four mob types, die, and respawn. The Nether, the End, and villagers wait.

## Where SteelMC stands

SteelMC has networking, encryption, chunk generation, lighting, world saving, player movement, inventories, commands, permissions, food, sleep, and advancements. It has 233 block behavior files and 40 item behavior files.

It has one hostile mob, the endermite, and six passive mobs. Entity spawning is off. There is no hostile mob AI, no natural spawning, no player combat parity, no loot system, and no explosions. You can walk around a very safe world.

## Rules

- Mob behavior matches vanilla exactly. Agents check against the decompiled vanilla source. I never commit that source.
- Every branch passes `cargo test`, `cargo fmt`, `cargo clippy`, and `typos` before it merges.
- Unit tests cover only what vanilla depends on: loot rolls, damage, and mob attributes.
- Each phase ends with a playtest on a real client. I do the playtest myself.
- Every implementer agent gets a reviewer agent. At most four `cargo` builds run at once.

## Roadmap

| Phase | Work | Done when |
|---|---|---|
| 0. Baseline | Install the pinned nightly toolchain, build, and publish the fork. | `cargo test` passes and a 26.3 client joins and walks around. |
| 1. Drops and death | Merge upstream PRs 769 (loot) and 625 (hurt and death). Fix issues 751 (torch and sign drops) and 574. | Blocks drop items, chests hold loot, and the player respawns. |
| 2. Mob brain | Merge PRs 613, 614, and 616. Build the hostile mob foundation (issue 701). | A test mob chases the player and loses track when the player leaves. |
| 3. First monsters | Add the zombie, skeleton, and spider. Then the creeper, which needs explosions (issue 575, PR 627). | Each mob matches vanilla stats and attack timing in a unit test. |
| 4. Spawning | Merge PR 712 and add the entity spawn stage. | Mobs spawn in darkness, despawn at distance, and respect the mob cap. |
| 5. Survival playtest | Play from the first night to an iron kit. | No crash and no stuck state. |
| 6. Wider content | Villagers, endermen, the Nether, minecarts, and dispensers. | Tracked per feature. |

Phases 1 to 3 touch the entity module, so the brain, spawner, and hostile foundation merge one after another. Other work runs in parallel, one git worktree per workstream.

SteelMC limits pull requests to about 1,000 lines and three open at a time. This fork has no such limit. When a feature is stable, I may send it upstream in pieces that fit.

## Open decisions

- The final fork name and where it lives. The name decides the trademark obligations. The host decides where the source offer goes.
- How many agents run at once. I start with four.

## License

SteelMC is AGPL-3.0-or-later, and so is this fork. I keep the license text and the copyright notices, and I date my changes. If you run a public server from this code, you must offer your players a visible link to the source.

Created in [T3 Code](https://t3.codes).
