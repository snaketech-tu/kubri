# Homebrew cask for the snaketech-tu/homebrew-kubri tap. The release workflow fills in the
# version and checksums; see docs/RELEASING.md.
cask "kubri" do
  arch arm: "aarch64", intel: "x64"

  version "@VERSION@"
  sha256 arm:   "@SHA256_DMG_AARCH64@",
         intel: "@SHA256_DMG_X64@"

  url "https://github.com/snaketech-tu/kubri/releases/download/v#{version}/Kubri_#{version}_#{arch}.dmg"
  name "Kubri"
  desc "Fast, simple Kubernetes GUI that covers what k9s can do"
  homepage "https://github.com/snaketech-tu/kubri"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :catalina"

  app "Kubri.app"

  # The app is not signed with an Apple certificate, so macOS would refuse to open it from a
  # download. Removing the quarantine flag is what "Open anyway" does by hand.
  postflight do
    system_command "/usr/bin/xattr", args: ["-dr", "com.apple.quarantine", "#{appdir}/Kubri.app"]
  end

  zap trash: [
    "~/Library/Application Support/io.github.snaketech-tu.kubri",
    "~/Library/Application Support/kubri",
    "~/Library/Caches/io.github.snaketech-tu.kubri",
    "~/Library/WebKit/io.github.snaketech-tu.kubri",
  ]
end
