#!/usr/bin/env python3
"""Render a binary-only formula from the release's published checksums.txt."""
import argparse
from pathlib import Path
import re
import sys

PLATFORMS = {
    "macos": {"intel": "x86_64-apple-darwin", "arm": "aarch64-apple-darwin"},
    "linux": {"intel": "x86_64-unknown-linux-gnu", "arm": "aarch64-unknown-linux-gnu"},
}
REPOSITORY = "https://github.com/federicopinan/lazylofi"


def render(tag, manifest):
    if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?", tag):
        raise ValueError("tag must be vMAJOR.MINOR.PATCH with an optional prerelease")
    expected = {
        f"lazylofi-{tag}-{target}.tar.gz"
        for arches in PLATFORMS.values() for target in arches.values()
    }
    hashes = {}
    for number, line in enumerate(manifest.splitlines(), 1):
        if not line.strip():
            continue
        fields = line.split()
        if len(fields) != 2:
            raise ValueError(f"malformed checksum line {number}")
        digest, filename = fields
        filename = filename.removeprefix("*")
        # Windows is published alongside the four Homebrew assets but is not used here.
        if filename.endswith("-x86_64-pc-windows-msvc.zip"):
            continue
        if filename not in expected:
            if any(filename.endswith(f"-{target}.tar.gz")
                   for arches in PLATFORMS.values() for target in arches.values()):
                raise ValueError(f"asset version does not match {tag}: {filename}")
            raise ValueError(f"unexpected asset: {filename}")
        if filename in hashes:
            raise ValueError(f"duplicate checksum for {filename}")
        if not re.fullmatch(r"[a-fA-F0-9]{64}", digest):
            raise ValueError(f"invalid SHA256 for {filename}")
        hashes[filename] = digest.lower()
    # Validate everything before emitting any formula bytes.
    for arches in PLATFORMS.values():
        for target in arches.values():
            filename = f"lazylofi-{tag}-{target}.tar.gz"
            if filename not in hashes:
                raise ValueError(f"missing checksum for {filename}")

    lines = [
        "# Generated from the release checksums.txt; do not edit by hand.",
        "class Lazylofi < Formula",
        '  desc "A lazy TUI player for lofi, synthwave, jazz lofi, and ambient music"',
        f'  homepage "{REPOSITORY}"',
        f'  version "{tag[1:]}"',
        '  license "MIT"',
        "",
    ]
    for platform, arches in PLATFORMS.items():
        lines.append(f"  on_{platform} do")
        for arch, target in arches.items():
            filename = f"lazylofi-{tag}-{target}.tar.gz"
            lines.extend([
                f"    on_{arch} do",
                f'      url "{REPOSITORY}/releases/download/{tag}/{filename}"',
                f'      sha256 "{hashes[filename]}"',
                "    end",
            ])
        lines.extend(["  end", ""])
    lines.extend([
        "  # Linux uses the system ALSA runtime and desktop PCM plugins, not Homebrew ALSA.",
        "  def install",
        '    bin.install "lazylofi"',
        "  end",
        "",
        "  test do",
        '    assert_match version.to_s, shell_output("#{bin}/lazylofi --version")',
        "  end",
        "end",
        "",
    ])
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True, help="release tag, e.g. v1.5.9")
    parser.add_argument("--checksums", required=True, type=Path,
                        help="published checksums.txt (sha256sum format)")
    args = parser.parse_args()
    try:
        formula = render(args.tag, args.checksums.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    sys.stdout.write(formula)
    return 0


if __name__ == "__main__":
    sys.exit(main())
