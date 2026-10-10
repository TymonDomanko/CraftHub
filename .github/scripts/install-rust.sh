#!/usr/bin/env bash
# Install (if needed) and select stable Rust on any GitHub runner, including the ARM ones.
set -euo pipefail

if ! command -v rustup >/dev/null 2>&1; then
  if [ "$RUNNER_OS" = "Windows" ]; then
    arch=x86_64
    [ "$RUNNER_ARCH" = "ARM64" ] && arch=aarch64
    curl -sSfL -o rustup-init.exe "https://static.rust-lang.org/rustup/dist/${arch}-pc-windows-msvc/rustup-init.exe"
    ./rustup-init.exe -y --profile minimal --default-toolchain none
    rm rustup-init.exe
    cargo_bin="$(cygpath -w "$USERPROFILE/.cargo/bin")"
  else
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
    cargo_bin="$HOME/.cargo/bin"
  fi
  echo "$cargo_bin" >> "$GITHUB_PATH"
  export PATH="$HOME/.cargo/bin:$USERPROFILE/.cargo/bin:$PATH"
fi

rustup toolchain install stable --profile minimal --component clippy,rustfmt "$@"
rustup default stable
rustc --version
