#!/usr/bin/env python3
"""Carries a change in one README over to the other, translated by Google Translate.

Usage: tools/sync_readme.py BASE [HEAD]   (run from the repository; writes the other README, commits nothing)

Both READMEs have the same lines in the same order, so only the changed lines are translated and the rest of the
translation stays as it is. Nothing happens if neither or both changed between BASE and HEAD.
"""
import json
import re
import subprocess
import sys
import time
import urllib.parse
import urllib.request

LANGS = {"README.md": "en", "README.es.md": "es"}
# Never sent to the translator: inline code, link targets, bare URLs, HTML tags and bold markers.
PROTECTED = re.compile(r"`[^`]*`|\]\([^)]*\)|https?://[^\s)]+|<[^>]+>|\*\*")
PREFIX = re.compile(r"\s*(?:(?:#{1,6}|[-*+]|\d+\.|>)\s+)*")
HUNK = re.compile(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", re.M)


def google(text, source, target):
    # shortcut: unofficial endpoint, free and without a key; if Google blocks it the job fails and nothing is
    # committed. To replace it, use the Cloud Translation API with a key.
    data = urllib.parse.urlencode({"client": "gtx", "sl": source, "tl": target, "dt": "t", "q": text}).encode()
    request = urllib.request.Request(
        "https://translate.googleapis.com/translate_a/single", data=data, headers={"User-Agent": "Mozilla/5.0"}
    )
    for attempt in range(3):
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                return "".join(part[0] for part in json.load(response)[0] if part[0])
        except OSError:
            if attempt == 2:
                raise
            time.sleep(5)
    raise AssertionError("unreachable")


# Phrases whose word order or gender changes, hidden from the translator and put back in the other language:
# (English, Spanish). Single words would leave the translator guessing their gender, so those go in ES_TERMS.
# shortcut: fixed lists; add an entry when the translator gets a term of this project wrong.
GLOSSARY = [
    ("the Hub tab", "la pestaña Hub"),
    # with its article, which replaces the one the translator writes ("the app's Hub tab")
    ("Hub tab", "la pestaña Hub"),
    ("hub items", "items del hub"),
    ("hub item", "item del hub"),
    ("the skins plugin", "el plugin de skins"),
    ("skins plugin", "plugin de skins"),
    ("admin tools", "herramientas de admin"),
    ("from the hub", "del hub"),
]
# Words the translator puts in Spanish that the READMEs keep in English (or say otherwise), fixed in its answer.
ES_TERMS = {
    "complementos": "plugins", "complemento": "plugin", "máscaras": "skins", "máscara": "skin",
    "antitrampas": "anticheat", "administradores": "admins", "administrador": "admin", "robots": "bots",
    "elementos centrales": "items del hub", "elemento central": "item del hub", "elementos": "items", "elemento": "item",
    "ajustes preestablecidos": "presets", "lanzamientos": "releases", "lanzamiento": "release", "prohibir": "banear",
    "las prohibiciones": "los baneos", "prohibiciones": "baneos", "patadas": "expulsiones", "patada": "expulsión",
    "saltos de conejo": "bunny hops", "sucursales": "ramas", "sucursal": "rama",
    "la aplicación": "la app", "aplicación": "app", "línea de comando": "terminal", "línea de comandos": "terminal",
}
# Google answers in formal Spanish (usted); the Spanish README speaks to the reader as tú.
# shortcut: only the imperatives a README uses; add one when a formal form slips through.
TU = {
    "usted": "tú", "descargue": "descarga", "ejecute": "ejecuta", "elija": "elige", "copie": "copia",
    "coloque": "pon", "use": "usa", "utilice": "usa", "abra": "abre", "pulse": "pulsa", "presione": "pulsa",
    "escriba": "escribe", "instale": "instala", "cierre": "cierra", "lea": "lee", "consulte": "mira",
    "agregue": "añade", "añada": "añade", "cree": "crea", "vea": "mira", "apunte": "apunta", "active": "activa",
    "selecciónelos": "elígelos", "selecciónelo": "elígelo", "selecciónelas": "elígelas",
}
# a line that goes on with the sentence of the previous one
CONTINUES = re.compile(r"(?!\s*(?:[|#<]|```|[-*+]\s|\d+\.\s))\s*\S")


def is_text(line):
    """A line of running text: not empty, a table, a heading or a fence."""
    return bool(line.strip()) and not re.match(r"\s*(?:[|#]|```)", line)


def hide_glossary(text, source, target, kept):
    if source == target:
        return text
    pairs = [(en, es) if source == "en" else (es, en) for en, es in GLOSSARY]

    def hide(match):
        word = match.group()
        frm, out = next(p for p in pairs if p[0].lower() == word.lower())
        # capital only where the sentence starts with it
        kept.append(out[0].upper() + out[1:] if word[0].isupper() and frm[0].islower() else out)
        return f"⟦{len(kept) - 1}⟧"

    words = "|".join(re.escape(frm) for frm, _ in sorted(pairs, key=lambda p: -len(p[0])))
    # outside the parts already hidden
    return re.sub(rf"(?<![\w⟦])(?:{words})(?![\w⟧])", hide, text, flags=re.I)


def to_project_spanish(text):
    """Speaks as tú and uses this project's terms, keeping a capital where the translator put one."""
    words = {**TU, **ES_TERMS}

    def fix(match):
        word = match.group()
        out = words[word.lower()]
        return out[0].upper() + out[1:] if word[0].isupper() else out

    alternatives = "|".join(re.escape(w) for w in sorted(words, key=len, reverse=True))
    return re.sub(rf"(?<!\w)(?:{alternatives})(?!\w)", fix, text, flags=re.I)


def put_back(text, kept):
    """Puts the hidden parts back. A glossary phrase brings its own article, so the one the translator put
    before it goes (\"desde el ⟦0⟧\" with \"la pestaña Hub\" gives \"desde la pestaña Hub\")."""

    def article(match):
        value = kept[int(match.group(2))]
        if not re.match(r"(el|la|los|las) ", value):
            return match.group()
        return {"del": "de ", "de la": "de ", "al": "a ", "a la": "a "}.get(match.group(1).lower(), "") + value

    text = re.sub(r"(?<!\w)(del|de la|al|a la|el|la|los|las) ⟦(\d+)⟧", article, text, flags=re.I)
    return re.sub(r"⟦(\d+)⟧", lambda m: kept[int(m.group(1))], text)


def translate_text(text, translate):
    """Translates a piece of a line that has its protected parts as ⟦n⟧ markers, keeping its outer spaces."""
    body = text.strip()
    if not re.search(r"[^\W\d_]", re.sub(r"⟦\d+⟧", "", body)):
        return text
    start = len(text) - len(text.lstrip())
    return text[:start] + translate(body) + text[start + len(body):]


def split_like(text, lines):
    """Splits text into as many lines as `lines`, in proportion to their lengths, never inside a ⟦n⟧ marker."""
    words = text.split()
    total = sum(len(line) for line in lines) or 1
    out, taken, done = [], 0, 0
    for i, line in enumerate(lines[:-1]):
        taken += len(line)
        goal = len(" ".join(words)) * taken / total
        part = []
        # leave at least one word for each line still to come
        while len(words) - done - len(part) > len(lines) - 1 - i:
            more = part + [words[done + len(part)]]
            # take the next word while it brings the line closer to its share
            if part and abs(done_len(out, more) - goal) >= abs(done_len(out, part) - goal):
                break
            part = more
        done += len(part)
        out.append(" ".join(part))
    out.append(" ".join(words[done:]))
    return out


def done_len(out, part):
    return sum(len(o) + 1 for o in out) + len(" ".join(part))


def translate_paragraph(lines, translate, source="en", target="es"):
    """Translates lines that form one paragraph as a whole, then splits it back into as many lines, so a
    sentence that goes over a line break is not cut in two and both READMEs keep the same lines."""
    prefix = PREFIX.match(lines[0]).group()
    indents = [re.match(r"\s*", line).group() for line in lines[1:]]
    kept = []

    def hide(match):
        kept.append(match.group())
        return f"⟦{len(kept) - 1}⟧"

    text = " ".join([lines[0][len(prefix):].strip()] + [line.strip() for line in lines[1:]])
    hidden = hide_glossary(PROTECTED.sub(hide, text), source, target, kept)
    result = translate_text(hidden, translate)
    if target == "es":
        result = to_project_spanish(result)
    found = re.findall(r"⟦(\d+)⟧", result)
    if sorted(map(int, found)) != list(range(len(kept))):
        raise ValueError(f"the translator damaged this text, so nothing was written: {text}")
    sizes = [len(lines[0]) - len(prefix)] + [len(line.strip()) for line in lines[1:]]
    parts = split_like(result, ["x" * n for n in sizes])
    return [prefix + put_back(parts[0], kept)] + [ind + put_back(p, kept) for ind, p in zip(indents, parts[1:])]


def translate_line(line, translate, source="en", target="es"):
    if not is_text(line) and line.strip().startswith("|"):
        prefix = PREFIX.match(line).group()
        kept = []

        def hide(match):
            kept.append(match.group())
            return f"⟦{len(kept) - 1}⟧"

        hidden = hide_glossary(PROTECTED.sub(hide, line[len(prefix):]), source, target, kept)
        # table cells one by one, so the columns cannot get mixed up
        result = "|".join(translate_text(cell, translate) for cell in hidden.split("|"))
        if target == "es":
            result = to_project_spanish(result)
        found = re.findall(r"⟦(\d+)⟧", result)
        if sorted(map(int, found)) != list(range(len(kept))):
            raise ValueError(f"the translator damaged this line, so nothing was written: {line}")
        return prefix + put_back(result, kept)
    return translate_paragraph([line], translate, source, target)[0]


def paragraph_bounds(lines, start, end):
    """Widens lines[start:end] to whole paragraphs, so a changed line is translated with its whole sentence."""
    while start > 0 and start < len(lines) and CONTINUES.match(lines[start]) and is_text(lines[start - 1]) \
            and is_text(lines[start]):
        start -= 1
    while end < len(lines) and end > 0 and is_text(lines[end - 1]) and is_text(lines[end]) \
            and CONTINUES.match(lines[end]):
        end += 1
    return start, end


def translate_range(lines, start, count, translate, source="en", target="es"):
    """Translates lines[start:start + count] a paragraph at a time; code blocks are copied as they are."""
    fenced = False
    for line in lines[:start]:
        fenced ^= line.lstrip().startswith("```")
    out = []
    i, end = start, start + count
    while i < end:
        line = lines[i]
        fence = line.lstrip().startswith("```")
        if fenced or fence or not line.strip():
            out.append(line)
            fenced ^= fence
            i += 1
            continue
        if not is_text(line):
            out.append(translate_line(line, translate, source, target))
            i += 1
            continue
        j = i + 1
        while j < end and is_text(lines[j]) and CONTINUES.match(lines[j]):
            j += 1
        out += translate_paragraph(lines[i:j], translate, source, target)
        i = j
    return out


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def sync(base, head="HEAD", translate=google):
    changed = [f for f in git("diff", "--name-only", base, head, "--", *LANGS).split() if f in LANGS]
    if len(changed) != 1:
        print(f"Nothing to translate: {len(changed)} READMEs changed.")
        return None
    source = changed[0]
    target = next(f for f in LANGS if f != source)
    old = git("show", f"{base}:{source}").split("\n")
    new = git("show", f"{head}:{source}").split("\n")
    with open(target, encoding="utf8") as f:
        lines = f.read().split("\n")
    if len(old) != len(lines):
        sys.exit(f"{source} had {len(old)} lines and {target} has {len(lines)}: line them up by hand first.")

    def to_target(text):
        return translate(text, LANGS[source], LANGS[target])

    # (old start, old end, new start, new end) of each change, widened to whole paragraphs and merged
    spans = []
    for hunk in HUNK.finditer(git("diff", "-U0", base, head, "--", source)):
        old_start, old_count, new_start, new_count = (int(g) if g else 1 for g in hunk.groups())
        at = old_start if old_count == 0 else old_start - 1
        first = new_start if new_count == 0 else new_start - 1
        # the lines around a change are the same in both versions, so both ends widen alike
        wide_start, wide_end = paragraph_bounds(new, first, first + new_count)
        span = [at - (first - wide_start), at + old_count + (wide_end - first - new_count), wide_start, wide_end]
        if spans and span[2] <= spans[-1][3]:
            spans[-1][1], spans[-1][3] = span[1], span[3]
        else:
            spans.append(span)
    # from the bottom up, so the line numbers of the earlier changes stay valid
    for at, at_end, first, first_end in reversed(spans):
        lines[at:at_end] = translate_range(new, first, first_end - first, to_target, LANGS[source], LANGS[target])
    with open(target, "w", encoding="utf8") as f:
        f.write("\n".join(lines))
    print(f"Updated {target} from {source}.")
    return target


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3):
        sys.exit(__doc__)
    sync(*sys.argv[1:])
