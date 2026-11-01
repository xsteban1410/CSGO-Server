"""Genera addons/sourcemod/configs/skins.cfg a partir de los archivos del juego.

Uso: python3 -I tools/generar_skins.py <carpeta csgo del servidor> <salida.cfg>
Lee scripts/items/items_game.txt y resource/csgo_latam.txt (+ csgo_english.txt de respaldo).
Ordena las skins: primero las más buscadas (lista POPULARES), luego por rareza y nombre.
"""
import re
import sys
from pathlib import Path

POPULARES = [
    "Dragon Lore", "Howl", "Fire Serpent", "Fade", "Doppler", "Gamma Doppler", "Marble Fade",
    "Tiger Tooth", "Lore", "Autotronic", "Crimson Web", "Case Hardened", "Slaughter", "Asiimov",
    "Printstream", "Hyper Beast", "Neo-Noir", "Vulcan", "Redline", "Bloodsport", "The Empress",
    "Wild Lotus", "Gungnir", "Medusa", "Kill Confirmed", "Fever Dream", "Neon Rider", "Blaze",
    "Hot Rod", "Golden Coil", "Hydroponic", "Fuel Injector", "Gold Arabesque", "Lightning Strike",
    "Cyrex", "Oni Taiji", "Damascus Steel", "Ultraviolet", "Bright Water", "Blue Steel", "Night",
    "Stained", "Black Laminate", "Emerald Pinstripe", "Aquamarine Revenge", "Jaguar", "Head Shot",
    "Code Red", "Mecha Industries", "Desolate Space", "Hellfire", "Orion", "Kill Confirmed",
    "Cortex", "Bullet Rain", "Nightmare", "Chantico's Fire", "Icarus Fell", "Atheris", "The Prince",
]
RAREZA = {"immortal": 7, "ancient": 6, "legendary": 5, "mythical": 4, "rare": 3, "uncommon": 2, "common": 1}
TOKEN = re.compile(r'"((?:[^"\\]|\\.)*)"|([{}])|//[^\n]*')


def parse_vdf(text):
    """VDF -> lista de (clave, valor) conservando claves repetidas; valor es str o lista."""
    stack, cur, key = [], [], None
    for m in TOKEN.finditer(text):
        s, brace = m.group(1), m.group(2)
        if brace == "{":
            new = []
            cur.append((key, new))
            stack.append(cur)
            cur, key = new, None
        elif brace == "}":
            cur = stack.pop() if stack else cur
        elif s is not None:
            if key is None:
                key = s
            else:
                cur.append((key, s))
                key = None
    return cur


def sections(tree, name):
    for k, v in tree:
        if isinstance(v, list):
            if k.lower() == name:
                yield v
            yield from sections(v, name)


def tokens(path):
    if not path.exists():
        return {}
    raw = path.read_bytes()
    text = raw.decode("utf-16") if raw[:2] in (b"\xff\xfe", b"\xfe\xff") else raw.decode("utf-8", "replace")
    out = {}
    for sec in sections(parse_vdf(text), "tokens"):
        for k, v in sec:
            if isinstance(v, str):
                out[k.lower()] = v
    return out


VARIANTES = {"phase1": "Fase 1", "phase2": "Fase 2", "phase3": "Fase 3", "phase4": "Fase 4", "ruby": "Rubí",
             "sapphire": "Zafiro", "blackpearl": "Perla negra", "emerald": "Esmeralda"}


def variante(kit):
    for k, v in VARIANTES.items():
        if k in kit:
            return v
    return kit


def esc(s):
    return s.replace('"', "'")


