---
name: init
description: Write an AGENTS.md for the repository, so that agents know how the project works. Use when the user asks for an AGENTS.md, a CLAUDE.md or instructions for agents.
---

# Init

Write an `AGENTS.md` at the root of the repository. It is the file every agent reads first, so
it has to be short and about the things that are not obvious from the code.

## What to do

1. **Look around.** Read the files that say how the project is built and run: the readme, the
   manifests, the task runner, the CI workflows and the lint and formatter configuration. Read
   the scripts, not the whole tree.
2. **Find the commands that are not written down.** The exact way to build, to test, to lint and
   to run the project. Take them from the files, never from memory.
3. **Find the conventions.** How the code is laid out, how the modules are named, what the
   architecture is. Note the parts that a new agent would get wrong.
4. **Write the file.** Keep it under about a hundred lines.

## What belongs in it

- The exact commands: build, test, lint, format, and how to run a single test.
- The layout of the project and what belongs where.
- The conventions that differ from the defaults of the language or the framework.
- The traps: the generated files, the migrations, the fixtures, the services a test needs.

## What does not belong in it

- What the language or the framework already explains.
- A file tree, unless the structure is the thing that is not obvious.
- Anything that is speculative: write down what the files say, not what you expect.

Write for an agent that has never seen the repository. If a line would not change what that
agent does, leave it out.
