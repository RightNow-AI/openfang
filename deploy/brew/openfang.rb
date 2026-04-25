# Homebrew tap formula skeleton for OpenFang.
#
# To publish: push this file to a tap repo (typically `<org>/homebrew-openfang`)
# under `Formula/openfang.rb`. Users then install with:
#
#   brew tap <org>/openfang
#   brew install openfang
#
# Bottle URLs and SHAs need to be filled in per release. The `download_strategy
# = :nil` placeholder below makes Homebrew fall back to building from source
# until you publish prebuilt bottles via `brew bottle openfang` and a tap
# release workflow.
#
# Building from source requires Rust 1.75+. Homebrew will pull `rust` as a
# build dependency unless the tap maintainer points it at a vendored
# toolchain.

class Openfang < Formula
  desc "Agent OS — Rust-native autonomous agent runtime"
  homepage "https://github.com/RightNow-AI/openfang"
  license any_of: ["Apache-2.0", "MIT"]

  # Pinned to v0.6.1 (the hardening release). Bump on every release alongside
  # the corresponding sha256 of the source tarball. To compute:
  #   curl -sL https://github.com/RightNow-AI/openfang/archive/refs/tags/v0.6.1.tar.gz | shasum -a 256
  url "https://github.com/RightNow-AI/openfang/archive/refs/tags/v0.6.1.tar.gz"
  sha256 "REPLACE_WITH_SHA256_OF_v0.6.1_TARBALL"
  version "0.6.1"

  head "https://github.com/RightNow-AI/openfang.git", branch: "main"

  depends_on "rust" => :build
  uses_from_macos "sqlite"

  def install
    system "cargo", "install", *std_cargo_args(path: "crates/openfang-cli")

    # Shell completions (ship them so users get tab-complete out of the box).
    bash_completion.install "deploy/shell/openfang.bash" => "openfang"
    zsh_completion.install "deploy/shell/openfang.zsh" => "_openfang"

    # Optional: install systemd / launchd templates under share/.
    (share/"openfang").install "deploy/launchd/io.openfang.plist"
    (share/"openfang").install "deploy/systemd/user/openfang.service"
    (share/"openfang").install "deploy/warp/openfang.yaml"
    (share/"openfang/docs").install "docs/security/cyber-intel-vault-setup.md"
    (share/"openfang/docs").install "docs/hardening-plan.md"
  end

  service do
    run [opt_bin/"openfang", "start"]
    keep_alive crashed: true
    log_path var/"log/openfang/stdout.log"
    error_log_path var/"log/openfang/stderr.log"
    working_dir "#{Dir.home}/.openfang"
  end

  test do
    # Smoke test: --version exits 0 and prints the version we built.
    assert_match version.to_s, shell_output("#{bin}/openfang --version")
  end
end
