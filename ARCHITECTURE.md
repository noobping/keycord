# Architecture

Keycord is a composition binary over subject crates. The root package owns process startup,
feature selection, and the callbacks that connect subjects; reusable behavior and specialized UI
belong to the crate for that subject.

| Crate | Owns |
| --- | --- |
| `keycord-architecture` | zero-dependency repository boundary validation and its explicit policy inventories |
| `keycord-runtime` | diagnostics, commands, workers, localization primitives, bounded configuration, secure files, and process hardening |
| `keycord-shell` | generic GTK helpers, application-level actions, navigation primitives, themes, file pickers, QR rendering, shell assets, and in-app product chrome |
| `keycord-ui-fragments` | dependency-free build-time validation and deterministic composition of declarative UI fragments |
| `keycord-preferences` | settings persistence, settings/search UI, stable settings IDs, and the GSettings schema |
| `keycord-fido` | FIDO security-token transport, PIN setup, protected-key envelopes, token caches, and FIDO-specific UI |
| `keycord-passkey` | passkey credential records and CXF/CXP import, export, request, and UI workflows |
| `keycord-keys` | OpenPGP key material, managed-key storage, protection/unlock flows, smartcards, key caches, and key-management UI |
| `keycord-stores` | password-store roots, paths, recipients, repository-independent store operations, management, and recipient UI |
| `keycord-entries` | pass-entry files, models, generation, OTP, search, import/export, entry operations, and entry UI |
| `keycord-git` | Git repositories, remotes, status, synchronization, audit/signing, and Git-specific UI |
| `keycord-docs` | bundled documentation, locale selection, rendering, search, and documentation UI |
| `keycord-lifecycle` | build/install metadata, packaged branding assets, desktop integration, setup, updates, and search-provider lifecycle |

## Dependency direction

`keycord-runtime` and `keycord-shell` are shared foundations. Subject crates may depend on those
foundations and on lower-level data owners, but never on the root binary. Cross-subject workflows
use explicit ports or callbacks when a direct dependency would create a cycle. The root package
provides those ports and otherwise delegates to subject APIs.

Shell owns the generic window and shortcut skeletons. Subject crates own declarative UI fragments;
Lifecycle composes the application window during the root build, while Shell composes its shortcut
dialog during the Shell build. Both use the build-only `keycord-ui-fragments` crate, which is not a
Shell runtime dependency.

Branding is split at the packaging boundary. Lifecycle owns installed/build artifacts such as app
icons, metadata, desktop files, and installers. Shell owns the product name, subtitle, About label,
and stable icon-name reference rendered by its in-app window chrome; `APP_WINDOW_TITLE` is that
presentation contract.

Each UI-owning crate also loads its fragment into a subject widget bundle and owns that bundle's
focus, search, shortcut, and page-presentation policy. The root `WindowWidgets` value is only an
aggregate of those bundles. It may order routes and connect explicit callbacks, but it must not
redeclare subject builder IDs or reproduce subject controllers.

Compile-time availability and platform permission probes live with the capability they describe:
Docs reports documentation availability, Git reports audit availability, FIDO and Keys report
their device/key capabilities, and Lifecycle reports setup availability. Runtime exposes only
cross-cutting process/environment capabilities.

## Boundary rules

- A behavior has one compiled implementation. The root has no compatibility re-export tree;
  its named `composition` modules only provide the ports and callbacks that connect subjects.
- Window actions and accelerators have one declared owner. Cross-subject dispatch actions remain
  in root composition and delegate their subject behavior through owner callbacks.
- Subject-specific GTK behavior and assets live beside the subject implementation behind optional
  `ui` features. Core feature profiles remain usable without UI dependencies.
- Stable external contracts—application IDs, D-Bus names, CLI switches, on-disk paths and formats,
  MIME types, GSettings keys, and installed filenames—do not depend on crate layout.
- Every crate is private (`publish = false`) and has an empty default feature set.

## Automated guard

`keycord-architecture` validates these stable boundaries without building the application
dependency graph. It checks crate metadata and forbidden edges, subject-owned declarative UI
composition, Lifecycle ownership of packaged branding assets, owner-provided root widget bundles,
and retired detailed owner-UI construction in root composition. Runtime checks cover exact generic
feature/capability and bounded-TOML public inventories plus forbidden subject identifiers. The
Stores/Keys seam is guarded by an exact reviewed Keys recipient-controller API, forbidden direct
key-management identifiers, a retired export-module check, and exact ownership of every shared
recipient builder ID. The FIDO/Keys seam likewise fixes the reviewed FIDO UI API, keeps the FIDO
row/widget/workflow and shared service lifecycle with FIDO, and permits Keys only its OpenPGP
adapter; the conditional root bundle is checked explicitly. Root compatibility facades, retired
root catchalls, and substantial exact Rust duplicates are also rejected. The Architecture CI
workflow runs its formatter, tests, strict Clippy profile, and repository validation.
