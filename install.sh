#!/bin/sh
# Install SEEK.
#
# The script is idempotent: it never overwrites a file of the user. It creates the folder layout
# of ~/seek, copies the built-in skills and widgets that are not there yet, installs the binary
# and checks what SEEK needs.
#
# Usage:
#   ./install.sh                install the binary and the folders
#   ./install.sh --from-source  build and install the binary from the source
#   ./install.sh --no-binary    only the folders and the built-in files
#   ./install.sh --dry-run      show what would happen, change nothing
#
# On Linux and macOS an interactive terminal gets the sweeping logo and a spinner for the long
# steps. Piped output, NO_COLOR, CI or TERM=dumb give the plain, quiet path.

set -eu

SEEK_HOME="${SEEK_HOME:-$HOME/seek}"
BIN_DIR="$HOME/.local/bin"
REPO_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
FROM_SOURCE=0
INSTALL_BINARY=1
DRY_RUN=0

# Strip the trailing slash macOS puts on TMPDIR, so the paths below never have a double one.
TMP_ROOT="${TMPDIR:-/tmp}"
TMP_ROOT="${TMP_ROOT%/}"

# The logo and the spinner are plain on a pipe, in CI, with NO_COLOR, or on a dumb terminal.
OS_NAME=$(uname -s)
case "$OS_NAME" in
Linux) PLATFORM="Linux" ;;
Darwin) PLATFORM="macOS" ;;
*) PLATFORM="$OS_NAME" ;;
esac
ANIMATE=0
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ] && [ -z "${CI:-}" ] && [ "${TERM:-}" != "dumb" ]; then
	ANIMATE=1
fi

usage() {
	awk 'NR >= 3 && NR <= 15 { sub(/^# ?/, ""); print }' "$0"
}

for argument in "$@"; do
	case "$argument" in
	--from-source) FROM_SOURCE=1 ;;
	--no-binary) INSTALL_BINARY=0 ;;
	--dry-run) DRY_RUN=1 ;;
	-h | --help)
		usage
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

# The script resolves the repository from its own path, so it works from any working directory.
# A piped or copied script has no repository beside it: clone it and re-run there.
repo_url="${SEEK_INSTALL_REPO:-https://github.com/piskevalee-cpu/Seek.git}"
if [ ! -f "$REPO_DIR/crates/seek/Cargo.toml" ]; then
	if [ "${SEEK_INSTALL_CLONED:-0}" = "1" ]; then
		echo "error: the cloned repository has no crates/seek" >&2
		exit 1
	fi
	if [ "$DRY_RUN" = "1" ]; then
		say "==> dry run: would clone $repo_url and re-run the installer there"
		say "==> dry run: nothing was changed"
		exit 0
	fi
	if ! command -v git >/dev/null 2>&1; then
		echo "error: git is needed to fetch the SEEK repository." >&2
		exit 1
	fi
	clone_dir="$TMP_ROOT/seek-install-$$"
	rm -rf "$clone_dir"
	say "==> the installer was run without the repository; cloning it on $PLATFORM"
	if ! git clone --depth 1 "$repo_url" "$clone_dir"; then
		echo "error: could not clone $repo_url" >&2
		exit 1
	fi
	status=0
	SEEK_INSTALL_CLONED=1 sh "$clone_dir/install.sh" "$@" || status=$?
	rm -rf "$clone_dir"
	exit "$status"
fi

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
	if [ "$ANIMATE" = "1" ]; then
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

# Run a step, with a spinner while it works on an interactive terminal. The child's output is
# kept and shown only when the step fails, so the spinner never gets smeared by a build log.
spin() {
	label=$1
	shift
	if [ "$ANIMATE" = "1" ]; then
		log="$TMP_ROOT/seek-install-spin-$$"
		(
			while :; do
				for glyph in '⠋' '⠙' '⠹' '⠸' '⠼' '⠴' '⠦' '⠧' '⠇' '⠏'; do
					printf '\r  %s %s' "$glyph" "$label"
					sleep 0.08
				done
			done
		) &
		spinner_pid=$!
		status=0
		"$@" >"$log" 2>&1 || status=$?
		kill "$spinner_pid" 2>/dev/null || true
		wait "$spinner_pid" 2>/dev/null || true
		if [ "$status" = "0" ]; then
			printf '\r  ✓ %s\n' "$label"
		else
			printf '\r  ! %s\n' "$label"
			cat "$log" >&2
		fi
		rm -f "$log"
		return "$status"
	fi
	say "  $label"
	status=0
	"$@" || status=$?
	return "$status"
}

