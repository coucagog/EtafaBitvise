package app.mlsconnect.desktop

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import java.io.File
import java.io.FileNotFoundException

/** Documents choisis dans le sélecteur système, appelé depuis Rust par JNI (crates/mc-core/src/appareil.rs). */
object FichiersAndroid {
  @JvmStatic
  fun nom(contexte: Context, uri: String): String? =
    contexte.contentResolver.query(Uri.parse(uri), arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
      if (it.moveToFirst() && !it.isNull(0)) it.getString(0) else null
    }

  @JvmStatic
  fun lire(contexte: Context, uri: String, destination: String) {
    val entree = contexte.contentResolver.openInputStream(Uri.parse(uri)) ?: throw FileNotFoundException()
    entree.use { e -> File(destination).outputStream().use { e.copyTo(it) } }
  }

  @JvmStatic
  fun ecrire(contexte: Context, source: String, uri: String) {
    val sortie = contexte.contentResolver.openOutputStream(Uri.parse(uri), "wt") ?: throw FileNotFoundException()
    sortie.use { s -> File(source).inputStream().use { it.copyTo(s) } }
  }
}
