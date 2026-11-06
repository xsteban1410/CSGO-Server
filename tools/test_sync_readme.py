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
        write("README.md", "one\ntwo\nthree\nfour\n")
        write("README.es.md", "uno\ndos\ntres\ncuatro\n")
        git("add", ".")
        git("commit", "-qm", "base")

    def commit(self):
        git("add", ".")
        git("commit", "-qm", "change")

    def test_change_insert_and_delete(self):
        write("README.md", "ONE\ntwo\nnew\nthree\n")
        self.commit()
        self.assertEqual(sync_readme.sync("HEAD~1", translate=fake), "README.es.md")
        self.assertEqual(read("README.es.md"), "es:ONE\ndos\nes:new\ntres\n")

    def test_other_direction(self):
        write("README.es.md", "uno\nDOS\ntres\ncuatro\n")
        self.commit()
        self.assertEqual(sync_readme.sync("HEAD~1", translate=fake), "README.md")
        self.assertEqual(read("README.md"), "one\nen:DOS\nthree\nfour\n")

    def test_both_changed_does_nothing(self):
        write("README.md", "ONE\ntwo\nthree\nfour\n")
        write("README.es.md", "UNO\ndos\ntres\ncuatro\n")
        self.commit()
        self.assertIsNone(sync_readme.sync("HEAD~1", translate=fake))
        self.assertEqual(read("README.es.md"), "UNO\ndos\ntres\ncuatro\n")

    def test_misaligned_stops(self):
        write("README.es.md", "uno\n")
        self.commit()
        write("README.md", "ONE\ntwo\nthree\nfour\n")
        self.commit()
        with self.assertRaises(SystemExit):
            sync_readme.sync("HEAD~1", translate=fake)


if __name__ == "__main__":
    unittest.main()
