#!/bin/sh
# Install SEEK.
#
# The script is idempotent: it never overwrites a file of the user. It creates the folder layout
# of ~/seek, copies the built-in skills and widgets that are not there yet, installs the binary
# and checks what SEEK needs.
#
# Usage:
#   ./install.sh              install the binary and the folders
#   ./install.sh --from-source  build and install the binary from the source
#   ./install.sh --no-binary    only the folders and the built-in files

set -eu

SEEK_HOME="${SEEK_HOME:-$HOME/seek}"
BIN_DIR="$HOME/.local/bin"
REPO_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
FROM_SOURCE=0
INSTALL_BINARY=1

for argument in "$@"; do
	case "$argument" in
	--from-source) FROM_SOURCE=1 ;;
	--no-binary) INSTALL_BINARY=0 ;;
	-h | --help)
		sed -n '3,12p' "$0" | sed 's/^# \{0,1\}//'
		exit 0
		;;
	*)
		echo "unknown option: $argument" >&2
		exit 1
		;;
	esac
done

say() {
	printf '%s\n' "$*"
}

# ---------------------------------------------------------------- the folders

say "==> the folders of $SEEK_HOME"
for folder in skills tools models widgets state; do
	mkdir -p "$SEEK_HOME/$folder"
	say "    $SEEK_HOME/$folder"
done

# ------------------------------------------------------- the built-in files

copy_if_absent() {
	source="$1"
	target="$2"
	if [ -e "$target" ]; then
		say "    kept   $target"
		return
	fi
	if [ ! -e "$source" ]; then
		return
	fi
	mkdir -p "$(dirname -- "$target")"
	cp -R "$source" "$target"
	say "    copied $target"
}

say "==> the built-in skills and widgets"
for skill in "$REPO_DIR"/builtin/skills/*; do
	[ -d "$skill" ] || continue
	copy_if_absent "$skill" "$SEEK_HOME/skills/$(basename -- "$skill")"
done
for widget in "$REPO_DIR"/builtin/widgets/*; do
	[ -d "$widget" ] || continue
	copy_if_absent "$widget" "$SEEK_HOME/widgets/$(basename -- "$widget")"
done

# ----------------------------------------------------------------- the binary

if [ "$INSTALL_BINARY" = "1" ]; then
	say "==> the binary"
	mkdir -p "$BIN_DIR"
	if [ "$FROM_SOURCE" = "1" ]; then
		say "    building from the source"
		cargo install --path "$REPO_DIR/crates/seek" --root "$HOME/.cargo" --force
		cp "$HOME/.cargo/bin/seek" "$BIN_DIR/seek"
	else
		# The release of the platform, when there is one; the source build is the fallback.
		version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO_DIR/Cargo.toml" | head -1)
		asset="seek-${version}-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m).tar.gz"
		url="https://github.com/piskevalee-cpu/Seek/releases/download/v${version}/${asset}"
		if command -v curl >/dev/null 2>&1 && curl -fsSL "$url" -o "/tmp/$asset" 2>/dev/null; then
			tar -xzf "/tmp/$asset" -C /tmp
			cp "/tmp/seek" "$BIN_DIR/seek"
			rm -f "/tmp/$asset" "/tmp/seek"
		else
			say "    no release for this platform, building from the source"
			cargo install --path "$REPO_DIR/crates/seek" --root "$HOME/.cargo" --force
			cp "$HOME/.cargo/bin/seek" "$BIN_DIR/seek"
		fi
	fi
	chmod +x "$BIN_DIR/seek"
	say "    $BIN_DIR/seek"
fi

# ------------------------------------------------------------------- the checks

say "==> what SEEK needs"
missing=0

if command -v git >/dev/null 2>&1; then
	say "    git      $(command -v git)"
else
	say "    git      MISSING - install git to see the repository in the widgets"
	missing=1
fi

if command -v llama >/dev/null 2>&1; then
	say "    llama    $(command -v llama)"
elif command -v llama-server >/dev/null 2>&1; then
	say "    llama    $(command -v llama-server)"
else
	say "    llama    MISSING - install llama.cpp to run local models"
	say "             https://github.com/ggml-org/llama.cpp"
	missing=1
fi

if [ -d "$SEEK_HOME/models" ] && [ -n "$(ls -A "$SEEK_HOME/models" 2>/dev/null)" ]; then
	say "    models   $(ls "$SEEK_HOME/models" | head -3 | tr '\n' ' ')"
else
	say "    models   none yet - put a .gguf file in $SEEK_HOME/models"
fi

# ----------------------------------------------------------------- the next step

say ""
say "SEEK is installed. Next:"
say "    export PATH=\"\$HOME/.local/bin:\$PATH\""
say "    seek"
say ""
say "The configuration lives in $SEEK_HOME."
if [ "$missing" = "1" ]; then
	say "Some programs are missing; see the list above."
fi
