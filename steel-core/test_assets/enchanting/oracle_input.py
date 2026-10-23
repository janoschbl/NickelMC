import json
import pathlib
import sys

root = pathlib.Path("steel-utils/build_assets/builtin_datapacks/minecraft")


def tag(kind, key):
    data = json.loads((root / "tags" / kind / (key.split(":")[-1] + ".json")).read_text())
    values = []
    for entry in data["values"]:
        if isinstance(entry, dict):
            entry = entry["id"]
        values.extend(tag(kind, entry[1:]) if entry.startswith("#") else [entry])
    return list(dict.fromkeys(values))


item = "minecraft:" + (sys.argv[1] if len(sys.argv) > 1 else "diamond_sword")
items = json.loads(pathlib.Path("steel-registry/build_assets/items.json").read_text())["items"]
print(next(i["components"]["minecraft:enchantable"]["value"] for i in items if i["name"] == item.split(":")[-1]))
for name in tag("enchantment", "minecraft:in_enchanting_table"):
    data = json.loads((root / "enchantment" / (name.split(":")[-1] + ".json")).read_text())
    supported = item in tag("item", data["supported_items"][1:])
    primary = "primary_items" not in data or item in tag("item", data["primary_items"][1:])
    if item != "minecraft:book" and not (supported and primary):
        continue
    exclusive = tag("enchantment", data["exclusive_set"][1:]) if "exclusive_set" in data else []
    print("\t".join(map(str, [name, data["weight"], data["max_level"], data["min_cost"]["base"], data["min_cost"]["per_level_above_first"], data["max_cost"]["base"], data["max_cost"]["per_level_above_first"], ",".join(exclusive)])))
