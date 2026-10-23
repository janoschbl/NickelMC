import java.util.*;

// Isolated 26.3 EnchantmentHelper/EnchantmentMenu algorithms; registry inputs arrive
// on stdin from existing vanilla JSON, never from handwritten registry data.
class EnchantmentOffersOracle {
    record Ench(String name, int weight, int maxLevel, int minBase, int minPer,
                int maxBase, int maxPer, Set<String> exclusive) {}
    record Pick(Ench enchantment, int level) {
        public String toString() { return enchantment.name + ":" + level; }
    }
    static Pick weighted(Random random, List<Pick> candidates) {
        int total = candidates.stream().mapToInt(p -> p.enchantment.weight).sum();
        int index = random.nextInt(total);
        for (Pick p : candidates) {
            index -= p.enchantment.weight;
            if (index < 0) return p;
        }
        throw new AssertionError();
    }
    static List<Pick> select(Random random, int cost, int enchantability, List<Ench> source) {
        cost += 1 + random.nextInt(enchantability / 4 + 1) + random.nextInt(enchantability / 4 + 1);
        float span = (random.nextFloat() + random.nextFloat() - 1.0F) * 0.15F;
        cost = Math.max(Math.round(cost + cost * span), 1);
        List<Pick> candidates = new ArrayList<>();
        for (Ench e : source) {
            for (int level = e.maxLevel; level >= 1; level--) {
                if (cost >= e.minBase + e.minPer * (level - 1)
                    && cost <= e.maxBase + e.maxPer * (level - 1)) {
                    candidates.add(new Pick(e, level));
                    break;
                }
            }
        }
        List<Pick> result = new ArrayList<>();
        if (!candidates.isEmpty()) {
            result.add(weighted(random, candidates));
            while (random.nextInt(50) <= cost) {
                Ench last = result.getLast().enchantment;
                candidates.removeIf(p -> last.name.equals(p.enchantment.name)
                    || last.exclusive.contains(p.enchantment.name)
                    || p.enchantment.exclusive.contains(last.name));
                if (candidates.isEmpty()) break;
                result.add(weighted(random, candidates));
                cost /= 2;
            }
        }
        return result;
    }
    public static void main(String[] args) {
        Scanner input = new Scanner(System.in);
        int enchantability = Integer.parseInt(input.nextLine());
        List<Ench> source = new ArrayList<>();
        while (input.hasNextLine()) {
            String[] v = input.nextLine().split("\t", -1);
            source.add(new Ench(v[0], Integer.parseInt(v[1]), Integer.parseInt(v[2]),
                Integer.parseInt(v[3]), Integer.parseInt(v[4]), Integer.parseInt(v[5]),
                Integer.parseInt(v[6]), new HashSet<>(Arrays.asList(v[7].split(",")))));
        }
        for (int seed : new int[]{0, 0x12345678, -1, Integer.MAX_VALUE, Integer.MIN_VALUE}) {
            Random random = new Random(seed);
            int[] costs = new int[3];
            for (int slot = 0; slot < 3; slot++) {
                int selected = random.nextInt(8) + 1 + (15 >> 1) + random.nextInt(16);
                costs[slot] = switch(slot) {
                    case 0 -> Math.max(selected / 3, 1);
                    case 1 -> selected * 2 / 3 + 1;
                    default -> Math.max(selected, 30);
                };
            }
            System.out.println("seed=" + seed + " costs=" + Arrays.toString(costs));
            for (int slot = 0; slot < 3; slot++) {
                random.setSeed(seed + slot); // int overflow before conversion to long
                List<Pick> list = select(random, costs[slot], enchantability, source);
                if (args.length > 0 && args[0].equals("book") && list.size() > 1)
                    list.remove(random.nextInt(list.size()));
                Pick clue = list.isEmpty() ? null : list.get(random.nextInt(list.size()));
                System.out.println(" slot=" + slot + " options=" + list + " clue=" + clue);
            }
        }
    }
}