def main(csgo, salida):
    csgo = Path(csgo)
    items = parse_vdf((csgo / "scripts/items/items_game.txt").read_text(encoding="utf-8", errors="replace"))
    en = tokens(csgo / "resource/csgo_english.txt")
    es = tokens(csgo / "resource/csgo_latam.txt")

    def loc(tag, default=""):
        tag = (tag or "").lstrip("#").lower()
        return es.get(tag) or en.get(tag) or default

    kits = {}      # nombre interno -> (id, nombre es, nombre en)
    for sec in sections(items, "paint_kits"):
        for pid, body in sec:
            if not isinstance(body, list) or not pid.isdigit() or pid == "0":
                continue
            d = {k.lower(): v for k, v in body if isinstance(v, str)}
            if "name" in d and "description_tag" in d:
                kits[d["name"].lower()] = (int(pid), loc(d["description_tag"]), en.get(d["description_tag"].lstrip("#").lower(), ""))
    rareza = {}
    for sec in sections(items, "paint_kits_rarity"):
        for k, v in sec:
            if isinstance(v, str):
                rareza[k.lower()] = RAREZA.get(v, 0)

    # arma -> conjunto de skins, sacado de las listas de botín "[skin]weapon_xxx"
    por_arma = {}
    loot = re.compile(r"^\[([^\]]+)\](weapon_[a-z0-9_]+)$")

    def walk(tree):
        for k, v in tree:
            m = loot.match(k)
            if m and m.group(1).lower() in kits:
                por_arma.setdefault(m.group(2), set()).add(m.group(1).lower())
            if isinstance(v, list):
                walk(v)
    walk(items)

    # Los cuchillos solo aparecen en iconos: econ/default_generated/<arma>_<skin>_light
    armas_conocidas, indices = set(), {}
    for sec in sections(items, "items"):
        for idx, body in sec:
            if isinstance(body, list):
                n = next((v for k, v in body if k.lower() == "name" and isinstance(v, str)), "")
                if n.startswith("weapon_"):
                    armas_conocidas.add(n)
                    if idx.isdigit():
                        indices.setdefault(n, idx)
    icono = re.compile(r"^econ/default_generated/(weapon_[a-z0-9_]+)_light$")

    def iconos(tree):
        for k, v in tree:
            if isinstance(v, list):
                iconos(v)
            elif k.lower() == "icon_path" and (m := icono.match(v)):
                partes = m.group(1)
                for i in range(len(partes)):
                    if partes[i] == "_" and partes[:i] in armas_conocidas and partes[i + 1:] in kits:
                        por_arma.setdefault(partes[:i], set()).add(partes[i + 1:])
                        break
    iconos(items)

    nombres = {}   # weapon_xxx -> nombre visible
    for sec in sections(items, "items"):
        for _, body in sec:
            if isinstance(body, list):
                d = {k.lower(): v for k, v in body if isinstance(v, str)}
                n = d.get("name", "")
                if n.startswith("weapon_") and "item_name" in d:
                    nombres.setdefault(n, loc(d["item_name"], n))
    for sec in sections(items, "prefabs"):
        for pname, body in sec:
            if isinstance(body, list) and pname.endswith("_prefab") and pname.startswith("weapon_"):
                d = {k.lower(): v for k, v in body if isinstance(v, str)}
                if "item_name" in d:
                    nombres.setdefault(pname[:-7], loc(d["item_name"], pname[:-7]))

    pop = {n.lower(): i for i, n in enumerate(POPULARES)}

    def orden(kit):
        pid, es_n, en_n = kits[kit]
        return (pop.get(en_n.lower(), 999), -rareza.get(kit, 0), es_n.lower())

    cuchillos = sorted(a for a in por_arma if a.startswith("weapon_knife_") or a == "weapon_bayonet")
    out = ['"skins"', "{", '\t"armas"', "\t{"]
    total = 0
    for arma in sorted(por_arma, key=lambda a: (a in cuchillos, nombres.get(a, a).lower())):
        out += [f'\t\t"{arma}"', "\t\t{", f'\t\t\t"nombre"\t"{esc(nombres.get(arma, arma))}"', f'\t\t\t"indice"\t"{indices.get(arma, 0)}"', '\t\t\t"pintura"', "\t\t\t{"]
        lista = sorted(por_arma[arma], key=orden)
        repetidos = {n for n in (kits[k][1] for k in lista) if [kits[k][1] for k in lista].count(n) > 1}
        for kit in lista:
            pid, es_n, en_n = kits[kit]
            visible = es_n if not en_n or es_n == en_n else f"{es_n} ({en_n})"
            if es_n in repetidos:  # p.ej. las fases de Doppler comparten nombre
                visible += " - " + variante(kit)
            out.append(f'\t\t\t\t"{pid}"\t"{esc(visible)}"')
            total += 1
        out += ["\t\t\t}", "\t\t}"]
    out += ["\t}", '\t"cuchillos"', "\t{"]
    visibles = [nombres.get(c, c) for c in cuchillos]
    out += [f'\t\t"{c}"\t"{esc(v if visibles.count(v) == 1 else v + " (" + c[13:] + ")")}"' for c, v in zip(cuchillos, visibles)]
    out += ["\t}", "}"]
    Path(salida).write_text("\n".join(out) + "\n", encoding="utf-8")
    print(f"{len(por_arma)} armas, {len(cuchillos)} cuchillos, {total} skins -> {salida}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
