#!/bin/sh
# Build "Sloth Writer.app" and install it in /Applications.
# Needs cargo-bundle: cargo install cargo-bundle
set -eu
cd "$(dirname "$0")/.."

# cargo-bundle reads the bundle settings from the package's own Cargo.toml,
# so run it there; the output lands in the workspace's target folder.
(cd writer && cargo bundle --release)
APP="target/release/bundle/osx/Sloth Writer.app"

rm -rf "/Applications/Sloth Writer.app"
cp -R "$APP" /Applications/

# Tell macOS about the new copy so Launchpad and Spotlight find it straight away.
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
"$LSREGISTER" -f "/Applications/Sloth Writer.app"

echo "Installed /Applications/Sloth Writer.app"
