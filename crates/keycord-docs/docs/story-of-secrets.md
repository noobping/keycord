# Story of Secrets

This is a code-oriented walkthrough of how Keycord moves secret data from store creation to password copy.

## Story 1: A Store Is Born

The store flow starts in the Stores-owned [management UI](../../keycord-stores/src/ui/management/mod.rs), with the folder-selection rules in [store management policy](../../keycord-stores/src/management.rs). When the user picks a folder, Keycord checks whether it is empty.

If the folder is not empty, Keycord treats it as an existing store and opens the store-key editor.

If the folder is empty, Keycord opens the create-store version of the store-key page through the Stores-owned [recipient-page controller](../../keycord-stores/src/ui/recipient_page/mod.rs). Create mode immediately queues an autosave, but the save only becomes real once there is at least one recipient.

The recipient page keeps an in-memory list of selected recipients. Before saving, [Stores recipient policy](../../keycord-stores/src/recipients.rs) normalizes that list into standard recipients that belong in `.gpg-id`.

The save action lives in the Stores-owned [recipient-page save controller](../../keycord-stores/src/ui/recipient_page/save.rs). The filesystem transaction is implemented by [the Stores integrated backend](../../keycord-stores/src/integrated.rs), while the root [composition adapter](../../../src/composition/backend/integrated/store.rs) supplies Entries crypto and Git effects:

1. Keycord gathers the current recipients and the current private-key requirement.
2. `save_store_recipients` ensures the store directory exists.
3. It decrypts every existing entry first.
4. It writes updated recipient files.
5. It re-encrypts every entry with the new policy.
6. If the store is brand new, it can initialize Git too.

Two details matter here.

First, recipient files are transactional. [Stores path handling](../../keycord-stores/src/paths.rs) writes the new `.gpg-id` recipient file, runs the reencryption closure, and restores the old file if reencryption fails.

Second, recipients are inherited per path. [Stores path handling](../../keycord-stores/src/paths.rs) resolves an entry's recipients by walking upward until it finds the nearest `.gpg-id`. So the "story of a secret" is really "find the closest recipient file, then use that policy."

## Story 2: A Secret Is Written

The new-item dialog is built in the Entries-owned [new-item UI](../../keycord-entries/src/ui/new_item.rs). It picks a store root and a pass-file label such as `team/service`.

When the editor opens in the Entries-owned [password-page controller](../../keycord-entries/src/ui/page/mod.rs), Keycord seeds the new file from the "new password template" in Preferences. [Pass-file composition](../../keycord-entries/src/file/compose.rs) turns that template into initial plaintext where:

- the first line is the password slot
- later lines are structured fields such as `username:` or `url:`

While the user edits, the Entries-owned [editor UI](../../keycord-entries/src/ui/page/editor.rs) and [pass-file composition](../../keycord-entries/src/file/compose.rs) keep rebuilding the pass-file text in memory. Keycord does not encrypt field-by-field. It always composes one plaintext pass file first, then encrypts the whole thing.

On save, the [password-page controller](../../keycord-entries/src/ui/page/mod.rs) calls the root [backend dispatcher](../../../src/composition/backend/mod.rs), which selects the active backend. For the integrated backend, the root [Entries composition adapter](../../../src/composition/backend/integrated/entries.rs) supplies Keys, Stores, and Git ports to the Entries-owned [integrated entry engine](../../keycord-entries/src/integrated.rs).

That save path does four important things:

1. It resolves the final file path for the label.
2. It loads the crypto context from the nearest recipient files.
3. It encrypts the plaintext according to the store policy.
4. It writes the ciphertext to disk.

The file extension is part of the standard pass layout. [Stores path handling](../../keycord-stores/src/paths.rs) and [entry-file policy](../../keycord-stores/src/entry_files.rs) use `.gpg` for password entries.

## Story 3: Password-Protected Key

This is the normal managed-key path.

The UI for generating the key is in the Keys-owned [private-key management controller](../../keycord-keys/src/ui/key_management/private.rs). The real key generation and import happen in [managed-key storage](../../keycord-keys/src/store/storage.rs):

1. `generate_ripasso_private_key` creates a Sequoia certificate with a required passphrase.
2. It serializes the secret key material.
3. It immediately imports that material back into Keycord's managed-key storage.

Imports use the same storage module. The important rule is enforced in [managed-key storage](../../keycord-keys/src/store/storage.rs): Keycord refuses to keep an unprotected software private key. Imported software keys must already be password protected.

Unlocking is session-based. The Keys-owned [unlock UI](../../keycord-keys/src/ui/unlock.rs) collects the passphrase, then [managed-key unlock logic](../../keycord-keys/src/store/unlock.rs) decrypts the stored key and caches the unlocked certificate in the [Keys session cache](../../keycord-keys/src/cache.rs).

When an entry is read, the Entries-owned [integrated entry engine](../../keycord-entries/src/integrated.rs) obtains its candidate list through [Stores integrated recipient policy](../../keycord-stores/src/integrated_recipients.rs):

- recipients for the entry
- the selected "own" fingerprint, if configured
- every imported managed key

