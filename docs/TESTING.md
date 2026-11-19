# The manual test checklist

How to go through SEEK by hand and check that everything works. Every item says what to do and
what to expect.

An agent that changes the code should walk this list. It is the fastest way to find out that
something broke which only shows up when a person uses it: the layout, the keyboard, the colours
and the flow of a conversation are not covered by the unit tests.

## Before you start

- [ ] **Build it.** `cargo build --release` — no errors, no warnings.
- [ ] **Check it.** `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` — both clean.
- [ ] **Run the tests.** `cargo test --workspace` — every test passes.
- [ ] **Look at the screens without a terminal.** `cargo run -p seek-tui --example screenshot` — the home screen, the chat screen, the sidebar and the picker are printed.
- [ ] **Put a model where SEEK can find it.** `ln -s ~/LLMs/<your-model>.gguf ~/seek/models/` — `seek --doctor` lists it under "local models".
- [ ] **Check the doctor.** `seek --doctor` — it names the home folder, the providers, the local models, `git` and the `llama` binary.
- [ ] **Have a cloud key ready, if you want to test the cloud.** `export ANTHROPIC_API_KEY=...` — without a key SEEK falls back to the scripted provider, which is useful to know but must not be mistaken for the real thing.

Run the TUI in a terminal of **at least 80x24**. Everything below is done in that terminal.

## The first run

- [ ] **Start it.** `seek` — the logo appears in the middle of the screen, the widgets below it, the prompt bar at the bottom, the footer at the very bottom.
- [ ] **The logo.** It is the mark: the plus symbol on the left, `SEEK` on the right. It is drawn with blocks, two columns per pixel.
- [ ] **The logo has no frame.** Only the lit pixels are drawn: the background of the mark is the background of the terminal, not a block of colour.
- [ ] **The logo is centred.** Horizontally and vertically in the home screen.
- [ ] **Thinking.** While the model works, a labelled `thinking` animation sits just after the RAM in the footer; open the sidebar and it moves into the sidebar's metrics. The chat keeps every row.
- [ ] **The sheen.** A shine sweeps across the logo from the top right to the low left. With reduced motion it rests.
- [ ] **The system prompt.** `~/seek/SYSTEM.md` exists after install and begins "You are SEEK". Edit a line, restart SEEK — the model follows the edited prompt. Delete the file and SEEK falls back to the built-in default.
- [ ] **The project prompt.** A `.seek/SYSTEM.md` beside the project's root wins over the user's file; `.seek/config.toml` overrides only the keys it sets. `seek doctor` names the file in use.
- [ ] **Command output.** Ask the agent to run a command and then what it said: it must quote the real output (or `(no output)`), not re-run the command because it "did not execute".
- [ ] **The widgets.** The `git` widget shows the branch and the recent commits; the `usage` widget shows the tokens used so far. A widget that fails shows its title and a dash.
- [ ] **No model at the start.** The chat is there from the first frame: the footer and the top border of the prompt say `[no model selected]`. No picker opens on its own.
- [ ] **A message needs a model.** Type "hello" and press Enter — the transcript says to choose a model with `/model`, and the draft stays.
- [ ] **The model selector.** Type `/model` and press Enter. A bordered panel like the sidebar's opens just above the prompt: `┌ models ─┐`, grouped section headers, compact `❯` rows with the model name, `LOCAL` badges and the model id on the right, and the directional gradient on the picked row. Open the sidebar and the panel stays in the main column beside it; putting the sidebar on the other side flips the gradient. Typing filters the list and the query appears in the title.
- [ ] **Choose a model.** Press Enter — the picker closes, the footer shows the model.
- [ ] **The footer.** One line: the mode chip (`ASK`), the model, the working directory, the branch and the metrics of the session. The metrics are not on a line of their own.
- [ ] **Shift+Tab switches the mode.** Press it — the chip changes, whatever the sidebar does.
- [ ] **The mode applies at once.** While the agent works, press Shift+Tab to `YOLO`: the next tool call of that turn runs without asking.

## The prompt bar

