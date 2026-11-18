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

# The same 25 x 9 bitmap the TUI draws. A short diagonal sweep runs from the top right to the
# low left, the way the startup logo does; a non-interactive install gets the resting frame.
print_logo_frame() {
	awk -v frame="$1" -v plain="${2:-0}" '
	{
		rows[NR] = $0
		while (length(rows[NR]) < 25) rows[NR] = rows[NR] " "
	}
	END {
		esc = sprintf("%c", 27)
		head = 1 - frame / 5
		for (r = 1; r <= NR; r++) {
			line = ""
			for (c = 1; c <= 25; c++) {
				ch = substr(rows[r], c, 1)
				if (ch == " ") { line = line "  "; continue }
				d = ((c - 1) + (9 - r)) / 34
				forward = d - head
				while (forward < 0) forward += 1
				backward = head - d
				while (backward < 0) backward += 1
				dist = (forward < backward ? forward : backward) * 34
				shine = dist < 4 ? (1 - dist / 4) ^ 2 : 0
				if (ch == "#") {
					br = 120; bg = 116; bb = 133
					hr = 214; hg = 193; hb = 240
					t = 0.55 + 0.45 * shine
				} else {
					br = 47; bg = 43; bb = 60
					hr = 160; hg = 154; hb = 176
					t = 0.6 * shine
				}
				if (t > 1) t = 1
				rr = int(br + (hr - br) * t + 0.5)
				gg = int(bg + (hg - bg) * t + 0.5)
				b2 = int(bb + (hb - bb) * t + 0.5)
				if (plain) {
					line = line (ch == "#" ? "██" : "░░")
				} else {
					line = line esc "[38;2;" rr ";" gg ";" b2 "m██"
				}
			}
			print line (plain ? "" : esc "[0m")
		}
	}
	' <<'LOGO'
    .
  .
 .  # .   ### ### ### # #
   ### .  #   #   #   # #
  #####.  ### ### ### ##
   ###      # #   #   # #
.   #     ### ### ### # #
  ...
      ..
LOGO
}

banner() {
	if [ -t 1 ] && [ -z "${NO_COLOR:-}" ] && [ -z "${CI:-}" ]; then
		frame=0
		while [ "$frame" -lt 5 ]; do
			if [ "$frame" -gt 0 ]; then
				printf '\033[9A'
			fi
			print_logo_frame "$frame"
			sleep 0.07
			frame=$((frame + 1))
		done
	else
		print_logo_frame 5 1
	fi
}

banner
say ""

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
copy_if_absent "$REPO_DIR/builtin/SYSTEM.md" "$SEEK_HOME/SYSTEM.md"
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
		case "$(uname -m)" in
			x86_64 | amd64) arch=x86_64 ;;
			aarch64 | arm64) arch=aarch64 ;;
			*) arch=$(uname -m) ;;
		esac
		case "$(uname -s)" in
			Linux) target="${arch}-unknown-linux-gnu" ;;
			Darwin) target="${arch}-apple-darwin" ;;
			*) target="${arch}-$(uname -s | tr '[:upper:]' '[:lower:]')" ;;
		esac
		staging="seek-v${version}-${target}"
		asset="${staging}.tar.gz"
		url="https://github.com/piskevalee-cpu/Seek/releases/download/v${version}/${asset}"
		if command -v curl >/dev/null 2>&1 && curl -fsSL "$url" -o "/tmp/$asset" 2>/dev/null; then
			tar -xzf "/tmp/$asset" -C /tmp
			cp "/tmp/$staging/seek" "$BIN_DIR/seek"
			rm -rf "/tmp/$asset" "/tmp/$staging"
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
