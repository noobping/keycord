# Window UI skeleton

`window.ui` is the Shell-owned skeleton for the application's single `GtkBuilder`
object graph. Lifecycle build support replaces every
`keycord-window-fragment:*` marker deterministically and writes the composed file
to the application build's `OUT_DIR`.

`shortcuts.ui` is the corresponding shortcuts-dialog skeleton. The Shell build
script composes its `keycord-shortcuts-fragment:*` markers into Shell's `OUT_DIR`
with the same small build-only fragment library used by Lifecycle.

The Shell skeleton intentionally retains only:

- the application window, toolbar, toast overlay, and navigation-view containers;
- generic header chrome: Back, window title, and primary-menu button;
- the Tools catalog/search scaffolding, generic log rows and copy control, and the
  empty-search group;
- the Logs navigation page; and
- primary-menu scaffolding plus Tools, Keyboard Shortcuts, and About items.

The shortcuts skeleton retains the dialog itself, generic Back/Home navigation,
the generic Tools and General section scaffolding, Open Tools/Open Logs, and Show
Shortcuts/About. Subject-specific sections and items are fragments.

Entries, Preferences, Docs, Git, Stores, Keys, and Lifecycle own the fragment
files inserted at the remaining markers. Fragment paths and marker order are
declared in `keycord-lifecycle` build support. Missing, duplicate, or unresolved
markers fail the build.
