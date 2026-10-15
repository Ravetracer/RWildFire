# Build environment for the Linux release: Debian 12 (glibc 2.36), so the binaries also run
# on older distributions than the build machine.
FROM rust:1.98-bookworm
RUN apt-get update \
 && apt-get install -y --no-install-recommends libasound2-dev pkg-config \
 && rm -rf /var/lib/apt/lists/*
