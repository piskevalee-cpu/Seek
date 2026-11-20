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
# On an interactive Linux or macOS terminal the installer prints the SEEK logo and asks what to
# do with a menu you navigate with the arrow keys. With a flag, or on a pipe, it just runs.

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

OS_NAME=$(uname -s)
case "$OS_NAME" in
Linux) PLATFORM="Linux" ;;
Darwin) PLATFORM="macOS" ;;
*) PLATFORM="$OS_NAME" ;;
esac

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

# ---------------------------------------------------------------- the logo

# The complete 25 x 9 bitmap the TUI draws, in the terminal palette. No animation: the logo is
# printed once, with every lit cell at the highlight colour.
print_logo() {
	awk -v plain="${1:-0}" '
	{
		rows[NR] = $0
		while (length(rows[NR]) < 25) rows[NR] = rows[NR] " "
	}
	END {
		esc = sprintf("%c", 27)
		for (r = 1; r <= NR; r++) {
			line = ""
			for (c = 1; c <= 25; c++) {
				ch = substr(rows[r], c, 1)
				if (ch == " ") { line = line "  "; continue }
				if (ch == "#") { rr = 214; gg = 193; bb = 240 }
				else { rr = 47; gg = 43; bb = 60 }
				if (plain) {
					line = line (ch == "#" ? "██" : "░░")
				} else {
					line = line esc "[38;2;" rr ";" gg ";" bb "m██"
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

# ---------------------------------------------------------------- the menu

# Read one key, turning escape sequences into names. Works in the POSIX sh of Linux and macOS.
read_key() {
	old_stty=$(stty -g 2>/dev/null) || return 0
	stty -echo -icanon min 1 time 0 2>/dev/null || {
		stty "$old_stty" 2>/dev/null || true
		return 0
	}
	key=$(dd bs=1 count=1 2>/dev/null) || key=''
	if [ "$key" = "$(printf '\033')" ]; then
		rest=$(dd bs=1 count=2 2>/dev/null) || rest=''
		case "$rest" in
		'[A') key=UP ;;
		'[B') key=DOWN ;;
		'[C') key=RIGHT ;;
		'[D') key=LEFT ;;
		esac
	fi
	stty "$old_stty" 2>/dev/null || true
	printf '%s' "$key"
}

# A mode is one of all, files, binary, dry. A flag on the command line preselects it.
MODE=all
if [ "$DRY_RUN" = "1" ]; then
	MODE=dry
elif [ "$INSTALL_BINARY" = "0" ]; then
	MODE=files
fi

menu() {
	labels=$(printf '%s\n' \
		"Full install (folders, files and binary)" \
		"Folders and built-in files only" \
		"Binary only" \
		"Dry run (change nothing)" \
		"Cancel")
	case "$MODE" in
	dry) selected=4 ;;
	files) selected=2 ;;
	binary) selected=3 ;;
	*) selected=1 ;;
	esac
	count=5
	say "==> what should SEEK do on $PLATFORM?"
	say ""
	while :; do
		index=1
		while [ "$index" -le "$count" ]; do
			label=$(printf '%s\n' "$labels" | sed -n "${index}p")
			if [ "$index" = "$selected" ]; then
				say "  ❯ $label"
			else
				say "    $label"
			fi
			index=$((index + 1))
		done
		say ""
		say "  ↑↓ move   ·   Enter select"
		key=$(read_key)
		case "$key" in
		UP)
			selected=$((selected - 1))
			[ "$selected" -lt 1 ] && selected=$count
			;;
		DOWN)
			selected=$((selected + 1))
			[ "$selected" -gt "$count" ] && selected=1
			;;
		'')
			break
			;;
		q | Q)
			selected=5
			break
			;;
		*) continue ;;
		esac
		printf '\033[%sA' $((count + 2))
	done
	say ""
	case "$selected" in
	1) MODE=all ;;
	2) MODE=files; INSTALL_BINARY=0 ;;
	3) MODE=binary ;;
	4) MODE=dry; DRY_RUN=1 ;;
	5)
		say "Cancelled. Nothing was changed."
		exit 0
		;;
	esac
}

# The menu is for a person at a keyboard; flags, pipes and CI go straight to the steps.
if [ -t 0 ] && [ -z "${CI:-}" ]; then
	print_logo "$( [ -z "${NO_COLOR:-}" ] && [ "${TERM:-}" != "dumb" ] && echo 0 || echo 1 )"
	say ""
	menu
else
	if [ -z "${NO_COLOR:-}" ] && [ "${TERM:-}" != "dumb" ]; then
		print_logo
	else
		print_logo 1
	fi
	say ""
	say "==> SEEK on $PLATFORM"
	if [ "$DRY_RUN" = "1" ]; then
		say "    dry run: no files will be changed"
	fi
	say ""
fi

# ---------------------------------------------------------------- the steps

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

if [ "$MODE" != "binary" ]; then
	say "==> the folders of $SEEK_HOME"
	for folder in skills tools models widgets state; do
		if [ "$DRY_RUN" = "1" ]; then
			say "    would create $SEEK_HOME/$folder"
		else
			mkdir -p "$SEEK_HOME/$folder"
			say "    $SEEK_HOME/$folder"
		fi
	done

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
fi

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
			build_binary || { echo "error: the source build failed" >&2; exit 1; }
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
			if ! download_binary; then
				say "    no release for this platform, building from the source"
				build_binary || { echo "error: the source build failed" >&2; exit 1; }
			fi
		fi
		chmod +x "$BIN_DIR/seek"
		say "    $BIN_DIR/seek"
	fi
fi

if [ "$MODE" != "binary" ]; then
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
fi

# ----------------------------------------------------------------- the end

say ""
if [ "$DRY_RUN" = "1" ]; then
	say "Dry run complete. Nothing was changed."
else
	say "SEEK is installed on $PLATFORM. Next:"
	say "    export PATH=\"\$HOME/.local/bin:\$PATH\""
	say "    seek"
	say ""
	say "The configuration lives in $SEEK_HOME."
	if [ "${missing:-0}" = "1" ]; then
		say "Some programs are missing; see the list above."
	fi
fi
