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
# Credentials, either of:
# - R2_API_KEY, a Cloudflare account API token with R2 read access, in the environment or
#   in this crate's .env (git-ignored). The S3 pair is derived at each run, as Cloudflare
#   documents it: the access key id is the token's id (from the token-verify API) and the
#   secret is the SHA-256 of the token. It goes to `aws` through that command's
#   environment only; nothing is saved, and no profile is used.
# - an `aws` profile (R2_PROFILE, default r2) with the S3 pair, set up once with
#   `aws configure --profile r2` (region: auto).
# The endpoint is https://<R2_ACCOUNT_ID>.r2.cloudflarestorage.com, with R2_ACCOUNT_ID
# from the environment or .env; without it, the profile's endpoint_url. Under the profile,
# also set request_checksum_calculation and response_checksum_validation to when_required.
#
# Settings (environment): R2_PROFILE (default r2), R2_BUCKET (default sample-exams),
# R2_PREFIX (default empty: the whole bucket), R2_ENDPOINT (overrides the endpoint),
# R2_DEST (default tests/sample-exams/r2). The default destination is a
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

# A setting from the environment, else from this crate's .env (never sourced: read only).
setting() {
    if [[ -n ${!1:-} ]]; then
        printf '%s' "${!1}"
    elif [[ -f .env ]]; then
        sed -n "s/^$1=//p" .env | tail -1 | sed -e "s/^[\"']//" -e "s/[\"']\$//"
    fi
}

account=$(setting R2_ACCOUNT_ID)
endpoint=${R2_ENDPOINT:-}
[[ -z $endpoint && -n $account ]] && endpoint="https://${account}.r2.cloudflarestorage.com"
token=$(setting R2_API_KEY)
if [[ -n $token ]]; then
    [[ -n $account ]] || die "R2_API_KEY needs R2_ACCOUNT_ID (in the environment or .env)"
    # Prints "<id> <sha256>" or nothing; the token travels in a header, never in argv.
    pair=$(R2_TOKEN=$token R2_ACCOUNT=$account python3 - <<'PY'
import hashlib, json, os, urllib.request
token, account = os.environ["R2_TOKEN"], os.environ["R2_ACCOUNT"]
url = f"https://api.cloudflare.com/client/v4/accounts/{account}/tokens/verify"
request = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}"})
try:
    with urllib.request.urlopen(request, timeout=20) as response:
        result = json.load(response).get("result") or {}
except Exception:
    result = {}
if result.get("status") == "active" and result.get("id"):
    print(result["id"], hashlib.sha256(token.encode()).hexdigest())
PY
)
    [[ -n $pair ]] || die "R2_API_KEY did not verify as an active token of account R2_ACCOUNT_ID"
    export AWS_ACCESS_KEY_ID=${pair%% *} AWS_SECRET_ACCESS_KEY=${pair#* }
    export AWS_DEFAULT_REGION=auto AWS_REQUEST_CHECKSUM_CALCULATION=when_required \
        AWS_RESPONSE_CHECKSUM_VALIDATION=when_required
    unset AWS_PROFILE AWS_SESSION_TOKEN
    aws_args=()
else
    aws_args=(--profile "$profile")
fi
[[ -n $endpoint ]] && aws_args+=(--endpoint-url "$endpoint")

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