If the needed key is still locked, the read fails with a locked-key error. The copy and open flows catch that error and reroute back into the unlock dialog through the Entries-owned [clipboard controller](../../keycord-entries/src/clipboard.rs) or the Keys-owned [unlock UI](../../keycord-keys/src/ui/unlock.rs).

For encryption, the Entries-owned [integrated crypto context](../../keycord-entries/src/integrated.rs) builds a normal OpenPGP recipient list and encrypts the whole pass file once.

## Story 4: Require All Keys (Experimental)

This experimental option combines the Keys-owned [recipient-key list controller](../../keycord-keys/src/ui/key_management/recipient_list.rs), which presents the available private keys and their selection actions, with the Stores-owned [recipient-page controller](../../keycord-stores/src/ui/recipient_page/list.rs), which exposes the "require all" toggle and applies store recipient policy.

Saving that option does not create a new file. It adds metadata to `.gpg-id`. [Stores integrated recipient policy](../../keycord-stores/src/integrated_recipients.rs) writes:

```text
# keycord-private-key-requirement=all
```

That one comment changes the whole read and write path.

On write, the Entries-owned [integrated crypto context](../../keycord-entries/src/integrated.rs) switches from "any selected key may open this" to experimental layered encryption:

1. Encrypt the plaintext for the innermost required recipient.
2. Wrap that ciphertext in a `keycord-require-all-private-keys-v1` layer.
3. Encrypt that wrapped value for the next recipient.
4. Repeat until every required key has added a layer.

On read, the same module reverses the process one recipient at a time. If even one required key is missing, incompatible, or still locked, the secret does not open.

## Story 5: Experimental FIDO2-Protected Private Key

The FIDO2-protected private-key flow starts from the Keys-owned [private-key management controller](../../keycord-keys/src/ui/key_management/private.rs). Device transport, binding, and envelope logic live in the separate [FIDO crate](../../keycord-fido/src), Keys adapts that service through its [FIDO integration](../../keycord-keys/src/fido2), and the protected private-key bytes are stored through [managed-key storage](../../keycord-keys/src/store/storage.rs).

When the user generates an experimental FIDO2-protected private key:

1. Keycord enrolls an `hmac-secret` credential against the Keycord RP ID.
2. It creates a `FidoBindingDescriptor` with the key fingerprint, display label, and credential id.
3. It stores that descriptor in the private-key manifest beside the protected key material.
4. It encrypts the private-key protection layer with the FIDO2 direct required-layer format.

That descriptor is private-key metadata. It is not a store recipient, it is not written to `.gpg-id`, and Keycord no longer writes a FIDO2 sidecar file.

Unlocking is still session-based. The Keys-owned [unlock UI](../../keycord-keys/src/ui/unlock.rs) can ask for a FIDO2 PIN, then [managed-key unlock logic](../../keycord-keys/src/store/unlock.rs) asks the FIDO service to validate the device. The [FIDO cache](../../keycord-fido/src/cache.rs) retains the PIN for the session, while the [Keys cache](../../keycord-keys/src/cache.rs) retains the unlocked OpenPGP certificate. Once unlocked, that managed key participates in the normal recipient flow described above.

## Story 6: A Secret Is Opened

Opening a password entry starts in the Entries-owned [password-page controller](../../keycord-entries/src/ui/page/mod.rs). The page shows a loading state and then calls `read_password_entry_with_progress`.

The integrated read path in the Entries-owned [integrated entry engine](../../keycord-entries/src/integrated.rs), reached through the root [Entries composition adapter](../../../src/composition/backend/integrated/entries.rs), branches by private-key requirement:

- `AnyManagedKey`: try candidates until one decrypts
- `AllManagedKeys`: require every selected key in order

The crypto context comes from the [Entries integrated engine](../../keycord-entries/src/integrated.rs). The candidate list and recipient metadata come from [Stores integrated recipient policy](../../keycord-stores/src/integrated_recipients.rs).

If the entry opens, the plaintext pass file goes back into the structured editor.

If the key is locked, Keycord surfaces a typed error from [Entries error types](../../keycord-entries/src/error.rs), and the UI can prompt for the missing unlock step instead of just failing.

## Story 7: Copying the Password

The copy button on each password row is wired in the Entries-owned [list-row UI](../../keycord-entries/src/ui/list/row.rs). It calls the Entries-owned [clipboard controller](../../keycord-entries/src/clipboard.rs).

From there the story is short:

1. If the integrated backend is active, Keycord reads only the first line of the entry through `read_password_line`.
2. If the read fails because the key is locked, Keycord resolves the preferred key and shows the unlock dialog.
3. If the read succeeds, Keycord writes the first line to the system clipboard and shows button feedback.

The important detail is that copy is still a decrypt operation. The password is not cached as ready-to-copy plaintext somewhere else in the app. Keycord re-enters the same read path, takes the first line, and hands that text to the clipboard.

If the Host backend is active, the [clipboard controller](../../keycord-entries/src/clipboard.rs) takes a different port supplied by the root [Entries composition adapter](../../../src/composition/entries_ui.rs), which shells out to `pass -c` instead. The rest of this guide follows the integrated path because that is where store-key management, experimental layered encryption, and experimental FIDO2 behavior live.