banner
say ""
say "==> SEEK on $PLATFORM"
if [ "$DRY_RUN" = "1" ]; then
	say "    dry run: no files will be changed"
fi
say ""

# ---------------------------------------------------------------- the folders

say "==> the folders of $SEEK_HOME"
for folder in skills tools models widgets state; do
	if [ "$DRY_RUN" = "1" ]; then
		say "    would create $SEEK_HOME/$folder"
	else
		mkdir -p "$SEEK_HOME/$folder"
		say "    $SEEK_HOME/$folder"
	fi
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
	if [ "$DRY_RUN" = "1" ]; then
		say "    would copy $target"
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

build_binary() {
	cargo install --path "$REPO_DIR/crates/seek" --root "$HOME/.cargo" --force &&
		cp "$HOME/.cargo/bin/seek" "$BIN_DIR/seek"
}

download_binary() {
	curl -fsSL "$url" -o "$archive" &&
		tar -xzf "$archive" -C "$TMP_ROOT" &&
		cp "$TMP_ROOT/$staging/seek" "$BIN_DIR/seek" &&
		rm -rf "$archive" "$TMP_ROOT/$staging"
}

if [ "$INSTALL_BINARY" = "1" ]; then
	say "==> the binary"
	if [ "$DRY_RUN" = "1" ]; then
		if [ "$FROM_SOURCE" = "1" ]; then
			say "    would build the source and install $BIN_DIR/seek"
		else
			version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO_DIR/Cargo.toml" | head -1)
			case "$(uname -m)" in
			x86_64 | amd64) arch=x86_64 ;;
			aarch64 | arm64) arch=aarch64 ;;
			*) arch=$(uname -m) ;;
			esac
			case "$OS_NAME" in
			Linux) target="${arch}-unknown-linux-gnu" ;;
			Darwin) target="${arch}-apple-darwin" ;;
			*) target="${arch}-$(printf '%s' "$OS_NAME" | tr '[:upper:]' '[:lower:]')" ;;
			esac
			say "    would download seek-v${version}-${target}.tar.gz to $BIN_DIR/seek"
			say "    falling back to a source build if that release is missing"
		fi
	else
		mkdir -p "$BIN_DIR"
		if [ "$FROM_SOURCE" = "1" ]; then
			spin "build from the source" build_binary
		else
			version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO_DIR/Cargo.toml" | head -1)
			case "$(uname -m)" in
			x86_64 | amd64) arch=x86_64 ;;
			aarch64 | arm64) arch=aarch64 ;;
			*) arch=$(uname -m) ;;
			esac
			case "$OS_NAME" in
			Linux) target="${arch}-unknown-linux-gnu" ;;
			Darwin) target="${arch}-apple-darwin" ;;
			*) target="${arch}-$(printf '%s' "$OS_NAME" | tr '[:upper:]' '[:lower:]')" ;;
			esac
			staging="seek-v${version}-${target}"
			archive="$TMP_ROOT/${staging}.tar.gz"
			url="https://github.com/piskevalee-cpu/Seek/releases/download/v${version}/${staging}.tar.gz"
			if ! spin "download the release for $target" download_binary; then
				say "    no release for this platform, building from the source"
				spin "build from the source" build_binary
			fi
		fi
		chmod +x "$BIN_DIR/seek"
		say "    $BIN_DIR/seek"
	fi
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
if [ "$DRY_RUN" = "1" ]; then
	say "Dry run complete. Nothing was changed."
else
	say "SEEK is installed on $PLATFORM. Next:"
	say "    export PATH=\"\$HOME/.local/bin:\$PATH\""
	say "    seek"
	say ""
	say "The configuration lives in $SEEK_HOME."
	if [ "$missing" = "1" ]; then
		say "Some programs are missing; see the list above."
	fi
fi
