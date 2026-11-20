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
# On Linux and macOS an interactive terminal gets a small TUI: the sweeping logo keeps animating
# for the whole install, the steps light up one by one, and the last frame completes the logo.
# Piped output, NO_COLOR, CI or TERM=dumb give the plain, quiet path.

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

# The TUI is plain on a pipe, in CI, with NO_COLOR, or on a dumb terminal.
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

# ---------------------------------------------------------------- the renderer

# The same 25 x 9 bitmap the TUI draws. A diagonal sweep runs from the top right to the low left.
# The third argument paints the completed logo, all lit pixels at full highlight.
print_logo_frame() {
	awk -v frame="$1" -v plain="${2:-0}" -v complete="${3:-0}" '
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
				if (complete) {
					t = (ch == "#") ? 1 : 0.30
				} else {
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
				}
				if (t > 1) t = 1
				if (ch == "#") { br = 120; bg = 116; bb = 133; hr = 214; hg = 193; hb = 240 }
				else { br = 47; bg = 43; bb = 60; hr = 160; hg = 154; hb = 176 }
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

FRAME=0
LOGO_COMPLETE=0
STEP_TOTAL=5
CURRENT_STEP=0
STEP_STATUS='00000'
STEP_LABELS=''
DETAIL_FILE="$TMP_ROOT/seek-install-detail-$$"
STEP_LOG="$TMP_ROOT/seek-install-step-$$"
: > "$DETAIL_FILE"

# A line of detail: shown under the step list in the TUI, printed directly on a pipe.
note() {
	if [ "$ANIMATE" = "1" ]; then
		printf '%s\n' "$*" >> "$DETAIL_FILE"
	else
		say "$*"
	fi
}

spinner_glyph() {
	case $((FRAME % 10)) in
	0) printf '⠋' ;;
	1) printf '⠙' ;;
	2) printf '⠹' ;;
	3) printf '⠸' ;;
	4) printf '⠼' ;;
	5) printf '⠴' ;;
	6) printf '⠦' ;;
	7) printf '⠧' ;;
	8) printf '⠇' ;;
	*) printf '⠏' ;;
	esac
}

# Draw one frame: home the cursor, clear to the end of the screen, then the whole interface.
render() {
	[ "$ANIMATE" = "1" ] || return 0
	printf '\033[H\033[J'
	if [ "$LOGO_COMPLETE" = "1" ]; then
		print_logo_frame 0 0 1
	else
		print_logo_frame "$((FRAME % 5))"
	fi
	printf '\n'
	say "==> SEEK on $PLATFORM"
	if [ "$DRY_RUN" = "1" ]; then
		say "    dry run: no files will be changed"
	fi
	printf '\n'
	render_row=1
	while [ "$render_row" -le "$STEP_TOTAL" ]; do
		render_state=$(printf '%s' "$STEP_STATUS" | cut -c "$render_row")
		render_label=$(printf '%s\n' "$STEP_LABELS" | sed -n "${render_row}p")
		case "$render_state" in
		0) render_icon='○' ;;
		1) render_icon=$(spinner_glyph) ;;
		2) render_icon='●' ;;
		*) render_icon='·' ;;
		esac
		say " $render_icon $render_label"
		render_row=$((render_row + 1))
	done
	printf '\n'
	if [ -s "$DETAIL_FILE" ]; then
		tail -3 "$DETAIL_FILE"
	fi
	printf '\n'
	say "   step $CURRENT_STEP of $STEP_TOTAL"
}

# Advance the animation a few frames. A frame is 0.12 s: deliberately slow enough to follow.
pulse() {
	[ "$ANIMATE" = "1" ] || return 0
	count=$1
	n=0
	while [ "$n" -lt "$count" ]; do
		FRAME=$((FRAME + 1))
		render
		sleep 0.12
		n=$((n + 1))
	done
}

set_status() {
	set_pos=$1
	set_char=$2
	set_before=''
	if [ "$set_pos" -gt 1 ]; then
		set_before=$(printf '%s' "$STEP_STATUS" | cut -c "1-$((set_pos - 1))")
	fi
	set_after=$(printf '%s' "$STEP_STATUS" | cut -c "$((set_pos + 1))-" || true)
	STEP_STATUS="$set_before$set_char$set_after"
}

# Mark a step active, animate while its command runs, then mark it done.
run_step() {
	step_index=$1
	step_label=$2
	shift 2
	CURRENT_STEP=$step_index
	: > "$DETAIL_FILE"
	set_status "$step_index" 1
	pulse 2
	status=0
	if [ "$ANIMATE" = "1" ]; then
		# A background animator redraws the frame while the step runs in the foreground. The
		# step's output goes to a log, so nothing smears the frame, and there is no status file
		# for a subshell to forget to write.
		(
			while :; do
				FRAME=$((FRAME + 1))
				render
				sleep 0.12
			done
		) &
		animator_pid=$!
		"$@" > "$STEP_LOG" 2>&1 || status=$?
		kill "$animator_pid" 2>/dev/null || true
		wait "$animator_pid" 2>/dev/null || true
	else
		"$@" || status=$?
	fi
	if [ "$status" != "0" ] && [ -s "$STEP_LOG" ]; then
		cat "$STEP_LOG" >&2
	fi
	rm -f "$STEP_LOG"
	set_status "$step_index" 2
	pulse 2
	return "$status"
}

