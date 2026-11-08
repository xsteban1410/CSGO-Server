# Git

- Make one commit per change (a fix, a feature, a translation), so the history shows what each version got.
- Format: `type(scope): what changed`, in English. Example: `fix(skins): recompile plugins when sources change`.
  Types: `feat`, `fix`, `i18n`, `refactor`, `test`, `docs`, `ci`, `chore`. Scope is the area touched: `skins`, `modes`, `deploy`, `app`...
- Push when a task is finished, not after every commit; CI tells you if something failed, and the fix is another commit.
- `Co-Authored-By` goes only on the last commit of the task, the one pushed; commits in between do not carry it.
- Never force-push or rewrite history.
- Work from `pre-releases` and open pull requests against it. It reaches `master` by itself after one quiet day, or at once with an `approve release` issue from the owner (see `CONTRIBUTING.md#branches`).
- Keep each pull request to one topic, and fill in the template, including what could not be tested.
- Do not change the version number or publish a release: the maintainer does that.
