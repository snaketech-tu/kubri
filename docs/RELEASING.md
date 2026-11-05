# Releasing

A release is built and published in one run of the **Release** workflow.

1. Set the new version in [`Cargo.toml`](../Cargo.toml) (`[workspace.package]`), [`ui/package.json`](../ui/package.json), [`ui/src-tauri/tauri.conf.json`](../ui/src-tauri/tauri.conf.json) and [`deploy/helm/kubri/Chart.yaml`](../deploy/helm/kubri/Chart.yaml) (`version` and `appVersion`, so the chart installs the image of the same release), run `cargo check` so `Cargo.lock` follows, write what changed under "What's new" in [`.github/release-notes.md`](../.github/release-notes.md), and merge that to `main`.
2. Open **Actions → Release → Run workflow**, keep `main`, tick **publish** and choose **Run workflow**. From a terminal: `gh workflow run release.yml --ref main -f publish=true`.
3. The run checks that the version is not released yet and creates a draft release with the text from `.github/release-notes.md`. It attaches the desktop builds to it: `.dmg` for macOS on Apple silicon and Intel, `.deb`, `.rpm` and `.AppImage` for Linux on x64 and Arm, and `.msi` and `-setup.exe` for Windows. With the updater key set up, it also attaches their signatures and `latest.json` (see [Updates in the app](#updates-in-the-app)). It publishes `ghcr.io/snaketech-tu/kubri` for amd64 and arm64 as `X.Y.Z`, `X.Y` and `latest`; every push to `main` also publishes `edge`.
4. When every build is attached, the run publishes the release, which creates the tag `vX.Y.Z` on the commit it ran on, and then updates the [package managers](#package-managers). Until then nobody sees the release. If a build fails, the draft stays: run the failed jobs again, or the whole workflow, which uses the same draft.

Pushing a tag still works too (`git tag v1.6.1 && git push origin v1.6.1`): the builds then land in a draft to publish by hand, and the **Docker** workflow publishes the image. A release published on GitHub's release page creates its tag and gets the builds attached as they finish, so it is public before it is complete.

To try the installers before releasing, run the Release workflow by hand without **publish**: it builds them without a release and keeps them as workflow artifacts. Pull requests that change the packaging do the same.

The first time an image is published, GitHub makes the package private: make it public under the package's settings (Package settings → Change visibility) so `docker pull` works without signing in.

## Package managers

After the desktop builds, the **Packages** job fills the templates in [`packaging/`](../packaging) with the version and the checksums of the release's downloads ([`packaging/render.sh`](../packaging/render.sh)), keeps the results as the `packages` workflow artifact, and publishes each one whose secret is set. In a run with **publish** this happens after the release is published. For a release that is still a draft, it only fills them in, since the packages point at downloads that are not public yet; run the job again after publishing.

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

## Updates in the app

The desktop app updates itself from the latest release ([SETTINGS.md](SETTINGS.md#updates)). It only installs packages signed with the updater key, whose public half is built into the app. Set the key up once:

1. Make a key pair, with a password, somewhere outside the repository:

   ```sh
   cd ui
   pnpm tauri signer generate -w "$HOME/.tauri/kubri-updater.key"
   ```

   This writes `kubri-updater.key` (the private key) and `kubri-updater.key.pub` (the public key). Keep the private key and its password safe, in a password manager for instance: an app built with this public key only accepts updates signed with this private key, so losing it means users have to install the next release by hand.
2. Under **Settings → Secrets and variables → Actions** in the repository, add:
   - the secret `TAURI_SIGNING_PRIVATE_KEY` with the contents of `kubri-updater.key`,
   - the secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` with its password,
   - the variable (on the **Variables** tab) `TAURI_UPDATER_PUBKEY` with the contents of `kubri-updater.key.pub`.

From then on, each release's builds put the public key into the app and attach a signature (`.sig`) next to each package, and the **Updater** job attaches `latest.json`, which lists the package and signature for each platform and format. The app reads `latest.json` from the latest published release, so a draft reaches nobody until it is published. Without the key, the builds work as before: no signatures, no `latest.json`, and the app only shows the notice for new releases.

## Code signing

The builds are not code-signed yet. macOS asks before opening Kubri the first time (the app is signed ad hoc, so it is not reported as damaged), and Windows SmartScreen warns about an unknown app; the release text says how to get past both.

To sign them, add the certificates as repository secrets and pass them to the `tauri-apps/tauri-action` step in [`release.yml`](../.github/workflows/release.yml) as environment variables, as described in Tauri's guides for [macOS](https://v2.tauri.app/distribute/sign/macos/) (`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, and `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID` for notarizing) and [Windows](https://v2.tauri.app/distribute/sign/windows/).

## Tools in the image

The Docker image ships `kubectl` and `helm` at the versions in the `KUBECTL_VERSION` and `HELM_VERSION` arguments at the top of the [`Dockerfile`](../Dockerfile). Raise them now and then; both are checked against their published checksums while building.
