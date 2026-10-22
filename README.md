<img src="assets/icon.svg" width="64" height="64" alt="">

# NickelMC Slop

NickelMC is a fork of SteelMC, a Minecraft Java server written in Rust. We throw as much AI at it as we can, because we are bored.

Agents write the code and agents review it. Expect rough edges.

## Goal

We start by solving every open pull request and the important issues in SteelMC. After that, we work toward a server where you can survive from the first night to an iron kit.

In this file, "survival" means that a player can gather, craft, smelt, fight four mob types, die, and respawn. The Nether, the End, and villagers come later.

## Where SteelMC stands

SteelMC has networking, encryption, chunk generation, lighting, world saving and loading, player movement, inventories, commands, permissions, food, sleep, and advancements. It has 233 block behavior files and 40 item behavior files.

It has one hostile mob (the endermite) and six passive mobs. Entity spawning is disabled. It has no hostile mob AI, no natural spawning, no player combat parity, no loot system, and no explosions.

## Rules

- Mob behavior must match vanilla 1:1. Agents check their work against the decompiled vanilla source. We never commit that source.
- Every branch passes `cargo test`, `cargo fmt`, `cargo clippy`, and `typos` before it merges.
- Unit tests cover only vanilla-critical behavior, such as loot rolls, damage calculations, and mob attributes.
- Each phase ends with a manual playtest on a real client.
- Each implementer agent gets a reviewer agent. We run at most four concurrent `cargo` builds.

## Roadmap

| Phase | Work | Done when |
|---|---|---|
| 0. Baseline | Install the pinned nightly toolchain, build, and publish the fork. | `cargo test` passes and a 26.3 client joins and walks around. |
| 1. Drops and death | Merge upstream PRs 769 (loot) and 625 (hurt and death). Fix issues 751 (torch and sign drops) and 574. | Blocks drop items, chests hold loot, and the player respawns. |
| 2. Mob brain | Merge PRs 613, 614, and 616. Build the hostile mob foundation (issue 701). | A test mob chases the player and loses track when the player leaves. |
| 3. First monsters | Add the zombie, skeleton, and spider. Then add the creeper, which needs explosions (issue 575, PR 627). | Each mob matches vanilla stats and attack timing in a unit test. |
| 4. Spawning | Merge PR 712 and add the entity spawn stage. | Mobs spawn in darkness, despawn at distance, and respect the mob cap. |
| 5. Survival playtest | Play from the first night to an iron kit. | No crash and no stuck state. |
| 6. Wider content | Villagers, endermen, the Nether, minecarts, and dispensers. | Tracked per feature. |

Parts of phases 1 to 3 touch the entity module, so we merge the brain, spawner, and hostile foundation work one after another. Other work runs in parallel, one git worktree per workstream.

SteelMC caps pull requests at about 1,000 lines and three open at a time. We do not follow that cap in the fork. Once a feature is stable, we may send it upstream in pieces that fit.

## Open decisions

- The final fork name and where we host it. This sets our trademark obligations and where the source offer lives.
- How many agents run at once. We start with four.

## License

SteelMC is licensed under AGPL-3.0-or-later, and so is this fork. We keep the license text and the copyright notices, and we mark our changes with dates. If you run a public server from this code, you must offer players a visible link to download the source.

Created in [T3 Code](https://t3.codes).