- [ ] **The prompt is a box.** The input sits in a box with rounded corners. The name of the model is in the top border, the keys and the status in the bottom one.
- [ ] **Command completion.** Type `/th` — the list above the bar holds `/think` and `/thoughts`. Down moves to the second, Tab takes it, Esc closes the list and typing brings it back.
- [ ] **File completion.** Type "read src/ma" — the list shows `src/main.rs`; Tab inserts it and keeps the prose before it.
- [ ] **Reference completion.** Type "look at @model" in the project root — the list finds `src/model.rs` anywhere below it; Enter inserts a file path without sending the message.
- [ ] **A directory.** Complete `@src` to `@src/` — the list moves into the directory.
- [ ] **The bar holds the text and nothing else.** While the agent works, the status is in the bottom border, the box keeps its shape and the cursor is the only thing that moves in it.
- [ ] **The guards.** Give the model a task that makes it think: it is interrupted with "Act now, not further planning" and it calls a tool instead of thinking for ever.
- [ ] **The read numbers the lines.** Ask the model to read a file: every line comes back as `12:the line`.
- [ ] **The edit by line.** "In the file, replace line 2 with ..." — the file changes, and the answer says which line.
- [ ] **The intent.** The tool row shows what the model said it was doing.
- [ ] **The commands complete.** Type `/` — a dropdown of the slash commands and the skills appears, with a column of the descriptions and the picked one marked. The arrows move it, Tab takes it, Enter takes it and runs it.
- [ ] **The name.** On the first run the bar asks for your name. Type it and press Enter — the model calls you by it. `/settings` asks again.

- [ ] **Type.** Every character appears, and the cursor is a block behind the last one.
- [ ] **Send.** Press Enter — the message appears in the transcript as a `USER` box.
- [ ] **A newline.** Paste a multi-line draft — the box grows to show every line; there is no newline key.
- [ ] **A pasted path.** Copy a file path (or `file://` URL) and paste it — it becomes an `@` pin with the full path, and the chip row shows it.
- [ ] **Grok-style keys.** Ctrl+Backspace deletes a word; Ctrl+Shift+V pastes raw text; Ctrl+V pastes a clipboard image as a pin under `state/pastes/`.
- [ ] **The history.** Press Up — the last message you sent comes back. Press Down — it goes away again.
- [ ] **The suggestions.** Type `/` — a list of the slash commands appears above the bar. Type `/mod` — the list narrows to `/model` and `/mode`.
- [ ] **A backspace.** It deletes one character, not one byte: a character with an accent is gone in one press.

## Pinned paths

- [ ] **The chip.** Type "read @Cargo.toml" — a `pinned @Cargo.toml` line appears above the prompt. Delete the mention and the chip goes away.
- [ ] **A directory.** Type `@docs` — the chip shows the path, and the agent gets the first names of the directory in its context, not their contents.
- [ ] **Quoted spaces.** Type `@"two words.txt"` for a file with a space — the path resolves and is pinned.
- [ ] **A miss.** Type `@does-not-exist` — no chip appears and the text stays prose.
- [ ] **The inspection.** Ask about a pinned file — the agent reads it with `read`; in `ASK` mode a write to it still asks.

## Scrolling and the side bar

- [ ] **The bar.** With a long conversation, a one-column bar appears at the right of the transcript; its thumb is a fraction of the height and the rest is a thin track. The old `↑ N of total` badge is gone.
- [ ] **The wheel.** Scroll up — the line you were reading stays on screen while the model answers. Scroll back to the bottom — new answers follow again.
- [ ] **The keys.** Shift+Up and Shift+Down move five lines, PgUp/PgDn a page, Home/End the ends when the prompt is empty.
- [ ] **The mouse bar.** Click above or below the thumb to page; click the thumb and drag it — the transcript follows.
- [ ] **Move the sidebar.** `/settings` → "Sidebar side" → press → to change it to right. The sidebar moves to the right and the scrollbar moves to the left at once. Restart SEEK: the choice is kept in `state/user.toml`.

## The conversation

