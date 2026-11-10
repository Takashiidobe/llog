# llog

A wiki for your llm.

## Commands

Initialize a wiki with `llog init`.

Create a dated work log with text or load the entry from a file using `@FILE` (use `@-` to read standard input):

```text
llog new "standup" "Finished the parser change" [--date YYYY-MM-DD]
llog new "standup" @notes.md [--date YYYY-MM-DD]
```

Create a concept file with:

```text
llog wiki "Rust ownership" [entry]
llog wiki "Rust ownership" @ownership.md
```
