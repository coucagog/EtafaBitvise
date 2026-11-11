package app.mlsconnect.desktop

import android.Manifest
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.View
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  private var vueWeb: WebView? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // En bord à bord, Android ne réduit plus la fenêtre à l'ouverture du clavier :
    // la WebView est réduite ici pour que le contenu reste au-dessus de lui.
    val contenu = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(contenu) { vue, encarts ->
      val clavier = encarts.getInsets(WindowInsetsCompat.Type.ime()).bottom
      vue.setPadding(0, 0, 0, clavier)
      encarts
    }
    SessionService.surDeconnexion = {
      runOnUiThread { vueWeb?.evaluateJavascript("window.dispatchEvent(new Event('mc-deconnecter'))", null) }
    }
  }

  override fun onWebViewCreate(webView: WebView) {
    vueWeb = webView
    webView.addJavascriptInterface(PontSession(), "McAndroid")
  }

  /** Appelé par l'interface à l'ouverture et à la fermeture de la session. */
  inner class PontSession {
    @JavascriptInterface
    fun ouverte(libelle: String) {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        ActivityCompat.requestPermissions(this@MainActivity, arrayOf(Manifest.permission.POST_NOTIFICATIONS), 71)
      }
      val intention = Intent(this@MainActivity, SessionService::class.java).putExtra(SessionService.LIBELLE, libelle)
      // Refusé par le système si l'application n'est plus au premier plan (reconnexion).
      try {
        ContextCompat.startForegroundService(this@MainActivity, intention)
      } catch (e: IllegalStateException) {
        Log.w("MLSConnect", "notification de session non démarrée", e)
      }
    }

    @JavascriptInterface
    fun fermee() {
      stopService(Intent(this@MainActivity, SessionService::class.java))
    }
  }
}
