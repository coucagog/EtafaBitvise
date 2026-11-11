package app.mlsconnect.desktop

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.Process
import androidx.core.app.NotificationCompat

/** Maintient l'application en vie pendant une session SSH, avec sa notification. */
class SessionService : Service() {
  companion object {
    const val LIBELLE = "libelle"
    private const val DECONNECTER = "app.mlsconnect.desktop.DECONNECTER"
    private const val CANAL = "session"
    private const val NOTIFICATION = 7

    /** Renseigné par l'activité : la déconnexion passe par l'interface. */
    @Volatile var surDeconnexion: (() -> Unit)? = null
  }

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent?.action == DECONNECTER) {
      surDeconnexion?.invoke()
      return START_NOT_STICKY
    }
    val libelle = intent?.getStringExtra(LIBELLE) ?: "Session ouverte"
    val notification = construire(libelle)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
      startForeground(NOTIFICATION, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
    } else {
      startForeground(NOTIFICATION, notification)
    }
    return START_NOT_STICKY
  }

  // L'application fermée depuis les tâches récentes ne garde ni session ni secret en mémoire.
  override fun onTaskRemoved(rootIntent: Intent?) {
    stopSelf()
    Process.killProcess(Process.myPid())
  }

  private fun construire(libelle: String): android.app.Notification {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      val canal = NotificationChannel(CANAL, "Session SSH", NotificationManager.IMPORTANCE_LOW)
      canal.setShowBadge(false)
      getSystemService(NotificationManager::class.java).createNotificationChannel(canal)
    }
    val drapeaux = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    val ouvrir = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP), drapeaux)
    val deconnecter = PendingIntent.getService(this, 1, Intent(this, SessionService::class.java).setAction(DECONNECTER), drapeaux)
    // Sur l'écran de verrouillage, le nom de l'hôte reste masqué.
    val publique = NotificationCompat.Builder(this, CANAL)
      .setSmallIcon(R.drawable.ic_notification)
      .setContentTitle("MLS Connect")
      .setContentText("Session SSH ouverte")
      .build()
    return NotificationCompat.Builder(this, CANAL)
      .setSmallIcon(R.drawable.ic_notification)
      .setContentTitle("MLS Connect")
      .setContentText(libelle)
      .setContentIntent(ouvrir)
      .setOngoing(true)
      .setOnlyAlertOnce(true)
      .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
      .setPublicVersion(publique)
      .addAction(0, "Se déconnecter", deconnecter)
      .build()
  }
}
