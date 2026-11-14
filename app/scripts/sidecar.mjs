// Prépare mc-mcp pour l'empaquetage : le construit puis le copie sous le nom attendu par Tauri
// (src-tauri/binaries/mc-mcp-<triplet>[.exe]). À lancer depuis `app`, avant
// `cargo tauri build --config src-tauri/tauri.paquets.conf.json`.
// Options : --debug (profil de débogage), --target <triplet> (compilation croisée).
import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const app = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const racine = path.resolve(app, '..')
const args = process.argv.slice(2)
const debug = args.includes('--debug')
const cible = args.includes('--target') ? args[args.indexOf('--target') + 1] : null
const hote = /^host: (.+)$/m.exec(execFileSync('rustc', ['-vV'], { encoding: 'utf8' }))?.[1]
const triplet = cible ?? hote
if (!triplet) throw new Error('triplet de la cible introuvable (rustc -vV)')

const cargo = ['build', '-p', 'mc-mcp']
if (!debug) cargo.push('--release')
if (cible) cargo.push('--target', cible)
execFileSync('cargo', cargo, { cwd: racine, stdio: 'inherit' })

const ext = triplet.includes('windows') ? '.exe' : ''
const sortie = path.join(process.env.CARGO_TARGET_DIR ?? path.join(racine, 'target'), cible ?? '', debug ? 'debug' : 'release', `mc-mcp${ext}`)
const dossier = path.join(app, 'src-tauri', 'binaries')
fs.mkdirSync(dossier, { recursive: true })
const destination = path.join(dossier, `mc-mcp-${triplet}${ext}`)
fs.copyFileSync(sortie, destination)
console.log(`mc-mcp (${debug ? 'débogage' : 'final'}) : ${destination}`)
