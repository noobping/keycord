# Keycord architecture guard

This zero-dependency tool protects repository boundaries that Cargo and the Rust
type system do not express. It is a workspace member but not a default member, so
it can run without building Keycord's application dependencies or installing GTK.

Run the complete guard from the repository root:

```sh
cargo run --package keycord-architecture --offline -- .
```

The check enforces:

- every `crates/keycord-*` package is a workspace member, is private, and has an
  empty default feature set;
- internal workspace dependencies disable default features;
- FIDO and Passkey do not depend on one another, and Lifecycle has no normal
  dependency on Passkey;
- every subject UI fragment lives under its owner's `data` directory, has exactly
  one reachable composition marker, and appears in the deterministic registry in
  composition order (including intentionally nested markers);
- the Shell skeleton exposes only the generic IDs, actions, and translatable text
  listed in `policy/shell-ui-*.txt`;
- subject Rust and UI may hard-code only window actions they own or are explicitly
  allowed to consume by `policy/window-action-owners.txt`;
- root composition does not recreate pure `keycord-*` re-export facades or a flat registry of
  subject-owned builder widgets;
- Runtime does not regain compile-time probes for Docs, Git, FIDO, Keys, Stores, or Lifecycle;
- retired root compatibility paths and root `support`/`tools` catchalls are not
  recreated; and
- substantial production Rust files and function bodies are not exact copies.

`policy/legacy-root-catchalls.txt` is intentionally empty. If a temporary bridge is
unavoidable, list its exact file path there; broad directory exceptions are not
supported. Remove the line when the bridge disappears. Shell inventory changes
similarly require an explicit policy update, making the ownership decision visible
in review.

Window-action policy lines use `OWNER ACTION-PATTERN [ALLOWED-CONSUMERS]`.
Owners automatically have access. Additional consumers are comma-separated; use
`all` only for genuinely generic Shell actions such as `back` and `go-home`. A
single trailing `*` covers generated families such as `open-store-git-*`. The
guard inventories `register_window_action` calls, qualified `win.*` references,
and known bare action strings in production Rust, plus action properties in UI
fragments. Root composition can wire across subjects, but its registrations must
still be declared. Test files, test modules, and test functions are ignored.

The duplicate detector is deliberately conservative: it checks byte-identical
files and substantial byte-identical function bodies, ignores test modules and
test files, and excludes field-only struct-literal adapters. It does not claim to
find semantic clones with different syntax. Dependency checks cover declared
direct Cargo edges; Cargo remains responsible for validating the complete graph.
