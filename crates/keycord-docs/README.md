# Keycord documentation crate

This crate owns bundled documentation assets, locale selection, Markdown parsing,
search, link handling, and the documentation index/detail UI behavior.

The crate owns the following declarative UI fragments:

- `data/window-pages.fragment.ui`
- `data/window-tool-row.fragment.ui`
- `data/shortcuts-tool-item.fragment.ui`

Lifecycle build support composes the window fragments into the generated
application `window.ui`, preserving the single `GtkBuilder` object graph expected
by the root window wiring. The Shell build composes the shortcut fragment into its
generated shortcuts dialog. The window fragments define these integration widgets:

- `docs_page`
- `docs_search_entry`
- `docs_list`
- `docs_detail_page`
- `docs_detail_scrolled`
- `docs_detail_box`

The tool-row fragment supplies `tools_docs_row` as the entry point, and the
shortcut fragment supplies the Open Docs shortcut description. These widgets are
passed into `DocumentationPageWidgets`; no documentation rendering or search
implementation remains in the root package.
