package app.mlsconnect.desktop

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Secrets des profils, appelé depuis Rust par JNI (crates/mc-core/src/trousseau.rs).
 * Chiffrés par une clé AES du Keystore Android, qui ne quitte jamais le système.
 */
object CoffreAndroid {
  private const val MAGASIN = "AndroidKeyStore"
  private const val ALIAS = "mls-connect-trousseau"
  private const val FICHIER = "mc_trousseau"
  private const val TRANSFORMATION = "AES/GCM/NoPadding"
  private const val LONGUEUR_IV = 12
  private const val LONGUEUR_TAG = 128

  private fun cle(): SecretKey {
    val magasin = KeyStore.getInstance(MAGASIN).apply { load(null) }
    (magasin.getKey(ALIAS, null) as SecretKey?)?.let { return it }
    val generateur = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, MAGASIN)
    generateur.init(
      KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
        .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
        .setKeySize(256)
        .build()
    )
    return generateur.generateKey()
  }

  private fun fichier(contexte: Context) = contexte.getSharedPreferences(FICHIER, Context.MODE_PRIVATE)

  @JvmStatic
  fun ecrire(contexte: Context, id: String, secret: String) {
    val chiffre = Cipher.getInstance(TRANSFORMATION)
    chiffre.init(Cipher.ENCRYPT_MODE, cle())
    // L'identifiant authentifié empêche d'attribuer ce secret à un autre profil.
    chiffre.updateAAD(id.toByteArray())
    val contenu = chiffre.iv + chiffre.doFinal(secret.toByteArray())
    check(fichier(contexte).edit().putString(id, Base64.encodeToString(contenu, Base64.NO_WRAP)).commit())
  }

  @JvmStatic
  fun lire(contexte: Context, id: String): String? {
    val texte = fichier(contexte).getString(id, null) ?: return null
    val contenu = Base64.decode(texte, Base64.NO_WRAP)
    val chiffre = Cipher.getInstance(TRANSFORMATION)
    chiffre.init(Cipher.DECRYPT_MODE, cle(), GCMParameterSpec(LONGUEUR_TAG, contenu, 0, LONGUEUR_IV))
    chiffre.updateAAD(id.toByteArray())
    return String(chiffre.doFinal(contenu, LONGUEUR_IV, contenu.size - LONGUEUR_IV))
  }

  @JvmStatic
  fun effacer(contexte: Context, id: String) {
    check(fichier(contexte).edit().remove(id).commit())
  }
}
