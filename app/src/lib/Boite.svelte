<script lang="ts">
  import type { Snippet } from 'svelte'

  let { largeur, bordure = 'var(--border-default)', ombre = '', etiquette, surAnnuler, children }: {
    largeur: number
    bordure?: string
    ombre?: string
    etiquette: string
    surAnnuler: () => void
    children: Snippet
  } = $props()

  let boite: HTMLDialogElement

  // showModal retient le focus dans la boîte ; Échap passe par surAnnuler.
  $effect(() => {
    boite.showModal()
    boite.querySelector<HTMLElement>('[data-focus-initial]')?.focus()
  })
</script>

<dialog
  bind:this={boite}
  class="boite"
  role="alertdialog"
  aria-label={etiquette}
  style:max-width={`${largeur}px`}
  style:border-color={bordure}
  style:box-shadow={ombre ? `var(--shadow-dialog), ${ombre}` : undefined}
  oncancel={(e) => { e.preventDefault(); surAnnuler() }}
>
  {@render children()}
</dialog>
