#!/usr/bin/env bash
# Tests tools/promote.sh against a local repository and a fake gh.
set -euo pipefail
script=$(realpath "$(dirname "$0")/promote.sh")
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
mkdir "$tmp/bin"
cat > "$tmp/bin/gh" <<'GH'
#!/usr/bin/env bash
echo "$*" >> "$GH_LOG"
case "$1 $2" in
    "run list") echo "$FAKE_CI" ;;
    "issue list") echo "$FAKE_ISSUES" ;;
esac
GH
chmod +x "$tmp/bin/gh"
export PATH="$tmp/bin:$PATH" GH_LOG="$tmp/gh.log" PROMOTE_APPROVERS=owner
later=$(( $(date +%s) + 2 * 86400 ))

setup() {
    rm -rf "$tmp/origin.git" "$tmp/w"; : > "$GH_LOG"
    git init -q --bare -b master "$tmp/origin.git"
    git clone -q "$tmp/origin.git" "$tmp/w" 2>/dev/null
    git -C "$tmp/w" switch -q -c master
    echo 1 > "$tmp/w/a"; git -C "$tmp/w" add a; git -C "$tmp/w" commit -qm base
    git -C "$tmp/w" push -q origin master master:pre-releases
    FAKE_CI=success FAKE_ISSUES='[]'
    export FAKE_CI FAKE_ISSUES
}
commit_on() { # branch file
    git -C "$tmp/w" fetch -q origin
    git -C "$tmp/w" switch -q --detach "origin/$1"
    echo "$1" >> "$tmp/w/$2"; git -C "$tmp/w" add .; git -C "$tmp/w" commit -qm "$2"
    git -C "$tmp/w" push -q origin "HEAD:$1"
}
run() { (cd "$tmp/w" && PROMOTE_NOW=${1:-$later} bash "$script") >/dev/null; }
ref() { git -C "$tmp/origin.git" rev-parse "$1"; }
fail() { echo "FAILED: $1"; exit 1; }

setup; before=$(ref master); run; [[ $(ref master) == "$before" ]] || fail "nothing to promote"
setup; commit_on master m; run; [[ $(ref pre-releases) == $(ref master) ]] || fail "keeps pre-releases equal to master"

setup; commit_on pre-releases x; before=$(ref master)
run "$(date +%s)"; [[ $(ref master) == "$before" ]] || fail "too recent"
FAKE_CI=failure; run; [[ $(ref master) == "$before" ]] || fail "failed CI"
FAKE_CI=success
FAKE_ISSUES='[{"number":1,"createdAt":"2000-01-01T00:00:00Z","labels":[{"name":"blocks-release"}]}]'
run; [[ $(ref master) == "$before" ]] || fail "blocks-release issue"
FAKE_ISSUES='[{"number":2,"createdAt":"2999-01-01T00:00:00Z","labels":[{"name":"bug"}]}]'
run; [[ $(ref master) == "$before" ]] || fail "new bug"
FAKE_ISSUES='[{"number":3,"createdAt":"2000-01-01T00:00:00Z","labels":[{"name":"bug"}]},{"number":4,"createdAt":"2999-01-01T00:00:00Z","labels":[{"name":"enhancement"}]}]'
run; [[ $(ref master) == $(ref pre-releases) ]] || fail "old bug and new request do not block"
grep -q 'workflow run release.yml --ref master' "$GH_LOG" || fail "starts the release"

approve() { # author, then extra issues
    FAKE_ISSUES="[{\"number\":9,\"title\":\" Approve Release \",\"author\":{\"login\":\"$1\"},\"createdAt\":\"2999-01-01T00:00:00Z\",\"labels\":[]}${2:+,$2}]"
}
setup; commit_on pre-releases x; before=$(ref master); now=$(date +%s)
approve stranger; run "$now"; [[ $(ref master) == "$before" ]] || fail "approval by someone else does not count"
approve owner '{"number":1,"createdAt":"2000-01-01T00:00:00Z","labels":[{"name":"blocks-release"}]}'
run "$now"; [[ $(ref master) == "$before" ]] || fail "blocks-release wins over approval"
FAKE_CI=failure; approve owner; run "$now"; [[ $(ref master) == "$before" ]] || fail "approval still needs CI"
FAKE_CI=success; approve owner '{"number":2,"createdAt":"2999-01-01T00:00:00Z","labels":[{"name":"bug"}]}'
run "$now"; [[ $(ref master) == $(ref pre-releases) ]] || fail "approval skips the wait and new bugs"
grep -q 'issue close 9' "$GH_LOG" || fail "closes the approval issue"

setup; commit_on pre-releases x; commit_on master y; run
[[ $(ref master) == $(ref pre-releases) ]] || fail "diverged branches end up equal"
git -C "$tmp/origin.git" merge-base --is-ancestor "$(git -C "$tmp/origin.git" rev-parse master~1)" master || fail "merge commit"

setup; commit_on pre-releases c; commit_on master c; before=$(ref master)
if run; then fail "conflict must fail"; fi
[[ $(ref master) == "$before" ]] || fail "conflict leaves master alone"
grep -q 'issue create --label blocks-release' "$GH_LOG" || fail "conflict opens an issue"
echo "promote: all cases passed"
