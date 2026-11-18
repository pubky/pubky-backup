#!/bin/bash

# -------------------------------------------------------------------------------------------------
# Sets the version of the app: both Rust crates, package.json and both lockfiles.
# The Tauri bundle takes its version from src-tauri/Cargo.toml.
# -------------------------------------------------------------------------------------------------

set -e # fail the script if any command fails
set -u # fail the script if any variable is not set
set -o pipefail # fail the script if any pipe command fails

cd "$(dirname "$0")/.."

NEW_VERSION=${1:-}
if [ -z "$NEW_VERSION" ]; then
  echo "Error: New version not specified."
  echo "Usage: $0 <new_version>"
  exit 1
fi

SEMVER_REGEX="^([0-9]+)\.([0-9]+)\.([0-9]+)(-([0-9A-Za-z.-]+))?(\+([0-9A-Za-z.-]+))?$"
if [[ ! "$NEW_VERSION" =~ $SEMVER_REGEX ]]; then
  echo "Error: Version '$NEW_VERSION' is not in semver format (e.g., 1.2.3, 1.0.0-rc.1)."
  exit 1
fi

read -p "Are you sure you want to set the version to $NEW_VERSION? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
  echo "Version change cancelled."
  exit 1
fi

echo "Updating package.json and package-lock.json to $NEW_VERSION..."
npm version --no-git-tag-version --allow-same-version "$NEW_VERSION" > /dev/null

# The crates depend on each other by path, so only their own versions change
for MANIFEST in src-tauri/Cargo.toml src-tauri/pubky-backup-core/Cargo.toml; do
  echo "Updating $MANIFEST to $NEW_VERSION..."
  # The first `version =` line is the package's own
  sed -i.bak "1,/^version = /s/^version = \".*\"/version = \"$NEW_VERSION\"/" "$MANIFEST"
  rm "$MANIFEST.bak"
done

echo "Updating src-tauri/Cargo.lock..."
(cd src-tauri && cargo update --workspace --quiet)

echo Done
