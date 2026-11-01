#!/usr/bin/env bash
# cobalt fork: merges upstream PhotoCraft (storytold/photocraft main) into the `cobalt` branch,
# with Claude Code doing the merge and the fixes by the cobalt-fork skill
# (.claude/skills/cobalt-fork/SKILL.md), then checks the result.
#
#   scripts/cobalt-update.sh               # headless; leaves the merge committed locally
#   scripts/cobalt-update.sh --push        # and pushes `cobalt` to origin when every check passes
#   scripts/cobalt-update.sh --install     # and reinstalls PhotoCraft - Cobalt (scripts/cobalt-install.sh)
#   scripts/cobalt-update.sh --interactive # opens Claude Code in the terminal instead
#   scripts/cobalt-update.sh -- --model opus   # anything after -- goes to `claude`
#
# Needs: `claude` on PATH, the remotes `upstream` (storytold/photocraft) and `origin` (the fork),
# and the Cobalt app checked out at ../cobalt (the fork builds its effects
# from there). The log of a headless run goes to target/cobalt-update-<time>.log.
set -euo pipefail

push=false
install=false
interactive=false
claude_args=()
while [ $# -gt 0 ]; do
    case "$1" in
        --push) push=true ;;
        --install) install=true ;;
        --interactive|-i) interactive=true ;;
        --) shift; claude_args=("$@"); break ;;
        -h|--help) awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"; exit 0 ;;
        *) echo "unknown option $1 (see --help)" >&2; exit 2 ;;
    esac
    shift
done

root=$(git rev-parse --show-toplevel)
cd "$root"
branch=cobalt
app=$(cd .. && pwd)/cobalt

die() { echo "cobalt-update: $*" >&2; exit 1; }
[ "$(git branch --show-current)" = "$branch" ] || die "check out the $branch branch first"
git diff --quiet && git diff --cached --quiet || die "the working tree has uncommitted changes"
[ -z "$(git ls-files --others --exclude-standard)" ] || die "the working tree has untracked files"
[ ! -e "$(git rev-parse --git-path MERGE_HEAD)" ] || die "a merge is already in progress"
[ -f "$app/crates/cobalt-core/Cargo.toml" ] || die "the Cobalt app is not at $app"
git remote get-url upstream >/dev/null 2>&1 || die "no 'upstream' remote (git remote add upstream https://github.com/storytold/photocraft.git)"
command -v claude >/dev/null || die "claude (Claude Code) is not on PATH"

# Reuse conflict resolutions from earlier merges
git config rerere.enabled true
git config rerere.autoupdate true

git fetch upstream
behind=$(git rev-list --count HEAD..upstream/main)
if [ "$behind" = 0 ]; then
    echo "cobalt-update: already up to date with upstream/main"
    if $install; then exec scripts/cobalt-install.sh; fi
    exit 0
fi
start=$(git rev-parse HEAD)
echo "cobalt-update: $behind upstream commits to merge into $branch (now at ${start:0:9})"

prompt="Use the cobalt-fork skill: merge upstream/main ($(git rev-parse --short upstream/main), $behind new commits) into the cobalt branch, following its 'Updating from upstream' steps. git fetch has already run and rerere is on. Resolve conflicts, fix anything upstream broke in the fork-only files, run every check in step 5 until it passes, and commit. Do not push. End with a short report: conflicts and how you resolved them, what you changed, the check results, and anything left for a human."

# Edits are allowed; shell access is limited to what an update needs, and never a push.
allowed=(Read Edit Write Glob Grep
    "Bash(git:*)" "Bash(cargo:*)" "Bash(rustfmt:*)" "Bash(python3:*)"
    "Bash(.claude/skills/cobalt-fork/compare.py:*)" "Bash(ls:*)" "Bash(cat:*)" "Bash(head:*)"
    "Bash(tail:*)" "Bash(grep:*)" "Bash(sed -n:*)" "Bash(wc:*)" "Bash(find:*)")
denied=("Bash(git push:*)" "Bash(git reset --hard:*)" "Bash(git checkout main:*)" "Bash(git branch -D:*)")

if $interactive; then
    # The prompt goes first: the tool lists are variadic and would take it as a tool
    claude "$prompt" --add-dir "$app" --allowedTools "${allowed[@]}" --disallowedTools "${denied[@]}" "${claude_args[@]}"
else
    mkdir -p target
    log="target/cobalt-update-$(date +%Y%m%d-%H%M%S).log"
    echo "cobalt-update: running Claude Code headless (log: $log)"
    claude -p "$prompt" --permission-mode acceptEdits --add-dir "$app" \
        --allowedTools "${allowed[@]}" --disallowedTools "${denied[@]}" "${claude_args[@]}" | tee "$log"
fi

# Trust, but verify
[ ! -e "$(git rev-parse --git-path MERGE_HEAD)" ] || die "the merge is still in progress: finish it, or run git merge --abort"
git merge-base --is-ancestor upstream/main HEAD || die "upstream/main is not merged into $branch"
git diff --quiet && git diff --cached --quiet || die "uncommitted changes are left over"
echo "cobalt-update: checking the result"
cargo test -p photocraft-engine --lib cobalt_cmds
cargo test -p photocraft-ui-egui --lib cobalt_ui
echo "cobalt-update: hooks in upstream files:"
git grep -n cobalt -- crates ':!crates/engine/src/cmd/cobalt_cmds.rs' ':!crates/ui-egui/src/dialogs/cobalt_ui.rs' || true

echo "cobalt-update: merged ${start:0:9}..$(git rev-parse --short HEAD)"
if $push; then
    git push origin "$branch"
else
    echo "cobalt-update: not pushed (git push origin $branch, or run with --push)"
fi
if $install; then
    scripts/cobalt-install.sh
fi
