# llog

A wiki for your llm.

## Commands

Initialize a wiki with `llog init`.

Create a dated work log with text or load the entry from a file using `@FILE` (use `@-` to read standard input):

```text
llog new "standup" "Finished the parser change" [--date YYYY-MM-DD]
llog new "standup" @notes.md [--date YYYY-MM-DD]
```

`new` and `wiki` take repeatable `-k KEY -v VALUE` pairs. They are written as a
`- key: value` list under the date line, matched by plain searches, and
filterable exactly. Keys are lowercased letters, digits, `-` and `_`; repeat a
key for several values:

```text
llog new "fix" @notes.md -k bead -v proj-12 -k area -v parser -k area -v lexer
```

Search returns file paths. Concept pages rank above logs, logs decay with a
90-day half-life, and anything under `historical/` ranks last. `-k`/`-v` on
`search` keep only entries whose metadata has that exact value (case
insensitive); the query may be empty when filtering:

```text
llog search "query" [--limit N] [--plain] [-k KEY -v VALUE]...
```

Create a concept file with:

```text
llog wiki "Rust ownership" [entry]
llog wiki "Rust ownership" @ownership.md
```
