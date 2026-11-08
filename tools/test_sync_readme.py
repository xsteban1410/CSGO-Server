#!/usr/bin/env python3
"""Tests tools/sync_readme.py against a temporary repository and a fake translator."""
import os
import subprocess
import tempfile
import unittest

import sync_readme


def fake(text, source, target):
    return f"{target}:{text}"


def git(*args):
    subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@t", *args], check=True, capture_output=True)


def write(name, text):
    with open(name, "w", encoding="utf8") as f:
        f.write(text)


def read(name):
    with open(name, encoding="utf8") as f:
        return f.read()


class Lines(unittest.TestCase):
    def line(self, text):
        return sync_readme.translate_line(text, lambda t: f"X{t}")

    def test_keeps_markup(self):
        self.assertEqual(self.line("- **Run** `a b` and [docs](x.md)"), "- X⟦0⟧".replace("⟦0⟧", "**Run** `a b` and [docs](x.md)"))
        self.assertEqual(self.line("## Title"), "## XTitle")

    def test_table_cells(self):
        self.assertEqual(self.line("| One | `a|b` two |"), "| XOne | X`a|b` two |")
        self.assertEqual(self.line("|---|---|"), "|---|---|")

    def test_damaged_marker_fails(self):
        with self.assertRaises(ValueError):
            sync_readme.translate_line("run `a` now", lambda t: "gone")

    def test_paragraph_is_translated_whole(self):
        lines = ["- A sentence that goes", "  over two lines.", "", "Next one."]
        seen = []

        def spy(t):
            seen.append(t)
            return t.upper()

        out = sync_readme.translate_range(lines, 0, 4, spy)
        self.assertEqual(seen, ["A sentence that goes over two lines.", "Next one."])
        self.assertEqual(out, ["- A SENTENCE THAT GOES", "  OVER TWO LINES.", "", "NEXT ONE."])

    def test_glossary_and_tu(self):
        es = {"Install ⟦0⟧ in ⟦1⟧": "Instale ⟦0⟧ en ⟦1⟧", "⟦0⟧ add ⟦1⟧": "⟦0⟧ añaden ⟦1⟧"}
        self.assertEqual(sync_readme.translate_line("Install hub items in the Hub tab", es.get), "Instala items del hub en la pestaña Hub")
        self.assertEqual(sync_readme.translate_line("Open it", lambda t: "Selecciónelo en Sucursales"), "Elígelo en Ramas")
        self.assertEqual(sync_readme.translate_line("From the app's Hub tab", lambda t: "Desde el ⟦0⟧ de la app"), "Desde la pestaña Hub de la app")
        self.assertEqual(sync_readme.translate_line("Plugins add skins", lambda t: "Los complementos añaden máscaras"), "Los plugins añaden skins")
        en = {"⟦0⟧ y plugins": "⟦0⟧ and plugins"}
        self.assertEqual(sync_readme.translate_line("items del hub y plugins", en.get, "es", "en"), "hub items and plugins")

    def test_code_blocks_are_copied(self):
        lines = ["a", "```", "code", "```", "b"]
        self.assertEqual(sync_readme.translate_range(lines, 0, 5, lambda t: f"X{t}"), ["Xa", "```", "code", "```", "Xb"])


class Sync(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(self.dir.name)
        git("init", "-q")
        write("README.md", "one\n\ntwo\n\nthree\n\nfour\n")
        write("README.es.md", "uno\n\ndos\n\ntres\n\ncuatro\n")
        git("add", ".")
        git("commit", "-qm", "base")

    def commit(self):
        git("add", ".")
        git("commit", "-qm", "change")

    def test_change_insert_and_delete(self):
        write("README.md", "ONE\n\ntwo\n\nnew\n\nthree\n")
        self.commit()
        self.assertEqual(sync_readme.sync("HEAD~1", translate=fake), "README.es.md")
        self.assertEqual(read("README.es.md"), "es:ONE\n\ndos\n\nes:new\n\nes:three\n")

    def test_a_changed_line_takes_its_paragraph(self):
        write("README.md", "one\n\ntwo\nTHREE\n\nfour\n")
        write("README.es.md", "uno\n\ndos\ntres\n\ncuatro\n")
        self.commit()
        write("README.md", "one\n\ntwo\nTHREE MORE\n\nfour\n")
        self.commit()
        sync_readme.sync("HEAD~1", translate=fake)
        self.assertEqual(read("README.es.md"), "uno\n\nes:two\nTHREE MORE\n\ncuatro\n")

    def test_other_direction(self):
        write("README.es.md", "uno\n\nDOS\n\ntres\n\ncuatro\n")
        self.commit()
        self.assertEqual(sync_readme.sync("HEAD~1", translate=fake), "README.md")
        self.assertEqual(read("README.md"), "one\n\nen:DOS\n\nthree\n\nfour\n")

    def test_both_changed_does_nothing(self):
        write("README.md", "ONE\n\ntwo\n\nthree\n\nfour\n")
        write("README.es.md", "UNO\n\ndos\n\ntres\n\ncuatro\n")
        self.commit()
        self.assertIsNone(sync_readme.sync("HEAD~1", translate=fake))
        self.assertEqual(read("README.es.md"), "UNO\n\ndos\n\ntres\n\ncuatro\n")

    def test_misaligned_stops(self):
        write("README.es.md", "uno\n")
        self.commit()
        write("README.md", "ONE\n\ntwo\n\nthree\n\nfour\n")
        self.commit()
        with self.assertRaises(SystemExit):
            sync_readme.sync("HEAD~1", translate=fake)


if __name__ == "__main__":
    unittest.main()
