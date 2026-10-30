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

Without a terminal, the release page on GitHub can make the tag instead: open `https://github.com/snaketech-tu/kubri/releases/new?tag=v0.2.0&target=main`, write the text (the one from `.github/release-notes.md` with "What's new" filled in) and choose **Publish release**. Saving it as a draft does not create the tag, so nothing would start. The same two workflows then run, and Release attaches the builds to this release instead of making a draft. It is public right away, and the installers show up as each build finishes, within about half an hour.

To try the installers before tagging, run the Release workflow by hand (Actions → Release → Run workflow): it builds them without a release and keeps them as workflow artifacts. Pull requests that change the packaging do the same.

The first time an image is published, GitHub makes the package private: make it public under the package's settings (Package settings → Change visibility) so `docker pull` works without signing in.

## Package managers

After the desktop builds, the **Packages** job fills the templates in [`packaging/`](../packaging) with the version and the checksums of the release's downloads ([`packaging/render.sh`](../packaging/render.sh)), keeps the results as the `packages` workflow artifact, and publishes each one whose secret is set. While the release is still a draft it only fills them in, since the packages point at downloads that are not public yet; run the job again after publishing.

| Channel | What happens | Secret |
| --- | --- | --- |
| Homebrew | Commits `Casks/kubri.rb` to [`snaketech-tu/homebrew-kubri`](https://github.com/snaketech-tu/homebrew-kubri). The cask removes the quarantine flag, since the macOS build is not signed. | `HOMEBREW_TAP_TOKEN`: a fine-grained token with contents write access to that repository |
| AUR | Pushes `PKGBUILD` and `.SRCINFO` of `kubri-bin`, made from the `.deb`. | `AUR_SSH_KEY`: the private SSH key of the AUR account that maintains the package |
| Flathub | Opens a pull request to `flathub/io.github.snaketech_tu.kubri` with the manifest and metainfo. | `FLATHUB_TOKEN`: a token of a maintainer of that repository |
| winget | The **winget** job runs `wingetcreate update SnakeTech.Kubri`, which opens a pull request to `microsoft/winget-pkgs`. | `WINGET_TOKEN`: a classic token with `public_repo` |

Each needs a one-time setup first:

- **Homebrew**: create the public repository `snaketech-tu/homebrew-kubri`. People then install with `brew install --cask snaketech-tu/kubri/kubri`.
- **AUR**: create an account on [aur.archlinux.org](https://aur.archlinux.org), add the public half of the SSH key to it, and store the private half as `AUR_SSH_KEY`. The first push creates `kubri-bin`.
- **Flathub**: submit the app once by hand, as [Flathub's guide](https://docs.flathub.org/docs/for-app-authors/submission) describes, with the manifest and metainfo from the `packages` artifact. Once it is accepted, Flathub creates the repository and the job keeps it up to date. The Flatpak cannot run commands installed on the computer, so kubeconfigs that sign in through one, k9s plugins and Helm changes do not work in it; its description says so.
- **winget**: submit the first version by hand, with `wingetcreate submit` and the manifests from the `packages` artifact, or as a pull request to `microsoft/winget-pkgs`. Later versions go through the job.

## Signing

The builds are not signed yet. macOS asks before opening Kubri the first time (the app is signed ad hoc, so it is not reported as damaged), and Windows SmartScreen warns about an unknown app; the release text says how to get past both.

To sign them, add the certificates as repository secrets and pass them to the `tauri-apps/tauri-action` step in [`release.yml`](../.github/workflows/release.yml) as environment variables, as described in Tauri's guides for [macOS](https://v2.tauri.app/distribute/sign/macos/) (`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, and `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID` for notarizing) and [Windows](https://v2.tauri.app/distribute/sign/windows/).

## Tools in the image

The Docker image ships `kubectl` and `helm` at the versions in the `KUBECTL_VERSION` and `HELM_VERSION` arguments at the top of the [`Dockerfile`](../Dockerfile). Raise them now and then; both are checked against their published checksums while building.
