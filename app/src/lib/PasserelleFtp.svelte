<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { writeText } from '@tauri-apps/plugin-clipboard-manager'
  import { Check, Copy, Eye, EyeOff, FolderSync, TriangleAlert } from '@lucide/svelte'
  import { app } from './etat.svelte'

  interface AccesFtp { port: number; utilisateur: string; motDePasse: string }

  let acces = $state<AccesFtp | null>(null)
  let port = $state('2121')
  let enCours = $state(false)
  let erreur = $state('')
  let visible = $state(false)
  let copie = $state(false)

  const portValide = $derived(/^\d+$/.test(port.trim()) && Number(port) >= 1 && Number(port) <= 65535)

  // La passerelle se ferme avec la session : son état est relu à chaque changement.
  $effect(() => {
    void app.etatSession
    invoke<AccesFtp | null>('ftp_etat').then((a) => (acces = a)).catch(() => (acces = null))
  })

  async function activer(e: Event) {
    e.preventDefault()
    if (!portValide || enCours) return
    enCours = true
    erreur = ''
    try {
      const a = await invoke<AccesFtp>('ftp_ouvrir', { port: Number(port) })
      acces = a
      visible = false
      app.consigner('INFO', 'ftp', `Passerelle ouverte sur 127.0.0.1:${a.port}`)
    } catch (err) {
      erreur = String(err)
      app.consigner('ERREUR', 'ftp', `Ouverture de la passerelle : ${erreur}`)
    } finally {
      enCours = false
    }
  }

  async function fermer() {
    await invoke('ftp_fermer')
    acces = null
    visible = false
    app.consigner('INFO', 'ftp', 'Passerelle fermée')
  }

  async function copier() {
    if (!acces) return
    await writeText(acces.motDePasse)
    copie = true
    setTimeout(() => (copie = false), 1500)
  }
</script>

<section class="ftp" aria-label="Passerelle FTP">
  <div class="ligne-ftp">
    <FolderSync size={16} color="var(--accent-blue)" />
    <div class="titre-ftp">
      <strong>Passerelle FTP</strong>
      <span>Pour un logiciel qui ne connaît que FTP : ses fichiers passent par cette session SFTP.</span>
    </div>
    {#if acces}
      <span class="active">Active</span>
      <button class="btn btn-secondaire btn-compact" onclick={fermer}>Fermer</button>
    {:else}
      <form onsubmit={activer}>
        <label>Port <input class="champ mono" inputmode="numeric" maxlength="5" bind:value={port} autocomplete="off" /></label>
        <button class="btn btn-secondaire btn-compact" type="submit" disabled={!portValide || enCours}>{enCours ? 'Ouverture…' : 'Activer'}</button>
      </form>
    {/if}
  </div>
  {#if erreur}<p class="message-erreur" role="alert">{erreur}</p>{/if}
  {#if acces}
    <dl>
      <div><dt>Hôte</dt><dd class="mono selectionnable">127.0.0.1</dd></div>
      <div><dt>Port</dt><dd class="mono selectionnable">{acces.port}</dd></div>
      <div><dt>Utilisateur</dt><dd class="mono selectionnable">{acces.utilisateur}</dd></div>
      <div class="secret">
        <dt>Mot de passe</dt>
        <dd class="mono selectionnable">{visible ? acces.motDePasse : '••••••••••••••••'}</dd>
        <button class="btn-icone" aria-label={visible ? 'Masquer le mot de passe' : 'Afficher le mot de passe'} title={visible ? 'Masquer' : 'Afficher'} onclick={() => (visible = !visible)}>
          {#if visible}<EyeOff size={14} />{:else}<Eye size={14} />{/if}
        </button>
        <button class="btn-icone" aria-label="Copier le mot de passe" title="Copier" onclick={copier}>
          {#if copie}<Check size={14} />{:else}<Copy size={14} />{/if}
        </button>
      </div>
    </dl>
    <p class="note"><TriangleAlert size={12} />Écoute sur ce poste uniquement, en mode passif. FTP n'est pas chiffré entre le logiciel et MLS Connect : à n'utiliser que sur un poste de confiance. Le mot de passe change à chaque ouverture.</p>
  {/if}
</section>

<style>
  .ftp { flex: none; display: flex; flex-direction: column; gap: 8px; margin: 8px 12px 0; padding: 8px 12px; background: var(--surface-1); border: 1px solid var(--border-default); border-radius: var(--radius-md); }
  .ligne-ftp { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; }
  .titre-ftp { flex: 1 1 220px; min-width: 0; display: flex; flex-direction: column; gap: 1px; font-size: 13px; }
  .titre-ftp span { font-size: 11px; color: var(--text-muted); }
  form { display: flex; align-items: center; gap: 8px; }
  label { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-secondary); }
  input { width: 72px; }
  .active { font-size: 11px; font-weight: 600; padding: 2px 8px; border-radius: var(--radius-pill); color: var(--ok-500); background: var(--ok-tint); }
  dl { margin: 0; display: flex; flex-wrap: wrap; gap: 6px 22px; }
  dl > div { display: flex; align-items: center; gap: 6px; min-width: 0; }
  dt { font-size: 11px; color: var(--text-muted); }
  dd { margin: 0; font-size: 12px; color: var(--text-primary); overflow-wrap: anywhere; }
  .note { margin: 0; display: flex; align-items: flex-start; gap: 6px; font-size: 11px; color: var(--warn-500); }
  .message-erreur { margin: 0; }
</style>
