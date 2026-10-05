#!/usr/bin/env sh
# Download the Stalwart Mail Server binary used by the integration tests into
# target/stalwart/. Override the version with STALWART_VERSION=vX.Y.Z.
set -eu

version="${STALWART_VERSION:-v0.16.25}"
dir="$(cd "$(dirname "$0")/.." && pwd)/target/stalwart"
bin="$dir/stalwart"

if [ -x "$bin" ] && [ "$("$bin" --version 2>/dev/null || true)" = "${version#v}" ]; then
    echo "stalwart $version already present at $bin"
    exit 0
fi

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)   target=x86_64-unknown-linux-gnu ;;
    Linux-aarch64)  target=aarch64-unknown-linux-gnu ;;
    Darwin-x86_64)  target=x86_64-apple-darwin ;;
    Darwin-arm64)   target=aarch64-apple-darwin ;;
    *) echo "unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

url="https://github.com/stalwartlabs/stalwart/releases/download/$version/stalwart-$target.tar.gz"
mkdir -p "$dir"
echo "downloading $url"
curl -fsSL "$url" -o "$dir/stalwart.tar.gz"
tar -xzf "$dir/stalwart.tar.gz" -C "$dir"
rm -f "$dir/stalwart.tar.gz"
chmod +x "$bin"
echo "installed stalwart $("$bin" --version) at $bin"