STEP_LABELS=$(printf '%s\n' \
	"the folders of $SEEK_HOME" \
	"the built-in skills and widgets" \
	"the binary" \
	"what SEEK needs" \
	"finishing")

banner() {
	if [ "$ANIMATE" = "1" ]; then
		pulse 10
	else
		print_logo_frame 5 1
	fi
}

# ---------------------------------------------------------------- the steps

step_folders() {
	note "==> the folders of $SEEK_HOME"
	for folder in skills tools models widgets state; do
		if [ "$DRY_RUN" = "1" ]; then
			note "    would create $SEEK_HOME/$folder"
		else
			mkdir -p "$SEEK_HOME/$folder"
			note "    $SEEK_HOME/$folder"
		fi
	done
}

copy_if_absent() {
	source="$1"
	target="$2"
	if [ -e "$target" ]; then
		note "    kept   $target"
		return
	fi
	if [ ! -e "$source" ]; then
		return
	fi
	if [ "$DRY_RUN" = "1" ]; then
		note "    would copy $target"
		return
	fi
	mkdir -p "$(dirname -- "$target")"
	cp -R "$source" "$target"
	note "    copied $target"
}

step_builtins() {
	note "==> the built-in skills and widgets"
	copy_if_absent "$REPO_DIR/builtin/SYSTEM.md" "$SEEK_HOME/SYSTEM.md"
	for skill in "$REPO_DIR"/builtin/skills/*; do
		[ -d "$skill" ] || continue
		copy_if_absent "$skill" "$SEEK_HOME/skills/$(basename -- "$skill")"
	done
	for widget in "$REPO_DIR"/builtin/widgets/*; do
		[ -d "$widget" ] || continue
		copy_if_absent "$widget" "$SEEK_HOME/widgets/$(basename -- "$widget")"
	done
}

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

step_binary() {
	note "==> the binary"
	if [ "$INSTALL_BINARY" != "1" ]; then
		note "    skipped (--no-binary)"
		return 0
	fi
	if [ "$DRY_RUN" = "1" ]; then
		if [ "$FROM_SOURCE" = "1" ]; then
			note "    would build the source and install $BIN_DIR/seek"
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
			note "    would download seek-v${version}-${target}.tar.gz to $BIN_DIR/seek"
			note "    falling back to a source build if that release is missing"
		fi
		return 0
	fi
	mkdir -p "$BIN_DIR"
	if [ "$FROM_SOURCE" = "1" ]; then
		build_binary ||
			{ note "    the source build failed"; return 1; }
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
			note "    no release for this platform, building from the source"
			build_binary || { note "    the source build failed"; return 1; }
		fi
	fi
	chmod +x "$BIN_DIR/seek"
	note "    $BIN_DIR/seek"
}

step_checks() {
	note "==> what SEEK needs"
	missing=0
	if command -v git >/dev/null 2>&1; then
		note "    git      $(command -v git)"
	else
		note "    git      MISSING - install git to see the repository in the widgets"
		missing=1
	fi
	if command -v llama >/dev/null 2>&1; then
		note "    llama    $(command -v llama)"
	elif command -v llama-server >/dev/null 2>&1; then
		note "    llama    $(command -v llama-server)"
	else
		note "    llama    MISSING - install llama.cpp to run local models"
		note "             https://github.com/ggml-org/llama.cpp"
		missing=1
	fi
	if [ -d "$SEEK_HOME/models" ] && [ -n "$(ls -A "$SEEK_HOME/models" 2>/dev/null)" ]; then
		note "    models   $(ls "$SEEK_HOME/models" | head -3 | tr '\n' ' ')"
	else
		note "    models   none yet - put a .gguf file in $SEEK_HOME/models"
	fi
	MISSING=$missing
}

step_finish() {
	if [ "$DRY_RUN" = "1" ]; then
		note ""
		note "Dry run complete. Nothing was changed."
	else
		note ""
		note "SEEK is installed on $PLATFORM. Next:"
		note "    export PATH=\"\$HOME/.local/bin:\$PATH\""
		note "    seek"
		note ""
		note "The configuration lives in $SEEK_HOME."
		if [ "$MISSING" = "1" ]; then
			note "Some programs are missing; see the list above."
		fi
	fi
}

# ------------------------------------------------------------------- the run

banner

# The TUI renderer draws these itself; on a pipe they are printed once here.
if [ "$ANIMATE" = "0" ]; then
	say "==> SEEK on $PLATFORM"
	if [ "$DRY_RUN" = "1" ]; then
		say "    dry run: no files will be changed"
	fi
	say ""
fi

run_step 1 "the folders of $SEEK_HOME" step_folders
run_step 2 "the built-in skills and widgets" step_builtins
run_step 3 "the binary" step_binary
run_step 4 "what SEEK needs" step_checks
run_step 5 "finishing" step_finish

# The last frame completes the logo and holds, so the install ends on a finished mark.
LOGO_COMPLETE=1
render

rm -f "$DETAIL_FILE" "$STEP_LOG"
