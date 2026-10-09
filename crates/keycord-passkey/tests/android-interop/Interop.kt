@file:OptIn(kotlin.ExperimentalUnsignedTypes::class)

import app.passwordstore.util.passkey.Algorithm
import app.passwordstore.util.passkey.PasskeyCredential
import com.github.michaelbull.result.getOrThrow
import java.io.File
import java.security.KeyFactory
import java.security.KeyPairGenerator
import java.security.Signature
import java.security.spec.ECGenParameterSpec
import java.security.spec.X509EncodedKeySpec
import java.time.Instant
import java.time.ZoneId
import java.util.Base64
import kotlinx.serialization.json.*
import org.bouncycastle.jce.provider.BouncyCastleProvider

private fun ByteArray.b64() = Base64.getUrlEncoder().withoutPadding().encodeToString(this)
private fun decode(text: String) = Base64.getUrlDecoder().decode(text.trim())
private val provider = BouncyCastleProvider()
private val names = listOf("es256", "ed25519", "rs256")

fun main(args: Array<String>) {
    val fixtures = File(args[1])
    if (args[0] == "generate") {
        fixtures.mkdirs()
        for ((index, name) in names.withIndex()) {
            val algorithm = listOf(Algorithm.ES256, Algorithm.EDDSA, Algorithm.RS256)[index]
            val jca = listOf("EC", "Ed25519", "RSA")[index]
            val generator = KeyPairGenerator.getInstance(jca, provider)
            if (name == "es256") generator.initialize(ECGenParameterSpec("secp256r1"))
            if (name == "rs256") generator.initialize(2048)
            val keys = generator.generateKeyPair()
            val credential = PasskeyCredential.createNew(
                credentialId = ByteArray(32) { (it + index * 32).toByte() },
                rpId = "example.com", rpName = if (index == 1) null else "Example",
                userId = byteArrayOf(0, 127, -128, -1), userName = "alice@example.com",
                userDisplayName = if (index == 1) null else "Alice Å",
                algorithm = algorithm, keyPair = keys,
                createdAt = Instant.ofEpochSecond(1700000000), zoneId = ZoneId.of("Europe/Amsterdam"),
            ).copy(signCount = 7u)
            credential.user.revealName = true
            File(fixtures, "$name.b64").writeText(credential.toCbor().b64() + "\n")
            File(fixtures, "$name.spki.b64").writeText(keys.public.encoded.b64() + "\n")
            val cxf = buildJsonObject {
                put("type", "passkey")
                put("credentialId", credential.idByteArray().b64())
                put("rpId", credential.rp.id)
                put("username", credential.user.name)
                credential.user.displayName?.let { put("userDisplayName", it) }
                put("userHandle", credential.user.idByteArray().b64())
                put("key", keys.private.encoded.b64())
            }
            File(fixtures, "$name.cxf.json").writeText(cxf.toString() + "\n")
        }
        println("Generated synthetic fixtures with Android's unchanged serializer.")
        return
    }
    val output = File(args[2])
    for ((index, name) in names.withIndex()) {
        val original = PasskeyCredential.fromCbor(decode(File(fixtures, "$name.b64").readText())).getOrThrow()
        val publicKey = KeyFactory.getInstance(listOf("EC", "Ed25519", "RSA")[index], provider)
            .generatePublic(X509EncodedKeySpec(decode(File(fixtures, "$name.spki.b64").readText())))
        for (kind in listOf("roundtrip", "import")) {
            val candidate = PasskeyCredential.fromCbor(decode(File(output, "$name.$kind.b64").readText())).getOrThrow()
            if (kind == "roundtrip") check(candidate == original) { "$name: metadata changed" }
            check(candidate.id.contentEquals(original.id))
            check(candidate.privateKey.contentEquals(original.privateKey))
            check(candidate.user.id.contentEquals(original.user.id))
            check(candidate.user.name == original.user.name && candidate.rp.id == original.rp.id)
            if (kind == "import") {
                check(candidate.signCount == 0u && candidate.zone == "UTC" && !candidate.user.revealName)
            }
            val challenge = "Keycord Android interoperability".toByteArray()
            val signed = candidate.signData(challenge).getOrThrow()
            val verifier = Signature.getInstance(listOf("SHA256withECDSA", "Ed25519", "SHA256withRSA")[index], provider)
            verifier.initVerify(publicKey)
            verifier.update(challenge)
            check(verifier.verify(signed)) { "$name: signature failed" }
        }
    }
    println("Android decoded and signed with all three Keycord roundtrips and CXF imports.")
}