- [ ] **Ask something.** "Say hello in three words." — the answer streams in, word by word, in a `SEEK` box.
- [ ] **The reasoning.** A model that thinks shows its reasoning above the answer, dimmed, behind a vertical bar. Press Alt+T — the reasoning collapses to one row that says "reasoning". Press Alt+T again — it comes back.
- [ ] **Your message is a band.** The prompt of the user is a band across the width with the arrow in front of it. The answer of the model is plain text, without a band.
- [ ] **Every block has a rail.** A one-column rail on the left of every block: blue for your message, magenta for the answer, grey for the reasoning and the tools.
- [ ] **The blocks breathe.** A blank line sits between the blocks, and a gap of two columns sits between the sidebar and the messages.
- [ ] **The metrics.** While the model answers, the footer line shows the context (`1.6k / 32.8k`), the percentage, the speed in tokens per second, the think time and the vitals (cpu, ram).
- [ ] **The animation.** While the model works, a line of dark cells shows at the tail of the transcript, with a streak of brightness going through them. It is one line, eight cells, and the streak wraps around: a new one starts at the left when the old one reaches the right.
- [ ] **A long answer.** Ask for a long text — the transcript scrolls, and the view stays at the bottom.
- [ ] **Scroll up.** Press PageUp — the transcript scrolls up, and the newest lines are no longer glued to the bottom. PageDown scrolls back.
- [ ] **The chat scrolls.** The wheel scrolls three lines at a time, PageUp and PageDown a page. The place in the transcript is shown in the corner while you are not at the bottom.
- [ ] **The window holds.** Scroll up while the model writes: the message you are reading stays where it is.
- [ ] **The log strip.** With a local model, one line of the server appears above the prompt bar: "loading model", "server is listening on ...". The strip hides itself after ten seconds of silence and comes back with the next event.

## The keys

Every key of the specification, one by one.

- [ ] **Enter** — sends the message.
- [ ] **Enter** — sends the message, including with Shift held.
- [ ] **Esc** — closes an overlay. With a turn running, it stops the turn.
- [ ] **Shift+Tab** — cycles the mode: `READ`, `ASK`, `YOLO`. The chip in the footer changes colour with it.
- [ ] **Ctrl+B** — opens and closes the sidebar.
- [ ] **Ctrl+O** — expands the selected tool row, and collapses it again.
- [ ] **Ctrl+C** — clears the input. Press it twice on an empty input — SEEK quits.
- [ ] **Ctrl+T** — cycles the reasoning level of the model: `off`, `low`, `medium`, `high`, `max`. The chip next to the model name shows it. A model without levels says that reasoning is not configurable for it.
- [ ] **Alt+T** — shows and hides the reasoning.
- [ ] **PageUp / PageDown** — scroll the transcript.
- [ ] **Tab** — with the sidebar open, moves the focus into it. Press Tab again to get back.

## The permissions

- [ ] **Ask for a write.** "Create a file called hello.txt with the word hi in it." — the prompt bar turns into a question with three choices: **Yes**, **No**, **Tell SEEK what to do**. The chosen answer is highlighted, and the line under it says which keys do what.
- [ ] **The answer arrives.** Say yes: the tool runs at once. Before, the answer never arrived and the tool stayed `running`.
- [ ] **`y` and `n`.** In the permission selector, `y` says yes and `n` says no.
- [ ] **Esc, then a message.** Press Esc while the model works, then type a message and press Enter: the message goes to the model. Before, nothing happened at all.
- [ ] **The keys arrive at once.** Type while the model is working: the letters appear in the box as you type them, without a pause.
- [ ] **Yes.** The file is written and the tool row shows `ok` with the time it took.
- [ ] **No.** The tool row shows the refusal, and the model is told that the user said no.
- [ ] **Tell SEEK what to do.** Choose the third choice, type "use hello.md instead", press Enter — the model gets that text as the result of the tool call.
- [ ] **Read mode.** Press Shift+Tab until the chip says `READ` — a write is denied without asking.
- [ ] **Yolo mode.** Press Shift+Tab until the chip says `YOLO` — the write goes through without asking.
- [ ] **A path outside the project.** Ask it to write to `/tmp/whatever.txt` — in `ASK` mode it asks, whichever tool writes or runs.
- [ ] **A read outside the project.** "Read the first lines of `/tmp/whatever.txt`." — in `ASK` mode the read runs without a question, wherever the file is.

