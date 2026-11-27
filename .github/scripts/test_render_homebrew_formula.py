"""Offline public-CLI contract tests; never install or modify a Homebrew tap."""
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / ".github/scripts/render_homebrew_formula.py"
TARGETS = (
    "x86_64-apple-darwin", "aarch64-apple-darwin",
    "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu",
)
HASHES = {target: str(index) * 64 for index, target in enumerate(TARGETS, 1)}


def manifest(tag="v1.5.9"):
    return "".join(f"{HASHES[t]}  lazylofi-{tag}-{t}.tar.gz\n" for t in TARGETS)


def expected_formula():
    text = '''# Generated from the release checksums.txt; do not edit by hand.
class Lazylofi < Formula
  desc "A lazy TUI player for lofi, synthwave, jazz lofi, and ambient music"
  homepage "https://github.com/federicopinan/lazylofi"
  version "1.5.9"
  license "MIT"

'''
    for platform, targets in (("macos", TARGETS[:2]), ("linux", TARGETS[2:])):
        text += f"  on_{platform} do\n"
        for arch, target in zip(("intel", "arm"), targets):
            text += (f"    on_{arch} do\n"
                     f'      url "https://github.com/federicopinan/lazylofi/releases/download/v1.5.9/lazylofi-v1.5.9-{target}.tar.gz"\n'
                     f'      sha256 "{HASHES[target]}"\n'
                     "    end\n")
        text += "  end\n\n"
    return text + '''  # Linux uses the system ALSA runtime and desktop PCM plugins, not Homebrew ALSA.
  def install
    bin.install "lazylofi"
  end

  test do
    assert_match(/\Alazylofi \d+\.\d+\.\d+/, shell_output("#{bin}/lazylofi --version"))
  end
end
'''


class RendererTests(unittest.TestCase):
    def invoke(self, content, tag="v1.5.9"):
        with tempfile.TemporaryDirectory(prefix="lazylofi-formula-") as directory:
            path = pathlib.Path(directory) / "checksums.txt"
            path.write_text(content)
            result = subprocess.run(
                [sys.executable, "-B", str(SCRIPT), "--tag", tag,
                 "--checksums", str(path)], capture_output=True, text=True,
            )
            self.assertEqual(path.read_text(), content, "input manifest must stay unchanged")
            self.assertEqual(sorted(p.name for p in pathlib.Path(directory).iterdir()),
                             ["checksums.txt"], "CLI must not write files")
            return result

    def reject(self, content, error, tag="v1.5.9"):
        result = self.invoke(content, tag)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, f"error: {error}\n")

    def test_complete_formula_and_windows_ignored(self):
        result = self.invoke(manifest() + "not-a-hash  lazylofi-v1.5.9-x86_64-pc-windows-msvc.zip\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")
        self.assertEqual(result.stdout, expected_formula())
        for dependency in ('depends_on', 'cargo', 'pkg-config', 'libasound2-dev'):
            self.assertNotIn(dependency, result.stdout)

    def test_missing_each_target(self):
        for target in TARGETS:
            with self.subTest(target=target):
                content = "".join(line + "\n" for line in manifest().splitlines() if target not in line)
                self.reject(content, f"missing checksum for lazylofi-v1.5.9-{target}.tar.gz")

    def test_duplicate_each_target_even_if_identical(self):
        for line in manifest().splitlines():
            with self.subTest(line=line):
                self.reject(manifest() + line + "\n", f"duplicate checksum for {line.split()[1]}")

    def test_malformed_each_expected_hash(self):
        for target in TARGETS:
            for bad in ("f" * 63, "f" * 65, "g" * 64):
                with self.subTest(target=target, bad=bad):
                    self.reject(manifest().replace(HASHES[target], bad),
                                f"invalid SHA256 for lazylofi-v1.5.9-{target}.tar.gz")

    def test_mismatched_versions(self):
        for target in TARGETS:
            filename = f"lazylofi-v1.5.9-{target}.tar.gz"
            wrong = filename.replace("v1.5.9", "v1.5.8")
            self.reject(manifest().replace(filename, wrong),
                        f"asset version does not match v1.5.9: {wrong}")

    def test_invalid_tag(self):
        for tag in ("1.5.9", 'v1.5.9"', "v1.5.9/evil"):
            self.reject(manifest(), "tag must be vMAJOR.MINOR.PATCH with an optional prerelease", tag)

    def test_malformed_line(self):
        self.reject(manifest() + "broken\n", "malformed checksum line 5")

    def test_order_independence_and_sha256sum_binary_format(self):
        content = "\n" + "\n".join(reversed(manifest().replace("  lazylofi", " *lazylofi").splitlines())) + "\n"
        result = self.invoke(content)
        self.assertEqual((result.returncode, result.stdout, result.stderr),
                         (0, expected_formula(), ""))

    @unittest.skipUnless(shutil.which("ruby"), "Ruby unavailable; formula syntax not checked")
    def test_ruby_syntax(self):
        result = self.invoke(manifest())
        self.assertEqual(result.returncode, 0, result.stderr)
        syntax = subprocess.run(["ruby", "-c"], input=result.stdout, text=True, capture_output=True)
        self.assertEqual((syntax.returncode, syntax.stdout, syntax.stderr), (0, "Syntax OK\n", ""))


class WorkflowTests(unittest.TestCase):
    def test_native_runner_baselines_and_windows_preserved(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        for target, runner in ((TARGETS[0], "macos-15-intel"), (TARGETS[1], "macos-15"),
                               (TARGETS[2], "ubuntu-24.04"), (TARGETS[3], "ubuntu-24.04-arm"),
                               ("x86_64-pc-windows-msvc", "windows-latest")):
            self.assertIn(f"target: {target}\n            os: {runner}\n", workflow)

    def test_published_manifest_renderer_and_auth_boundary(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        release = workflow.split("  release:\n", 1)[1].split("  update-tap:\n", 1)[0]
        tap = workflow.split("  update-tap:\n", 1)[1]
        self.assertIn("sha256sum *.{tar.gz,zip} > ../checksums.txt", release)
        self.assertIn("            checksums.txt\n", release)
        self.assertIn("gh release download", tap)
        self.assertIn('--pattern checksums.txt', tap)
        self.assertIn('GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}', tap)
        self.assertIn('python3 .github/scripts/render_homebrew_formula.py', tap)
        self.assertIn('--tag "$GITHUB_REF_NAME" --checksums checksums.txt > lazylofi.rb', tap)
        self.assertLess(tap.index('render_homebrew_formula.py'), tap.index('git clone'))
        self.assertIn('cp lazylofi.rb tap/Formula/lazylofi.rb', tap)
        self.assertNotIn('/archive/refs/tags/', tap)
        self.assertNotIn('Compute source tarball SHA', tap)
        self.assertIn('TAP_TOKEN: ${{ secrets.HOMEBREW }}', tap)
        self.assertEqual(tap.count("if: ${{ env.TAP_TOKEN != '' }}"), 4)
        self.assertIn('https://x-access-token:${TAP_TOKEN}@github.com/federicopinan/homebrew-tap.git', tap)
        self.assertIn('    permissions:\n      contents: read\n', tap)
        self.assertIn('          git push\n', tap)
        self.assertIn('tar -C staging -czf', workflow)


if __name__ == "__main__":
    unittest.main()
