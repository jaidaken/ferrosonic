#!/usr/bin/env bash
#
# Parallel, scoped cargo-mutants runner. Scratch on disk (not /dev/shm, which
# caps at 32G and fits ~2 of the big per-worker build trees), workers sized to
# the cores, slow proptests skipped in each mutant's test phase.
#
# Usage:
#   scripts/mutants.sh                          # mutate the diff vs origin/master
#   scripts/mutants.sh --file src/daemon/x.rs   # mutate one file
#   scripts/mutants.sh --in-diff some.patch     # mutate a specific diff
#   scripts/mutants.sh -- -E 'binary(foo)'      # scope the test phase (nextest)
# Anything after `--` is passed to `cargo nextest run`. Any cargo-mutants flag
# passes straight through.
#
# Env knobs:
#   MUTANTS_JOBS        parallel workers            (default: nproc / build_jobs)
#   MUTANTS_BUILD_JOBS  build threads per worker    (default: 4)
#   MUTANTS_SCRATCH     scratch dir, NOT /dev/shm   (default: /tmp)
#   MUTANTS_BASE        diff base for default scope (default: origin/master)
#   MUTANTS_EXCLUDE     nextest filter for tests to skip (slow proptests/fuzz)
#
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

build_jobs="${MUTANTS_BUILD_JOBS:-4}"
# RAM tmpfs by default: disk scratch routes every rebuild through LUKS+btrfs
# CoW writeback, which saturates long before the CPU does.
scratch="${MUTANTS_SCRATCH:-/dev/shm}"
worker_gb="${MUTANTS_WORKER_GB:-7}"
# On a RAM tmpfs the binding limit is space, not cores: size workers to whichever
# is smaller.
jobs_cpu=$(( $(nproc) / build_jobs ))
scratch_gb=$(df -BG --output=avail "$scratch" 2>/dev/null | tail -1 | tr -dc 0-9 || echo 0)
jobs_space=$(( scratch_gb / worker_gb ))
jobs="${MUTANTS_JOBS:-$(( jobs_cpu < jobs_space ? jobs_cpu : jobs_space ))}"
(( jobs < 1 )) && jobs=1
base="${MUTANTS_BASE:-origin/master}"
exclude="${MUTANTS_EXCLUDE:-not (binary(stress_proptest) | binary(queue_proptest) | binary(ipc_frame_proptest) | binary(fuzz_ipc_frame) | binary(playback_state_proptest) | test(mpris_handler_off_tokio_runtime_does_not_panic))}"

args=("$@")
have_scope=false
have_tests=false
for a in ${args[@]+"${args[@]}"}; do
  case "$a" in
    --file | --file=* | --in-diff | --in-diff=*) have_scope=true ;;
    --) have_tests=true ;;
  esac
done

scope=()
if ! $have_scope; then
  diff_file="$(mktemp "${scratch%/}/mutants-XXXXXX.diff")"
  trap 'rm -f "$diff_file"' EXIT
  git diff "$base"...HEAD -- '*.rs' > "$diff_file"
  if [[ ! -s "$diff_file" ]]; then
    echo "no .rs diff vs $base; pass --file or --in-diff to scope explicitly" >&2
    exit 1
  fi
  echo "scope: diff vs $base ($(grep -c '^+' "$diff_file") added lines)"
  scope=(--in-diff "$diff_file")
fi

tail_args=()
# Cap nextest threads per worker too; without this each worker fans out to
# nproc test threads and JOBS workers oversubscribe the box (load >> cores).
$have_tests || tail_args=(-- --test-threads "$build_jobs" -E "$exclude")

echo "jobs=$jobs build_jobs=$build_jobs scratch=$scratch (${scratch_gb}G free)"
# debug=0 + no incremental shrinks each worker tree (smaller binaries, no
# incremental cache), so more workers fit in RAM and less gets written.
exec env TMPDIR="$scratch" \
  CARGO_BUILD_JOBS="$build_jobs" \
  CARGO_INCREMENTAL=0 \
  CARGO_PROFILE_DEV_DEBUG=0 \
  CARGO_PROFILE_TEST_DEBUG=0 \
  cargo mutants --jobs "$jobs" --test-tool nextest \
  ${scope[@]+"${scope[@]}"} ${args[@]+"${args[@]}"} ${tail_args[@]+"${tail_args[@]}"}
