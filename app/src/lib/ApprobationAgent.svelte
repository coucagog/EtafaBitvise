<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { listen } from '@tauri-apps/api/event'
  import { Bot } from '@lucide/svelte'
  import Boite from './Boite.svelte'

  // Niveau 2 de l'accès agent : chaque commande demandée par mc-mcp attend ici un accord.
  interface Demande { id: number; profil: string; serveur: string; commande: string; delaiS: number }

  let demandes = $state<Demande[]>([])
  const courante = $derived(demandes[0])

  const ajouter = (d: Demande) => { if (!demandes.some((x) => x.id === d.id)) demandes = [...demandes, d].sort((a, b) => a.id - b.id) }

  $effect(() => {
    const ecoutes = [
      listen<Demande>('mc://agent-demande', (e) => ajouter(e.payload)),
      listen<number>('mc://agent-demande-fin', (e) => { demandes = demandes.filter((d) => d.id !== e.payload) }),
    ]
    // Demandes arrivées avant le chargement de l'interface.
    void invoke<Demande[]>('agent_demandes').then((liste) => liste.forEach(ajouter)).catch(() => {})
    return () => ecoutes.forEach((p) => p.then((fin) => fin()))
  })

  function repondre(accepte: boolean) {
    const d = courante
    if (!d) return
    demandes = demandes.filter((x) => x.id !== d.id)
    void invoke('agent_repondre', { id: d.id, accepte })
  }
</script>

{#if courante}
  {#key courante.id}
    <Boite largeur={560} bordure="var(--warn-line)" etiquette="Commande demandée par un agent" surAnnuler={() => repondre(false)}>
      <div class="boite-entete"><Bot size={18} color="var(--warn-500)" /><h3>Commande demandée par un agent</h3></div>
      <div class="boite-corps">
        <p>Un agent demande à exécuter cette commande sur <span class="fort">{courante.profil}</span> (<span class="mono">{courante.serveur}</span>). Elle ne part qu'avec votre accord.</p>
        <pre class="commande mono selectionnable" aria-label="Commande à approuver">{courante.commande}</pre>
        <small class="aide">
          Sans réponse en {courante.delaiS} s, la commande est refusée.
          {#if demandes.length > 1}{demandes.length - 1} autre{demandes.length > 2 ? 's' : ''} demande{demandes.length > 2 ? 's' : ''} en attente.{/if}
        </small>
      </div>
      <div class="boite-pied">
        <!-- Focus initial sur Refuser : Entrée ne doit pas approuver une commande non lue. -->
        <button class="btn btn-discret" data-focus-initial onclick={() => repondre(false)}>Refuser</button>
        <button class="btn btn-primaire" onclick={() => repondre(true)}>Approuver</button>
      </div>
    </Boite>
  {/key}
{/if}

<style>
  .fort { color: var(--text-primary); }
  .commande {
    margin: 0; padding: 10px 12px; max-height: 240px; overflow: auto;
    white-space: pre-wrap; overflow-wrap: anywhere; tab-size: 4;
    background: rgba(0, 0, 0, 0.35); border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    font-size: 12px; line-height: 1.5; color: var(--text-primary);
  }
</style>
