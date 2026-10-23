# Enchanting offer oracle

`EnchantmentOffersOracle.java` reproduces the isolated Minecraft Java 26.3
`EnchantmentHelper` selection and `EnchantmentMenu` offer algorithms with
`java.util.Random`. It generates the costs, ordered enchantment lists, and clues
used by `fixed_seed_offers_match_vanilla_26_3`.

The oracle uses JDK 25. It does not load a Minecraft jar or run a server.
The source reference is the decompiled 26.3 `EnchantmentHelper.java` and
`EnchantmentMenu.java` under `minecraft-src/minecraft/src/net/minecraft/`.
`oracle_input.py` reads enchantability and ordered enchantment tags from the
repository's extracted 26.3 items and builtin datapack. It contains no registry
fixtures of its own.

From the repository root, with JDK 25 on `PATH`, run:

```sh
python3 steel-core/test_assets/enchanting/oracle_input.py diamond_sword | java steel-core/test_assets/enchanting/EnchantmentOffersOracle.java
python3 steel-core/test_assets/enchanting/oracle_input.py book | java steel-core/test_assets/enchanting/EnchantmentOffersOracle.java book
```

The seeds include signed overflow when the slot index is added. Book selection
also exercises result removal before the clue draw. Compare the printed results
with the Rust test before changing its expected values.
