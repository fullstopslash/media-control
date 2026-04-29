

# === universal API (see ~/projects/just-spec) ===

# Build the default nix-flake package. On malphas, the
# post-build-hook auto-pushes the result to attic — so this
# is the cache-warm path called by `verify` before `commit`.
build:
    nix build .#default --print-build-logs

# Lint stub. Replace with project-specific linters/formatters.
lint:
    @echo "no linter configured"

# Test stub. Replace with project-specific tests.
test:
    @echo "no tests configured"

# Pre-push gate — runs lint + test + build (the build step
# warms the attic cache on malphas via post-build-hook).
verify: lint test build

# Bump version in Cargo.toml. patch | minor | major.
bump level="patch":
    cargo set-version --bump {{level}}

# End-to-end: verify, bump patch, jj describe, push to every remote.
# Embed `--no-verify` in the message to skip the verify step.
commit +message:
    #!/usr/bin/env bash
    set -euo pipefail
    MESSAGE={{ quote(message) }}
    SKIP_VERIFY=0
    case " $MESSAGE " in
        *" --no-verify "*)
            SKIP_VERIFY=1
            MESSAGE="${MESSAGE/ --no-verify/}"
            MESSAGE="${MESSAGE/--no-verify /}"
            MESSAGE="${MESSAGE/--no-verify/}"
            ;;
    esac
    [ -n "$MESSAGE" ] || { echo "error: commit message required" >&2; exit 1; }
    [ "$SKIP_VERIFY" = 1 ] || just verify
    just bump patch
    jj describe -m "$MESSAGE"
    BOOKMARK=$(jj log -r '@ | @-' --no-graph -T 'bookmarks ++ "\n"' 2>/dev/null \
        | tr ' ' '\n' | grep -v '@' | grep -v '^$' | sed 's/[*?]\+$//' | head -1)
    if [ -z "$BOOKMARK" ]; then
        for n in trunk main master; do
            if jj bookmark list "$n" 2>/dev/null | grep -q "^$n:"; then
                BOOKMARK="$n"; break
            fi
        done
    fi
    [ -n "$BOOKMARK" ] || { echo "error: no main/trunk/master bookmark found" >&2; exit 1; }
    jj bookmark set "$BOOKMARK" -r @
    REMOTES=$(jj git remote list | awk '{print $1}')
    [ -n "$REMOTES" ] || { echo "warn: no remotes configured, skipping push" >&2; exit 0; }
    for remote in $REMOTES; do
        echo "» push → $remote"
        jj git push --remote "$remote" --bookmark "$BOOKMARK" --allow-new \
            || echo "warn: push to $remote failed"
    done
    jj new

