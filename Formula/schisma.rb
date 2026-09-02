class Schisma < Formula
  desc "Standalone controller-first MPE physical-modeling synthesizer"
  homepage "https://github.com/TheColby/schisma"
  license "MIT"
  head "https://github.com/TheColby/schisma.git", branch: "main"

  depends_on "rust" => :build
  depends_on :macos

  def fetch
    system "cargo", "fetch", "--locked"
  end

  def install
    system "cargo", "install", "--offline",
           *std_cargo_args(root: libexec, path: "crates/schisma-app")
    system "cargo", "install", "--offline",
           *std_cargo_args(root: libexec, path: "crates/schisma-engine")
    system "cargo", "install", "--offline",
           *std_cargo_args(root: libexec, path: "crates/schisma-gpu")

    app = libexec/"Schisma.app/Contents"
    (app/"MacOS").install libexec/"bin/schisma"
    app.install "packaging/macos/Info.plist"
    bin.write_exec_script app/"MacOS/schisma"

    bin.install libexec/"bin/schisma-render"
    bin.install libexec/"bin/schisma-live"
    bin.install libexec/"bin/schisma-gpu-info"
  end

  def caveats
    <<~EOS
      Launch the standalone interface with:
        schisma

      The app bundle is installed at:
        #{opt_libexec}/Schisma.app
    EOS
  end

  test do
    assert_match "8000..384000", shell_output("#{bin}/schisma-render --help")
    assert_match "self-test: passed", shell_output("#{bin}/schisma-gpu-info")
  end
end
