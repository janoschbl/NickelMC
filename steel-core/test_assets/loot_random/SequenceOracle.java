import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.Random;

// Standalone transcription of Minecraft 26.3 RandomSequence/RandomSupport and
// XoroshiroRandomSource. Run: java SequenceOracle.java (JDK 25).
class SequenceOracle {
    long lo, hi;
    SequenceOracle(long seed, String key) throws Exception {
        var hash = ByteBuffer.wrap(MessageDigest.getInstance("MD5").digest(key.getBytes(StandardCharsets.UTF_8)));
        long low = seed ^ 0x6a09e667f3bcc909L;
        lo = mix(low ^ hash.getLong());
        hi = mix((low + 0x9e3779b97f4a7c15L) ^ hash.getLong());
        if ((lo | hi) == 0) { lo = 0x9e3779b97f4a7c15L; hi = 0x6a09e667f3bcc909L; }
    }
    static long mix(long n) {
        n = (n ^ (n >>> 30)) * 0xbf58476d1ce4e5b9L;
        n = (n ^ (n >>> 27)) * 0x94d049bb133111ebL;
        return n ^ (n >>> 31);
    }
    long nextLong() {
        long result = Long.rotateLeft(lo + hi, 17) + lo;
        long x = hi ^ lo;
        lo = Long.rotateLeft(lo, 49) ^ x ^ (x << 21);
        hi = Long.rotateLeft(x, 28);
        return result;
    }
    float nextFloat() { return (nextLong() >>> 40) * 0x1.0p-24f; }
    int nextInt(int bound) {
        long n = Integer.toUnsignedLong((int)nextLong());
        long product = n * bound;
        long low = product & 0xffffffffL;
        if (low < bound) {
            long threshold = Integer.toUnsignedLong(Integer.remainderUnsigned(-bound, bound));
            while (low < threshold) {
                product = Integer.toUnsignedLong((int)nextLong()) * bound;
                low = product & 0xffffffffL;
            }
        }
        return (int)(product >>> 32);
    }
    public static void main(String[] args) throws Exception {
        for (long seed : new long[]{0, 1234, -1}) {
            var rng = new SequenceOracle(seed, "minecraft:blocks/stone");
            System.out.println(seed + " first=" + rng.nextLong());
            System.out.print("mixed=");
            for (int i = 0; i < 4; i++)
                System.out.print(" " + Float.floatToIntBits(rng.nextFloat()) + ":" + rng.nextInt(1073741825));
            System.out.println();
            rng = new SequenceOracle(seed, "minecraft:blocks/stone");
            int count = 0;
            for (int i = 0; i < 64; i++) if (rng.nextFloat() <= 0.25f) count++;
            System.out.println("survival64=" + count);
        }
        var legacy = new Random(1);
        System.out.print("legacy=");
        for (int i = 0; i < 4; i++)
            System.out.print(" " + Float.floatToIntBits(legacy.nextFloat()) + ":" + legacy.nextInt(1073741825));
        System.out.println();
    }
}
