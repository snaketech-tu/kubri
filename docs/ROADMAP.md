# Roadmap

What is coming in Kubri, roughly in order. Plans change with what people report, so [tell us](https://github.com/snaketech-tu/kubri/issues/new?template=idea.yml) what you are missing. What has already shipped is in the [changelog](../CHANGELOG.md).

## Now: 2.0, a solid base

2.0 adds few new features. It makes Kubri easier to trust, to install and to work on.

- **Signed apps.** The macOS app is signed and notarized, and the Windows installer is signed, so neither shows a warning on first start.
- **More ways to install.** An apt repository for Debian and Ubuntu, Homebrew, winget and the AUR, and the Helm chart as an OCI package.
- **Windows on Arm.** A native build for Windows arm64.
- **Checked dependencies.** Every change is checked for known vulnerabilities and licenses, dependencies are updated every week, and the Docker image is signed.
- **One way to talk to the core.** The desktop app and the web server use the same calls, generated from one description, so they cannot drift apart. This changes the web server's internal API.
- **Clearer drain.** Draining a node first checks that you may evict its pods.
- **Docs for operators.** How to run kubri-server safely for a team.

## Next

Chosen after 2.0 from what people ask for most. Ideas we already have:

- Forms for common objects, so you can create and change them without YAML.
- Alerts when something in a cluster you watch goes wrong.
- Saved views: a list with its filter, columns and namespaces under a name.

## Later

- Kubri on Flathub.
- More integrations, chosen by request.

## Releases

A new minor version comes out every two to four weeks, and a patch only when a bug needs fixing sooner. Before each minor version, Kubri is used daily for a week on real clusters.
