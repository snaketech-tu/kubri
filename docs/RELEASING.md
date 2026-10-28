# Releasing

A release is a version tag. Pushing it builds the desktop app and the Docker image; the installers land in a draft release that is published by hand.

1. Set the new version in [`Cargo.toml`](../Cargo.toml) (`[workspace.package]`) and [`ui/package.json`](../ui/package.json), run `cargo check` so `Cargo.lock` follows, and merge that to `main`.
2. Tag the merge commit and push the tag:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

3. Two workflows start:
   - **Release** checks that the tag matches the version, creates a draft release with the text from [`.github/release-notes.md`](../.github/release-notes.md), and attaches the desktop builds: `.dmg` for macOS on Apple silicon and Intel, `.deb`, `.rpm` and `.AppImage` for Linux on x64 and Arm, and `.msi` and `-setup.exe` for Windows.
   - **Docker** publishes `ghcr.io/snaketech-tu/kubri` for amd64 and arm64 as `0.2.0`, `0.2` and `latest`. Every push to `main` also publishes `edge`.
4. When all builds are attached, open the draft on GitHub, write what changed under "What's new", and publish it.

To try the installers before tagging, run the Release workflow by hand (Actions → Release → Run workflow): it builds them without a release and keeps them as workflow artifacts. Pull requests that change the packaging do the same.

The first time an image is published, GitHub makes the package private: make it public under the package's settings (Package settings → Change visibility) so `docker pull` works without signing in.

## Signing

The builds are not signed yet. macOS asks before opening Kubri the first time (the app is signed ad hoc, so it is not reported as damaged), and Windows SmartScreen warns about an unknown app; the release text says how to get past both.

To sign them, add the certificates as repository secrets and pass them to the `tauri-apps/tauri-action` step in [`release.yml`](../.github/workflows/release.yml) as environment variables, as described in Tauri's guides for [macOS](https://v2.tauri.app/distribute/sign/macos/) (`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, and `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID` for notarizing) and [Windows](https://v2.tauri.app/distribute/sign/windows/).

## Tools in the image

The Docker image ships `kubectl` and `helm` at the versions in the `KUBECTL_VERSION` and `HELM_VERSION` arguments at the top of the [`Dockerfile`](../Dockerfile). Raise them now and then; both are checked against their published checksums while building.
