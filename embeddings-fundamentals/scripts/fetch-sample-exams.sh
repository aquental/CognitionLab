#!/usr/bin/env bash
# Downloads the owner's sample exams from Cloudflare R2 (S3 API) into a git-ignored
# directory. Local only. The exams hold patient names in their file names and content, so
# this script never prints an object key, a file name, or a path below the destination:
# only counts, sizes, and fixed messages. The `aws` CLI's own output, which names
# objects, is captured to a temporary file, counted, and deleted.
#
#   scripts/fetch-sample-exams.sh           # sync the bucket into tests/sample-exams/r2/
#   scripts/fetch-sample-exams.sh --count   # objects and size in the bucket, nothing downloaded
#
# Setup (once): an R2 API token with "Object Read only" on the bucket, then
#   aws configure --profile r2      # region: auto
# and in ~/.aws/config, under [profile r2]:
#   endpoint_url = https://<account id>.r2.cloudflarestorage.com
#   request_checksum_calculation = when_required
#   response_checksum_validation = when_required
#
# Settings (environment): R2_PROFILE (default r2), R2_BUCKET (default sample-exams),
# R2_PREFIX (default empty: the whole bucket), R2_ENDPOINT (overrides the profile's
# endpoint_url), R2_DEST (default tests/sample-exams/r2). The default destination is a
# subdirectory on purpose: the local sample-exam tests and anonymize-sample-exams.sh read
# only the files directly in tests/sample-exams/, and a full bucket there would make them
# run for hours.
set -euo pipefail
cd "$(dirname "$0")/.."

profile=${R2_PROFILE:-r2}
bucket=${R2_BUCKET:-sample-exams}
prefix=${R2_PREFIX:-}
dest=${R2_DEST:-tests/sample-exams/r2}
source_uri="s3://${bucket}/${prefix}"

die() {
    echo "fetch-sample-exams: $*" >&2
    exit 1
}

command -v aws >/dev/null || die "the aws CLI is not installed (brew install awscli)"
aws_args=(--profile "$profile")
[[ -n ${R2_ENDPOINT:-} ]] && aws_args+=(--endpoint-url "$R2_ENDPOINT")

log=$(mktemp)
trap 'rm -f "$log"' EXIT

# Runs aws with its stdout and stderr in the log; on failure, prints only a count.
run_aws() {
    if ! aws "${aws_args[@]}" "$@" >"$log" 2>&1; then
        lines=$(grep -c . "$log" || true)
        die "aws failed (${lines} line(s) of output withheld: they may name exams); check the profile, the endpoint, and the token"
    fi
}

if [[ ${1:-} == "--count" ]]; then
    run_aws s3 ls "$source_uri" --recursive --summarize --human-readable
    # Only the two summary lines: every other line names an object.
    grep -E '^ *Total (Objects|Size):' "$log" | sed 's/^ */fetch-sample-exams: /'
    exit 0
fi
[[ $# -eq 0 ]] || die "usage: $0 [--count]"

# The destination must be one git cannot commit: inside this repository it must be
# ignored. Outside any work tree there is nothing to commit.
mkdir -p "$dest"
if git -C "$dest" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    git check-ignore -q "$dest/probe.pdf" ||
        die "the destination is not git-ignored; refusing to download exams there"
fi

count_files() { find "$dest" -type f ! -name '.*' | wc -l | tr -d ' '; }
before=$(count_files)
echo "fetch-sample-exams: ${before} file(s) in the destination before the sync"
# No --delete: a file removed from the bucket is never removed here.
run_aws s3 sync "$source_uri" "$dest" --only-show-errors --no-progress
after=$(count_files)
size=$(du -sh "$dest" | cut -f1 | tr -d " ")
echo "fetch-sample-exams: ${after} file(s) after the sync (+$((after - before))), ${size} on disk"
