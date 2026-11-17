#!/bin/bash
set -e

# remove stale package files so removed sources don't linger in the output
rm -f src/rust/*/package.aix

for src in ./src/rust/*; do
  (
    cd "$src"
    ./build.sh
  )
done

# regenerate the published source list (public/index.json + sources + icons)
rm -rf public
aidoku build ./src/rust/*/package.aix
