#!/bin/sh
# Build Sloth.app and install it in /Applications.
# Needs cargo-bundle: cargo install cargo-bundle
set -eu
cd "$(dirname "$0")/.."

# cargo-bundle reads the bundle settings from the package's own Cargo.toml,
# so run it there; the output still lands in the workspace's target folder.
(cd sloth && cargo bundle --release)
APP=target/release/bundle/osx/Sloth.app

rm -rf /Applications/Sloth.app
cp -R "$APP" /Applications/

# Tell macOS about the new copy so Finder's "Open With" lists it straight away.
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
"$LSREGISTER" -f /Applications/Sloth.app

echo "Installed /Applications/Sloth.app"
