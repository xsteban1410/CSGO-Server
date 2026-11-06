<!--
Thanks for contributing! Please read CONTRIBUTING.md first, especially "What you must not change" and "Sensitive areas":
https://github.com/xsteban1410/CSGO-Server/blob/master/CONTRIBUTING.md
Open it against pre-releases (CI does that for you if you forget). Do not change the version number. Fill in every section; delete nothing.
-->

## What and why

<!-- What does this change, and which problem does it solve? -->

Closes #

## Type of change

- [ ] Bug fix
- [ ] New game mode
- [ ] New feature
- [ ] Translation or documentation
- [ ] Other:

## How it was tested

<!-- Commands you ran and what you saw. Say what you could not test, such as joining with a real client. -->

## Sensitive areas

<!-- See the table in CONTRIBUTING.md. Sensitive paths get a label and a manual review. -->

- [ ] This does not touch any sensitive area
- [ ] It does, and this is why it is needed and how I tested it:

## Checklist

- [ ] I read [CONTRIBUTING.md](https://github.com/xsteban1410/CSGO-Server/blob/master/CONTRIBUTING.md), and this does none of what it forbids
- [ ] `cargo test` and `cargo clippy --all-targets` pass
- [ ] Plugins compile, if I changed `plugin/`
- [ ] `tools/test-modes.sh` passes for the affected modes, if I changed `cfg/`, `modes/` or `plugin/`
- [ ] New app text has its Spanish translation in `crates/gtk/src/i18n.rs`
- [ ] Both READMEs are updated if behavior changed, and `CHANGELOG.md` has a line under `Unreleased`
- [ ] No passwords, tokens, `config.toml`, `server/` or build output are included
