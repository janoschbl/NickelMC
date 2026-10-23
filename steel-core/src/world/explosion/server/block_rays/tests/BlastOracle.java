// Standalone oracle for vanilla ServerExplosion.calculateExplodedPositions and the
// Util.shuffle applied by interactWithBlocks. The ray loop is copied from the decompiled
// 26.3 sources; only the level lookup is replaced by the fixture world below.
// Run on OpenJDK 25; see ../FIXTURES.md for extracted resistance arguments.
//
// java.util.Random is the same 48-bit LCG as LegacyRandomSource: identical seed scrambling,
// next(bits), nextInt(bound) and nextFloat() (next(24) * 2^-24).
import java.util.*;

public class BlastOracle {
    record BlockPos(int x, int y, int z) {
        // Vec3i.hashCode
        @Override public int hashCode() { return (y + z * 31) * 31 + x; }
    }

    static final Map<String, Float> resistances = new HashMap<>();

    static float block(String name) { return Objects.requireNonNull(resistances.get(name), name); }

    /** Fixture world: block explosion resistance at a position, or null for air without fluid. */
    static Float resistance(BlockPos p) {
        if (p.y() <= 62) return block("stone");
        if (p.y() == 63) return block("dirt");
        if (p.x() == -2 && p.z() >= -2 && p.z() <= 2 && p.y() >= 64 && p.y() <= 66) return block("glass");
        if (p.x() == 3 && p.z() == 1 && p.y() >= 64 && p.y() <= 65) return block("obsidian");
        if (p.x() >= 1 && p.x() <= 2 && p.z() >= -1 && p.z() <= 0 && p.y() >= 64 && p.y() <= 65) return block("water");
        return null;
    }

    static List<BlockPos> calculateExplodedPositions(double cx, double cy, double cz, float radius, Random random) {
        Set<BlockPos> toBlowSet = new HashSet<>();
        for (int xx = 0; xx < 16; xx++) {
            for (int yy = 0; yy < 16; yy++) {
                for (int zz = 0; zz < 16; zz++) {
                    if (xx == 0 || xx == 15 || yy == 0 || yy == 15 || zz == 0 || zz == 15) {
                        double xd = xx / 15.0F * 2.0F - 1.0F;
                        double yd = yy / 15.0F * 2.0F - 1.0F;
                        double zd = zz / 15.0F * 2.0F - 1.0F;
                        double d = Math.sqrt(xd * xd + yd * yd + zd * zd);
                        xd /= d;
                        yd /= d;
                        zd /= d;
                        float remainingPower = radius * (0.7F + random.nextFloat() * 0.6F);
                        double xp = cx;
                        double yp = cy;
                        double zp = cz;
                        while (remainingPower > 0.0F) {
                            BlockPos pos = new BlockPos((int) Math.floor(xp), (int) Math.floor(yp), (int) Math.floor(zp));
                            if (pos.y() < -64 || pos.y() >= 320) {
                                break;
                            }
                            Float resistance = resistance(pos);
                            if (resistance != null) {
                                remainingPower -= (resistance + 0.3F) * 0.3F;
                            }
                            if (remainingPower > 0.0F) {
                                toBlowSet.add(pos);
                            }
                            xp += xd * 0.3F;
                            yp += yd * 0.3F;
                            zp += zd * 0.3F;
                            remainingPower -= 0.22500001F;
                        }
                    }
                }
            }
        }
        return new ArrayList<>(toBlowSet);
    }

    // Util.shuffle
    static <T> void shuffle(List<T> list, Random random) {
        for (int i = list.size(); i > 1; i--) {
            int swapTo = random.nextInt(i);
            list.set(i - 1, list.set(swapTo, list.get(i - 1)));
        }
    }

    static void run(String name, double cx, double cy, double cz, float radius, long seed) {
        Random random = new Random(seed);
        List<BlockPos> blown = calculateExplodedPositions(cx, cy, cz, radius, random);
        StringBuilder out = new StringBuilder();
        out.append("case ").append(name).append(" count ").append(blown.size()).append('\n');
        out.append("iteration\n");
        for (BlockPos p : blown) out.append(p.x()).append(' ').append(p.y()).append(' ').append(p.z()).append('\n');
        shuffle(blown, random);
        out.append("shuffled\n");
        for (BlockPos p : blown) out.append(p.x()).append(' ').append(p.y()).append(' ').append(p.z()).append('\n');
        System.out.print(out);
    }

    public static void main(String[] args) {
        if (args.length == 1 && args[0].equals("--loot")) {
            for (float radius : new float[] {3.0F, 4.0F}) {
                Random random = new Random(1L);
                int count = 0;
                for (int i = 0; i < 64; i++) {
                    if (random.nextFloat() <= 1.0F / radius) count++;
                }
                System.out.println("radius " + (int) radius + " seed 1 decay64 " + count);
            }
            return;
        }
        for (String arg : args) {
            String[] pair = arg.split("=", 2);
            resistances.put(pair[0], Float.parseFloat(pair[1]));
        }
        run("creeper-radius-3", 0.5, 64.0, 0.5, 3.0F, 20260L);
        run("charged-creeper-radius-6", 0.5, 64.0, 0.5, 6.0F, 20261L);
        run("tnt-radius-4", 0.5, 64.0, 0.5, 4.0F, 20262L);
    }
}
