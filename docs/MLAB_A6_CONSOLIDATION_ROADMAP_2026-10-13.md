# MLab a6 — Consolidation Beta | roadmap de référence

**Projet :** slot. for KONKR Pocket Advance (`fyi.slot.konkr`)  
**PR de développement :** [#29](https://github.com/Baggio94/slot-konkr/pull/29), `feat/konkr-multiplayer-lab` → `integration/konkr-rc4-next`  
**Date de validation :** 13 octobre 2026  
**État :** lots A en développement, B/C/D à venir. **Tests Mac AVD et KONKR physique différés à la demande de l'utilisateur.**  
**Interdit :** modifier `release/konkr-0.1.0-rc4-clean`, merger #29 ou installer une APK a6 pendant cette phase sans décision ultérieure.

## Principes de qualification

- Distinguer **codé**, **tests unitaires/CI verts**, **APK produite**, **vrai essai en jeu**, **validé en conditions matérielles**. Ne jamais appeler une fonctionnalité « validée » sur simple compilation.
- **Une seule APK normale consolidée** pour les deux appareils, plutôt qu'une APK par micro-fix ; même fichier/SHA sur l'AVD et la console. La variante sidecar de Lab n'est pas exigée.
- Conserver tous les fixes et toutes les fonctions RC4, RC5, MLab a3/a4/a5 : GBA/GB/GBC, mGBA/gpSP, carousel, cartouches, Cart Studio officiel, Apps, manette, wallpaper, états et RTC, Polaroids/Undo, RetroArch RZIP, RetroAchievements online/offline, audio/Run-Ahead, interface et options à permissions conditionnelles.
- Le dernier **test effectif** partagé entre KONKR et Mac AVD a porté sur **MLab a4 code 74**, même hash APK, Cable mGBA fonctionnel en jeu (F-Zero: Maximum Velocity, Final Fight One, Super Mario Advance). a5 et a6 ne sont **pas** encore qualifiées physiquement.
- Les changements intermédiaires de source a6 héritent provisoirement de `versionCode=75` et `versionName=0.1.0-rc5-mlab-a5` ; un même code de version **n'établit pas l'identité de l'APK**.

## Lot A — Multiplayer définitif (en cours)

| Sous-lot | Engagement | État / réserve |
| --- | --- | --- |
| **A1 Continuité mGBA** | Reprendre **deux progressions solo distinctes** sans titre lors de l'entrée en `SLK1` ; ne jamais injecter un state cross-core ni écraser la timeline solo | Intégration Android et preuve native mGBA en CI ; essai KONKR/AVD différé |
| **A1 Sessions** | Chaque Host/Join crée `Saves/Multiplayer/mGBA/<hash>/<role>/sessions/<id>` ; les anciennes SRAM/RTC ne sont ni supprimées ni reprises arbitrairement | Codé et tests de préservation ; sélecteur des anciennes sessions non prévu dans a6 |
| **A2 Eject** | La ligne 0 `Eject to play solo` effectue réellement le même Exit sûr que `Save & Eject`, focus après Unplug/HOME | Codé ; test matériel différé |
| **A3 Réseau** | mDNS Android LAN ; pont Bonjour macOS `_slotlink._tcp.local` vers ADB reverse TCP 7222 ; IP manuelle en secours ; KMLB/2 strict inchangé | Codé ; Mac réel, VPN/pare-feu/NAT et DHCP à tester. Conflit de deux hosts même jeu : aucun choix arbitraire, retour à Direct Join |
| **A4 UI/gestes** | SELECT+HOME, menus Pick/Working/Linked/Failed/Unplug, badges Host/Join/Lost/Off, keycaps, animation d'origine Linked **1000 ms** et Unplug **260+160=420 ms**, textes, toasts et **sons seulement s'ils existent** dans l'original/fork | Principalement codé ; comparaison pixel/frame et écoute matérielle différées. Ne pas fabriquer de « Link chime » absent upstream |
| **A5 Protocoles** | **mGBA Cable** double GBA et lockstep ; **gpSP Cable et Wireless/RFU** avec vrai `gpsp_serial` réglé **avant chargement de la ROM**, recovery après reload ; aucune fausse interop mGBA↔gpSP | mGBA Cable prouvé sur a4 ; gpSP RFU non validé physiquement ; tests de déconnexion/absence de frame solo en CI |
| **A5 Reconnexion** | Refuser les peers incompatibles (ROM révision/hash, core, BIOS, backend, Cable/RFU), annuler les recherches obsolètes ; arrêt Link fail-closed et retour **par Eject puis nouvelle session** sans utiliser de SRAM historique | Tests de handshake/cancel/peer lost ; **pas d'auto-reconnexion transparente de session active déclarée** |

**Tests A à mener après la phase code :** vrais Host+Join Mac↔KONKR, deux états distincts, 20 cycles d'entrée/sortie/End/Eject, perte Wi-Fi, réveil, BIOS incompatible, ROM hack/région incorrecte, deux hosts identiques, changement DHCP, 10 cycles mDNS, gpSP Pokémon FireRed/Emerald/Advance Wars puis jeux témoins mGBA. Comparer vidéos/son/gestes au slot. original et au fork `pvaibhav-improvements`. La CI native n'est **pas** une preuve d'un vrai gameplay gpSP.

**Hors garantie a6** : RFU mGBA, Single-Pak/multiboot, 3–4 joueurs, pont inter-core, Splitscreen local. Conserver comme backlog de recherche sans les annoncer comme intégrés.

## Lot B — Share Saves et Save States (à développer après A)

**UI décidée :** Menu → Library → **`Share Saves: mGBA ↔ gpSP    OFF / ON`**, immédiatement avant `Refresh Library`, **OFF par défaut**, réglage persistant. Les points d'entrée `RetroArchStorage.importBeforeLaunch()` et `exportAfterSave()` doivent **réellement conditionner** `importSharedGba/exportSharedGba`. OFF ne doit plus partager une SRAM par accident.

- **OFF :** SRAM, RTC, states et auto-resume totalement distincts par core, y compris les import/export RetroArch.
- **ON :** partage surveillé de la **SRAM uniquement** ; jamais de partage de states, PNG, RTC ou sessions Multiplayer ; pas de `unserialize` inter-core.
- **ON + core inchangé :** reprendre son state cohérent. **ON + changement de core avec state présent :** dialogue « Reprendre l'ancien état de ce core » / « Continuer depuis SRAM partagée » / annuler ; si aucun state, démarrer depuis SRAM.
- **Première activation avec deux SRAM divergentes :** choisir explicitement **mGBA / gpSP / Annuler** ; interdiction de choisir par date/mtime. Comparer contenus/checksums et longueurs, padding `0xFF` seulement, journaux `SharedGBA`, baselines et copies `.before-share.srm`.
- **State ignoré :** ancien state **immuable**, même au prochain Save & Eject, autosave et export RetroArch. Nouvelle génération de states et journal de provenance `(ROM,core,origin,hash,generation)` ; staging, commit atomique et récupération crash-safe.
- **ON → OFF :** arrêter toute synchronisation sans effacer fichiers et historique. Les saves liées sont **toujours** isolées.
- **Gate critique F-Zero Maximum Velocity :** les **six states historiques** (mGBA/gpSP, local/RetroArch) doivent garder leur SHA256 après switch de core avec SRAM partagée ; générer de nouvelles versions plutôt que les réécrire. Refaire le scénario avec `skip-resume.once`, choix de reprise, RZIP v1, RTC, Syncthing, URI SAF, conflits/tailles, disque plein/process kill, GB/GBC.

## Lot C — Qualité de l'émulation, input et audio (après B)

**Suite CLI à développer (non encore disponible) :** `tools/slot-kpa-qa` avec `doctor`, `bench`, `input`, `audio`, `link`, `saves`, `compare`, `report`. ADB **avec `-s <serial>`** ; mesures/debug opt-in, export JSONL/CSV reproductible incluant version/empreintes APK/core/ROM/BIOS, paramètres, CPU/mémoire/thermique et horloges monotones. Ne jamais exporter une ROM, une SRAM personnelle ou des credentials.

- **GBA nominal : ~59,7275 FPS** ; mesurer vitesse d'émulation séparément des FPS affichés, pacing P50/P95/P99, stutters/frames manquées, latence événement→libretro→GL/swap ; **pas de mesure bouton→photon prétendue sans instrument externe**.
- Matrice mGBA/gpSP × Run-Ahead **OFF/1/2** × **STANDARD/RESPONSIVE/DISPLAY SYNC** ; 30 s warmup + 3×180 s et >=15 min chaud sur KONKR, valeurs de référence comparées Mac/console.
- **Son :** PCM pré-AudioTrack, stéréo, fréquence, saturation, clics, trous et répétitions, rollback audio spéculatif propre à **chaque** core, `AudioTrack.underrunCount`/xruns ; tester haut-parleurs/casque/AirPods, sans augmenter aveuglément les buffers au détriment de la latence.
- Préserver les patches audio actuels ; tester Advance Wars 2 et Virtua Tennis gpSP, Worms Blast mGBA, Wario Land 4/Kirby/Mario/ F-Zero/Pokémon ; aucune frame spéculative, rewind ou Fast Forward incontrôlé pendant Link.
- Régresser GB/GBC, SRAM/RTC, auto/manual states, RA online/offline et notifications (pas d'achievements sur frames spéculatives).

## Lot D — Consolidation et bêta unique (après A/B/C)

1. Auditer que **tous les correctifs** RC4/RC5 + MLab a3/a4/a5 et A/B/C ont été repris ; tests Rust/JVM, mGBA et gpSP arm64, JNI, UI/keycaps/animations/gestes, saves, RetroArch et RA ; aucun regress.
2. G1 CI complète verte sur **le HEAD final**, tests natifs et Android, packaging/signature.
3. G2 gameplay réel Link mGBA, gpSP Cable/RFU selon compatibilité déclarée, badges, réseau, 1000/420ms, sons originaux.
4. G3 données : **six states F-Zero immuables**, OFF réellement isolé, ON à choix sûr, RTC/RetroArch/RZIP/Polaroids/Undo et reprise après incident.
5. G4 mesures QA CPU/FPS/audio/input, baselines et comparaison matérielle ; pas de promesse « parfait » sans tests.
6. G5 créer une **seule APK normale `fyi.slot.konkr`**, **SHA256 identique** téléchargé sur Mac/console ; numéro/version a6 propres, notes de version, journal de tests, plan de rollback. **PR #29 reste draft et non mergée avant validation.**

**Décisions préservées** : les tests matériels sont volontairement remis à plus tard ; aucune donnée personnelle, ROM ni état ne doit être supprimé silencieusement ; la branche RC4 stable n'est pas une cible de travail ; l'objectif final est une bêta consolidée, pas une série d'APK dispersées.
