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


def translate_text(text, translate):
    """Translates a piece of a line that has its protected parts as ⟦n⟧ markers, keeping its outer spaces."""
    body = text.strip()
    if not re.search(r"[^\W\d_]", re.sub(r"⟦\d+⟧", "", body)):
        return text
    start = len(text) - len(text.lstrip())
    return text[:start] + translate(body) + text[start + len(body):]


def translate_line(line, translate):
    prefix = PREFIX.match(line).group()
    kept = []

    def hide(match):
        kept.append(match.group())
        return f"⟦{len(kept) - 1}⟧"

    hidden = PROTECTED.sub(hide, line[len(prefix):])
    # table cells one by one, so the columns cannot get mixed up
    result = "|".join(translate_text(cell, translate) for cell in hidden.split("|"))
    found = re.findall(r"⟦(\d+)⟧", result)
    if sorted(map(int, found)) != list(range(len(kept))):
        raise ValueError(f"the translator damaged this line, so nothing was written: {line}")
    return prefix + re.sub(r"⟦(\d+)⟧", lambda m: kept[int(m.group(1))], result)


def translate_range(lines, start, count, translate):
    """Translates lines[start:start + count]; code blocks are copied as they are, comments included."""
    out = []
    fenced = False
    for i, line in enumerate(lines[: start + count]):
        fence = line.lstrip().startswith("```")
        if i >= start:
            out.append(line if fenced or fence else translate_line(line, translate))
        fenced ^= fence
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

    # from the bottom up, so the line numbers of the earlier changes stay valid
    for hunk in reversed(list(HUNK.finditer(git("diff", "-U0", base, head, "--", source)))):
        old_start, old_count, new_start, new_count = (int(g) if g else 1 for g in hunk.groups())
        at = old_start if old_count == 0 else old_start - 1
        first = new_start if new_count == 0 else new_start - 1
        lines[at : at + old_count] = translate_range(new, first, new_count, to_target)
    with open(target, "w", encoding="utf8") as f:
        f.write("\n".join(lines))
    print(f"Updated {target} from {source}.")
    return target


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3):
        sys.exit(__doc__)
    sync(*sys.argv[1:])
