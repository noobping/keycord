// Standalone equivalent of Android's byte-array wipe extension; no Android framework required.
package app.passwordstore.util.extensions
fun ByteArray.wipe() = fill(0)
