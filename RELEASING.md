# Releasing pubky-backup

A release is a version tag on `main`. Pushing the tag builds the app for every platform and publishes a GitHub Release with the installers.

## Prerequisites

- Push access to the repository, to push tags and publish releases.
- Node.js and Rust installed, to run the version script.

## Release steps

### 1. Bump the version and create a version PR

Start from the latest `main`:

```bash
git checkout main
git pull origin main
```

Run the version script with the new version, without the `v`:

```bash
./.scripts/set-version.sh 0.6.0
```

The script asks for confirmation, then sets the version in `package.json`, `package-lock.json`, both `Cargo.toml` files and `src-tauri/Cargo.lock`. The app bundle takes its version from `src-tauri/Cargo.toml`, so `tauri.conf.json` has no version of its own.

Create the version PR:

```bash
git checkout -b chore/v0.6.0
git add -A
git commit -m "chore: v0.6.0"
git push origin chore/v0.6.0
```

Use the PR title `chore: v0.6.0`.

### 2. Tag and push

After the version PR is reviewed and squash-merged, tag the merge commit on `main`:

```bash
git checkout main
git pull origin main
git tag v0.6.0
git push origin v0.6.0
```

The tag starts the [build and release workflow](.github/workflows/build-and-release.yml). That's it: CI handles the rest, and the release is public as soon as the workflow finishes.

## Post-release verification

On [Releases](https://github.com/pubky/pubky-backup/releases), check the release named `Release v0.6.0`:

- [ ] It has all five artifacts:
  - `pubky-backup-v0.6.0-linux-amd64.AppImage`
  - `pubky-backup-v0.6.0-linux-amd64.deb`
  - `pubky-backup-v0.6.0-windows-amd64.msi`
  - `pubky-backup-v0.6.0-macos-arm64.app.tar.gz`
  - `pubky-backup-v0.6.0-macos-amd64.app.tar.gz`
- [ ] The release notes read well. They are generated from the conventional-commit titles of the PRs merged since the previous tag; edit the release to fix them.
- [ ] A build for your platform starts and shows the new version.

## What CI does

When a `v*` tag is pushed, the workflow:

1. **Validates** that the tag is valid semver and matches the version in both `Cargo.toml` files and `package.json`. If any differ, nothing is built.
2. **Builds** the app for Linux (amd64), Windows (amd64) and macOS (arm64 and amd64).
3. **Publishes a GitHub Release** with the artifacts and release notes grouped by commit type (features, fixes, and so on), once every build has succeeded. A tag with a pre-release suffix, such as `v0.6.0-rc.1`, is published as a pre-release.

The release notes depend on PR titles following conventional commits (`feat:`, `fix:`, `chore:` and so on). The [PR title check](.github/workflows/pr-title.yml) enforces this on every PR.

## Versioning

Follow [Semantic Versioning](https://semver.org/):

- **Patch** (0.5.x): bug fixes only.
- **Minor** (0.x.0): new features, or changes users notice, such as a feature turned off.
- **Major** (x.0.0): changes that break existing backups or require users to act, such as a new data directory layout.

Pre-release versions such as `0.6.0-rc.1` also work: set them with the script and tag `v0.6.0-rc.1`. CI publishes them as pre-releases, which GitHub never shows as the latest release.

## Hotfix releases

Use a hotfix when a fix must ship without what `main` has gained since the last release.

1. Create a branch from the release tag:

   ```bash
   git checkout -b hot_fix_v0.5.1 v0.5.0
   ```

2. Merge the fixes into the branch, then bump the version:

   ```bash
   ./.scripts/set-version.sh 0.5.1
   git commit -am "chore: v0.5.1"
   ```

3. Push the branch, then tag it and push the tag:

   ```bash
   git push origin hot_fix_v0.5.1
   git tag v0.5.1
   git push origin v0.5.1
   ```

4. Back-port the fix to `main` so it isn't lost:

   ```bash
   git checkout main
   git cherry-pick <commit-sha>
   ```

## Troubleshooting

**The Validate job fails with a version mismatch.** The tag doesn't match the version in the files, usually because the version PR wasn't merged before tagging. Delete the tag, merge the version PR, then tag again:

```bash
git tag -d v0.6.0
git push origin :refs/tags/v0.6.0
```

**A build job fails.** No release is published, since publishing waits for every build. Delete the tag as above, fix the problem on `main`, then tag the new commit and push the tag again.

**A build failed for a reason unrelated to the code**, such as a download timing out. Re-run the failed jobs from the workflow run's page instead of re-tagging.
