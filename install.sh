#!/bin/sh
# Installs Oven Spring, or updates it to the latest release: downloads the archive for this
# machine, verifies it against the release's SHA256SUMS, and puts the one executable,
# `ovenspring`, in ~/.local/bin. A download that fails or does not match leaves the installed
# executable as it was.
#
#   curl -fsSL https://github.com/shinpr/ovenspring/releases/latest/download/install.sh | sh
#
# OVENSPRING_RELEASE_URL names another release to install from, such as
# https://github.com/shinpr/ovenspring/releases/download/v0.1.0-rc.1
set -eu

release="${OVENSPRING_RELEASE_URL:-https://github.com/shinpr/ovenspring/releases/latest/download}"
bin_dir="$HOME/.local/bin"

fail() {
  echo "ovenspring install: $*" >&2
  exit 1
}

case "$(uname -s)" in
  Darwin) os="apple-darwin" ;;
  Linux) os="unknown-linux-musl" ;;
  *) fail "Oven Spring runs on macOS and Linux; this machine runs $(uname -s)." ;;
esac
case "$(uname -m)" in
  arm64 | aarch64) arch="aarch64" ;;
  x86_64 | amd64) arch="x86_64" ;;
  *) fail "Oven Spring is built for arm64 and x86_64; this machine is $(uname -m)." ;;
esac
archive="ovenspring-$arch-$os.tar.gz"

command -v curl >/dev/null 2>&1 || fail "curl is needed to download Oven Spring."
if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d ' ' -f 1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
else
  fail "sha256sum or shasum is needed to verify the download."
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "Downloading $archive"
curl -fsSL "$release/$archive" -o "$work/$archive" || fail "could not download $release/$archive"
curl -fsSL "$release/SHA256SUMS" -o "$work/SHA256SUMS" || fail "could not download $release/SHA256SUMS"

expected="$(awk -v name="$archive" '$2 == name || $2 == "*" name { print $1 }' "$work/SHA256SUMS")"
[ -n "$expected" ] || fail "SHA256SUMS lists no $archive; the installed executable is unchanged."
actual="$(sha256 "$work/$archive")"
[ "$actual" = "$expected" ] ||
  fail "$archive does not match SHA256SUMS; the installed executable is unchanged. Run the script again."

tar -xzf "$work/$archive" -C "$work" ovenspring || fail "$archive holds no ovenspring executable."
mkdir -p "$bin_dir"
# Moved into place in one step, so a running daemon keeps its executable until it restarts.
cp "$work/ovenspring" "$bin_dir/.ovenspring.new"
chmod 755 "$bin_dir/.ovenspring.new"
mv -f "$bin_dir/.ovenspring.new" "$bin_dir/ovenspring"

echo "Installed $bin_dir/ovenspring"
case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *)
    echo "Add it to your PATH, for example in your shell's profile:"
    echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
    ;;
esac
echo "Repositories already running Oven Spring use the new version after \`ovenspring stop\` and \`ovenspring start\`."
