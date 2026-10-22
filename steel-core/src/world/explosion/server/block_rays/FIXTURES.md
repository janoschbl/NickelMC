# Explosion test fixtures

These files are captured from the Minecraft server, not hand-written. Do not edit them by hand.

## `java_block_pos_set/tests/openjdk25*.txt`

Output of `java_block_pos_set/tests/HashSetOracle.java`, which inserts `BlockPos` values into a real
`java.util.HashSet` and prints the bucket count, tree-bin count and iteration/tree-shape digests after
every insertion. `JavaBlockPosSet` is checked against it by `java_block_pos_set/tests.rs`.

Run `HashSetOracle` on OpenJDK 25 with Minecraft's server and library jars on the classpath
(`net.minecraft.core.BlockPos` must resolve) and `--add-opens java.base/java.util=ALL-UNNAMED`:

- `openjdk25.txt`: run with no program arguments.
- `openjdk25-sequential-identity.txt`: run with `--sequential-identities` and the VM options
  `-XX:+UnlockExperimentalVMOptions -XX:hashCode=3`, so `System.identityHashCode` increases monotonically
  and the tie-break order of equal-hash keys is reproducible.

## `tests/vanilla-tnt-order.txt`

Radius 4 TNT blast captured from vanilla (`iteration`, `callbacks`, `fuses`, `surviving-tnt` sections and
the `insertions-sha256` line). The inputs reproduced by `tests/gameplay.rs` are:

- Obsidian everywhere in `x, z ∈ [-4, 20]`, `y ∈ [52, 80]` except an air strip at `y ∈ [65, 67]` where
  `|x + z - 16| <= 1`.
- TNT blocks at `(6, 65, 9)`, `(10, 65, 5)` and `(8, 65, 7)`.
- Source: a primed TNT at `(8.5, 65, 8.5)`, explosion centre `y` raised by `height * 0.0625`, radius 4,
  `LegacyRandom` seed 1 for the rays and the shuffle.

The vanilla harness that produced this file is not in the repository.