## The tools

- [ ] **The four tools and the search.** `/tools` lists `read`, `write`, `edit`, `bash` and `web_search` as checked, and `ls`, `grep`, `find` and `fetch` as unchecked.
- [ ] **Turn one off.** Open `/tools`, move to `bash`, press Space — the check disappears. Close the picker: the next turn has no `bash` tool.
- [ ] **Turn one on.** Do the same with `grep` — now it is on.
- [ ] **Read a file.** "Read the first ten lines of Cargo.toml." — the tool row shows the path, the result is the content.
- [ ] **Edit a file.** "Change the description in Cargo.toml to something else." — the file changes and the row says `ok`.
- [ ] **A long output.** "Read the whole of Cargo.lock." — the output is cut with a marker that says how much was left out and how to fetch it.
- [ ] **Bash.** "Run `cargo --version`." — the row shows the command and the output.
- [ ] **The processes.** With a command running, Ctrl+B shows it in the sidebar under "Processes", with its pid and how long it has been running. Press `x` on it to kill it.

## The web search

- [ ] **On by default.** `/tools` lists `web_search` checked; leave the picker without touching it.
- [ ] **A search without a URL.** In `ASK` mode, "What is the latest version of Rust?" — the search runs without a permission question, and the answer lists numbered sources with their address.
- [ ] **The operators.** "Search for `site:rust-lang.org async`." — the results stay on `rust-lang.org`; a constraint the engine could not honour is reported in a `Note:` line instead of an empty answer.
- [ ] **The fallback.** Put `providers = ['mojeek', 'duckduckgo']` under `[web_search]` in `~/seek/config.toml` — a Mojeek failure is named in a `Note:` line and the answer comes from DuckDuckGo.
- [ ] **The public fan-out.** Set `providers = ['public']` — the results merge the engines that answer, with the pages found by more than one engine first.
- [ ] **A keyed provider.** With `BRAVE_API_KEY` in the environment, set `providers = ['brave']` — the results come from Brave. Without the key the chain says that `BRAVE_API_KEY` is not set.
- [ ] **SearXNG.** Set `searxng_url = 'https://<your-instance>'` and `providers = ['searxng']` — the instance answers its JSON API.

## The models

- [ ] **The picker.** `/model` lists the models of the configured providers, grouped by provider, and the local ones with a `LOCAL` tag.
- [ ] **Filter.** Type `sonnet` — the list narrows.
- [ ] **Switch.** Choose a cloud model — the footer follows, and the context bar is recalculated for the new window.
- [ ] **The launch options.** With a local model selected in the picker, press the right arrow — the launch options open. The first line is read-only and shows the command SEEK runs; the second line is yours.
- [ ] **Save the options.** Type `--ctx-size 8192`, press Enter — the options are saved next to the model and used the next time the server starts.
- [ ] **The reasoning level.** `/think` turns the reasoning off and on again; `/think high` sets it to high. The level belongs to the model: switch to another model and back, and it is still there.
- [ ] **A model without levels.** A model whose catalog entry declares no variants shows no reasoning chip, and Ctrl+T says that reasoning is not configurable for it.

## The sessions

- [ ] **The gradient.** The selected session row and the selected menu row shade from dark to blue; with the sidebar on the right the gradient runs the other way.
- [ ] **The sidebar.** Ctrl+B — the sessions are at the top, but only the last three are shown, with their title and how old they are. The first message lines up with "Sessions" inside the sidebar. Up and Down select one, Enter resumes it.
- [ ] **All sessions.** `/sessions` (or `/history`) opens the full list; Enter resumes the selected one.
- [ ] **A new session.** `/new` — the transcript is empty again.
- [ ] **Resume.** `/resume` — the sidebar opens on the sessions. Move with Up and Down, press Enter — the conversation comes back.
- [ ] **The focus.** With the sidebar open, Tab moves the focus into it; the selected session is highlighted. Tab again moves it back.
- [ ] **The file.** `~/seek/state/sessions/` holds one JSONL file per session: a header, then a line per message and per model switch.
- [ ] **The model switch is in the file.** Switch models in the middle of a conversation: the file says which model said what.
- [ ] **Continue from the shell.** Quit SEEK, run `seek --continue` — the past messages are on screen and the model chip shows the last model used, even from another directory.
- [ ] **History in the app.** `/history` lists past sessions; Enter resumes one and the conversation appears.

