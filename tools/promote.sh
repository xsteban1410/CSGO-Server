#!/usr/bin/env bash
# Merges pre-releases into master once it has been quiet for PROMOTE_DAYS (run daily by .github/workflows/promote.yml).
# It waits if the newest commit is too recent, CI on it did not succeed, an issue labelled blocks-release is open,
# or a bug or connection issue was opened since the first commit not yet in master.
# shortcut: an issue cannot be matched to a change automatically, so any new bug report holds the promotion;
# the maintainer closes it or removes its label if it is unrelated.
set -euo pipefail
from=origin/pre-releases
to=origin/master
days=${PROMOTE_DAYS:-3}
now=${PROMOTE_NOW:-$(date +%s)}

wait_more() { echo "Not promoting: $1"; exit 0; }
give_up() {
    gh issue create --label blocks-release --title "Promotion to master failed" --body "$1

Merge by hand: \`git switch master && git pull && git merge origin/pre-releases && git push\`. Close this issue afterwards: automatic promotion stays on hold while it is open."
    exit 1
}

git fetch -q origin
if [[ $(git rev-list --count "$to..$from") -eq 0 ]]; then
    # nothing to promote: keep pre-releases equal to master
    if [[ $(git rev-list --count "$from..$to") -gt 0 ]]; then
        git push -q origin "$to:refs/heads/pre-releases" || echo "::warning::Could not update pre-releases: run git push origin master:pre-releases"
    fi
    wait_more "pre-releases has nothing new"
fi

sha=$(git rev-parse "$from")
age=$(( now - $(git log -1 --format=%ct "$from") ))
[[ $age -ge $(( days * 86400 )) ]] || wait_more "the newest commit is only $(( age / 3600 )) hours old"

ci=$(gh run list --workflow ci.yml --branch pre-releases --commit "$sha" --limit 1 --json conclusion --jq '.[0].conclusion // ""')
[[ $ci == success ]] || wait_more "CI on $sha is '${ci:-missing}'"

since=$(date -u -d "@$(git log --format=%ct "$to..$from" | tail -1)" +%FT%TZ)
blockers=$(gh issue list --state open --limit 200 --json number,createdAt,labels | jq -r --arg since "$since" '
    .[] | ([.labels[].name]) as $l
    | select(($l | index("blocks-release")) or (.createdAt >= $since and (($l | index("bug")) or ($l | index("connection")))))
    | "#\(.number)"' | tr '\n' ' ')
[[ -z $blockers ]] || wait_more "open issues: $blockers"

git switch -q --detach "$to"
if ! git -c user.name='github-actions[bot]' -c user.email='41898282+github-actions[bot]@users.noreply.github.com' \
    merge -q --no-edit -m "Promote pre-releases to master" "$from"; then
    git merge --abort
    give_up "pre-releases conflicts with master."
fi
# GITHUB_TOKEN cannot push changes to workflow files, and those stop here
git push -q origin HEAD:master || give_up "The push to master was rejected. It usually means pre-releases changes \`.github/workflows\`, which the workflow token cannot push."
[[ $(git rev-parse HEAD) == "$sha" ]] || git push -q origin HEAD:pre-releases || echo "::warning::Could not update pre-releases"
# pushes made with GITHUB_TOKEN trigger no workflows: start them by hand. Release does nothing without a new version.
gh workflow run ci.yml --ref master
gh workflow run release.yml --ref master
echo "Promoted $sha to master."
