# Full-suite Linux gate (spec/migration-plan.md §3.15 item 6): Rust 1.99.1 plus uv and the pinned
# Python, so the parity and BDD targets can run the reference. Build from this directory:
#
#   docker build -t mswea-rs-full parity
#
# Run with the parent `mini-rust-swe-agent/` mounted at /work (the reference needs its local,
# git-ignored uv.lock); the reference is mounted read-only and copied inside the container:
#
#   docker run --rm -v "$PWD/..":/work -v "<reference with uv.lock>":/work/mini-swe-agent:ro \
#     -w /work/mini-swe-agent-rs mswea-rs-full
FROM rust:1.99.1
COPY --from=ghcr.io/astral-sh/uv:0.13.9 /uv /uvx /usr/local/bin/
RUN rustup component add rustfmt clippy
ENV UV_PYTHON=3.15.2 \
    UV_PROJECT_ENVIRONMENT=/opt/mswea-venv \
    UV_CACHE_DIR=/opt/uv-cache \
    UV_PYTHON_INSTALL_DIR=/opt/uv-python \
    CARGO_TARGET_DIR=/tmp/target
# The pinned interpreter is part of the image; only the reference's packages are synced at run
# time, because the reference (with its git-ignored uv.lock) is mounted then.
RUN uv python install 3.15.2
# The read-only reference is copied (without its host virtualenv) so the build backend can write
# its metadata; MSWEA_RS_REFERENCE_DIR points the parity harness at the copy.
ENV MSWEA_RS_REFERENCE_DIR=/tmp/ref
CMD ["sh", "-c", "mkdir -p /tmp/ref && tar -C ../mini-swe-agent --exclude=.venv -cf - . | tar -C /tmp/ref -xf - && uv sync -q --frozen --project /tmp/ref && cargo fmt --check && cargo clippy -q --all-targets --locked -- -D warnings && cargo build -q --locked && cargo test --locked"]