## The context

- [ ] **The bar.** Ten cubes that fill up as the context grows, with the used and the total beside it. It turns amber at 80% and red at 95%.
- [ ] **The estimate.** Before the first answer the number has a `~` in front of it: it is an estimate. After an answer it is exact.
- [ ] **Compact.** Fill the context a bit and press `/compact` — the older turns become one summary message.
- [ ] **Truncate.** `/truncate` drops the oldest turns until the history fits.
- [ ] **An overflow.** Switch to a model with a much smaller window while the context is large — the switch is refused and the app says to compact or truncate first.

## The skills

- [ ] **The list.** `/skills` lists the installed skills with a check next to each.
- [ ] **The built-in one.** `init` is there even when it is not installed in `~/seek/skills`.
- [ ] **Run a skill.** Type `/init` and press Enter — the instructions of the skill fill the prompt, and the model follows them.
- [ ] **Turn one off.** `/skills`, Space on a skill, close the picker — it is gone from the prompt.
- [ ] **The catalog.** The prompt that reaches the model carries the name, the description and the path of every enabled skill, and nothing else.

## The providers

- [ ] **The list.** `/provider` lists the configured providers, with their kind and their base URL.
- [ ] **Add one.** Choose "Add a provider": the form opens with the fields for the name, the kind, the base URL, the key environment variable, the models, the context and the rate limits.
- [ ] **Fill it in.** Tab moves from field to field; type the values. The kind has to be `anthropic-messages` or `openai-chat`.
- [ ] **Save it.** Enter — the provider is written to `~/seek/providers.toml` and shows up in the list.
- [ ] **An incomplete form.** Leave the name empty and press Enter — nothing is written and the app says what is missing.
- [ ] **The detected providers.** With no `providers.toml` and `ANTHROPIC_API_KEY` in the environment, the provider is there without any setup.
- [ ] **The keys.** A key can also live in `~/seek/state/auth.json`, under the name of the environment variable it stands in for. The file is readable by its owner only: `ls -l ~/seek/state/auth.json`.
- [ ] **The discovered models.** With an openai-chat provider that answers, the models it knows are in the picker, marked "found on the endpoint". The entries of `providers.toml` win over them.

## The widgets and the overlays

- [ ] **The widgets disappear.** Send a message — the widgets are gone from the home screen.
- [ ] **The help.** `/help` lists every key. Esc closes it.
- [ ] **The small terminal.** Make the terminal smaller than 80x24 — the app says "terminal too small" instead of drawing something broken.
- [ ] **The narrow terminal.** Make it 90 columns — the sidebar hides itself.
- [ ] **The colours.** `SEEK_COLOR=256 seek` — the app still looks right with 256 colours.

## Leaving

- [ ] **Quit.** `/quit` — the app exits and the terminal is the way it was.
- [ ] **The server is stopped.** After quitting, `pgrep -af "llama serve"` finds nothing: SEEK stopped the server it started.
- [ ] **Ctrl+C twice** — the same.

## Looking at the interface as a picture

- [ ] **Dump a screen.** `cargo run -p seek-tui --example dump_screen -- home > /tmp/home.json` and `python3 tools/screen_to_png.py /tmp/home.json /tmp/home.png` — the home screen as a picture. Do the same with `chat` and `permission`.
- [ ] **The picture matches the terminal.** The picture is a layout picture: every character fills its cell. Use it to check the alignment, the bands and the colors, not the font.

## When something is wrong

- [ ] **The log.** `~/seek/state/logs/` holds the log of the local server.
- [ ] **The doctor.** `seek --doctor` says what SEEK found and what is missing.
- [ ] **The session.** `~/seek/state/sessions/*.jsonl` shows exactly what was said.
