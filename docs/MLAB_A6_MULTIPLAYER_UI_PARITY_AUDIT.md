# MLab a6 / Lot A — Audit de fidélité visuelle Multiplayer

**Référence visuelle** : huit captures de l'original fournies le 13 octobre 2026 (HOST, JOIN, recherche violette/blanche, LINKED violet/blanc, badges Host/Join). **Captures de l'APK KONKR/AVD à recevoir** ; aucune égalité pixel/frame n'a encore été prouvée sur les appareils.

**Sources vérifiées directement :**
- Original : `BrandonKowalski/slot`, `crates/slot/src/app.rs` (`draw_game_menu`), `crates/slot/src/link_screen.rs`, `crates/slot/src/frontend.rs`, `crates/slot-ui/src/link_art.rs`, `plate.rs`, `hud.rs`.
- Fork : `pvaibhav/slot` branche `pvaibhav-improvements`, mêmes fichiers.
- Android : `Baggio94/slot-konkr` branche `feat/konkr-multiplayer-lab`, `crates/slot-android/src/runtime.rs`, `crates/slot/src/link_screen.rs`, `crates/slot-ui/src/{link_art,plate,hud}.rs`.

## 1. Comparaison écran par écran

| Capture originale | Spécification exacte | État dans le code Android a6 | Contrôle restant |
|---|---|---|---|
| **HOST** | Texte central `HOST` à `LINK_TEXT_Y=44` ; flèches latérales ; prise **violette** position `PICK_TIP=330` ; base du port en `PORT_Y` ; touches `B CANCEL`, `SELECT MODE` si mode commutable, `↔ SWAP`, `A LINK` | `menu_face(LinkRow::Host)`, sprites et `draw_link_art()` de l'original ; même ordonnancement des légendes | Comparer pixels/échelle, en particulier `SELECT MODE` selon la ROM/core |
| **JOIN** | Même écran ; prise **blanche/bleutée** et `JOIN` | Même original, source `link_screen.rs` byte-identique ; changement `Left/Right` et `A` | Captures KONKR et AVD |
| **HOST en recherche** | `LOOKING FOR THE OTHER PLAYER` ; prise violette descendant dans le port (200 ms) ; `B CANCEL` uniquement | `LinkStep::Waiting`, `LinkMenu::Working`, `link_hints[..1]` ; pas d'input d'émulation pendant la recherche | Vidéo pour vérifier le moment de passage de `Radio` à `Waiting` |
| **JOIN en recherche** | Identique avec prise blanche, `B CANCEL` | Idem | Vidéo/connexion en vrai |
| **HOST LINKED** | `LINKED` ; insertion complète avec clics ; badges maillons **violets** en haut à droite ; ni touche de validation ni autre barre pendant l'animation | `draw_link_art` + `LinkBadge::Hosting`. **Corrigé par `8273c2c`** : `Hud::draw` maintenant rendu **après** le menu, comme l'original | Contrôler apparition du badge sur captures en cours de transition |
| **JOIN LINKED** | Même animation, maillons **cyan** dans le coin supérieur droit | `LinkBadge::Joined`, mêmes sprites et overlay HUD correctif | Vidéo après paire en vrai |
| **Badge seul HOST** | Maillons violets, icône originale sur fond sombre | `badge_face`, `HUD_ICON_PX`, `LINK_HOST_INK` de `slot-ui` | Comparaison visuelle |
| **Badge seul JOIN** | Maillons cyan, mêmes dimensions/orientation | `LINK_JOIN_INK` et `badge_at` d'origine | Comparaison visuelle |

## 2. Constantes, assets et typographie vérifiés

- `crates/slot/src/link_screen.rs` de la branche KONKR est **byte-identique** à l'original au moment de l'audit. Les valeurs originales `PICK_TIP=330`, `SEATED_TIP=419`, `DROP_MS=200`, `SEAT_MS=160`, `LIFT_MS=250`, `UNPLUG_MS=260`, `ARC_MS=1200`, `ARC_STAGGER_MS=300` sont donc conservées.
- `crates/slot-ui/src/link_art.rs` est **byte-identique** à l'original : SVG/raster des deux prises, port, clics, flèches, adaptateur et arcs Wireless.
- `menu_face()` pour les titres et `hint_face()/arrows_hint_face()` pour les keycaps reprennent le rendu de l'original, avec toile de référence **720 × 480**. Les fonctions utilisées dans `plate.rs` pour les touches sont identiques à l'original, même si ce fichier a des ajouts propres à KONKR.
- Constantes écran copiées exactement de `app.rs` original : `LINK_TEXT_Y=44`, `LINK_LEGEND_Y=422`, `LINK_LEGEND_GAP=40`. Les largeurs comptent `w-HINT_EDGE` pour le centrage, conformément à l'original.
- Fond opaque `opening()` avant sprites/texte ; pas de barres Android ni d'overlay de batterie sur le menu. En jeu, badges intégrés au `Hud`.
- `LINKED_HOLD_MS=1000`, `UNPLUG_HOLD_MS=420` (retrait mécanique 260 ms + attente 160 ms) ; ces timers sont déjà codés/testés.
- Icônes maillons HOST/JOIN et LOST : `LinkBadge` et `badge_face` de Slot ; géométrie `badge_at`. **Écart corrigé** : l'original appelle `hud.draw()` après `draw_game_menu()`. Android oubliait l'appel dans la branche overlay, d'où badge absent pendant LINKED. Correctif `8273c2c`.

## 3. Comportement exact des keycaps (original ET fork)

| Phase `GameMenu` | Légende basse originale | Android a6 |
|---|---|---|
| `Pick` mode réellement commutable | `B CANCEL` / `SELECT MODE` / `← → SWAP` / `A LINK` | Identique, tests `LinkLegendProfile::PickerWithMode` |
| `Pick` non commutable | `B CANCEL` / `← → SWAP` / `A LINK` | Identique ; **ne pas afficher un SELECT inopérant** |
| `Working` | `B CANCEL` seulement | Identique |
| `Linked` animation initiale | Aucune keycap | Identique |
| `Linked` menu rouvert | `B BACK` / `A END LINK` | Identique |
| `Failed` | `A OK` | Identique |
| `Unplug` | Aucune keycap | Identique |

La présence de `SELECT MODE` dans les captures est **conditionnelle dans le code original comme dans le fork** : `link_switchable()` compare les options série des deux modes. Android impose en plus `Core::Gpsp` : le backend mGBA double-GBA actuel ne prend en charge que Cable ; afficher « SELECT MODE » pour un Wireless mGBA indisponible serait trompeur. **Écart fonctionnel explicite, pas une erreur de keycaps.** Il faut comparer pour **la même ROM et le même core** ; ne pas ajouter un bouton qui ne fonctionne pas juste pour ressembler à une image d'un autre contexte.

## 4. Différences originales/fork à ne pas confondre

- Le **fork** ajoute `draw_network_label()` (`net_home` ou `net_direct`), uniquement sur le role picker. L'**original** et les captures utilisateur ne montrent **pas** cette étiquette. Le port Android reproduit l'original plutôt que cette addition du fork ; décider seulement si l'utilisateur veut explicitement la variante fork.
- La KONKR utilise Wi-Fi LAN/mDNS et Mac AVD peut passer par pont Bonjour/Direct-IP plutôt que la radio directe de l'original. Le réseau sous-jacent **ne doit pas changer** le dessin des prises et commandes.
- Les captures fournies sont celles de l'**original**, pas de l'application Android : impossible d'affirmer une ressemblance pixel-perfect ou de quantifier un décalage sur la KONKR sans ses images actuelles.

## 5. Garde-fous et validation à venir

- Commit `5df2073` : une fonction pure `link_legend_profile()` + tests automatiques couvrent tous les états de keycaps, pour empêcher la réapparition d'un bouton `A LINK` dans `Working`/`Linked`.
- Nouvelle CI requise sur ce HEAD (build Android + tests Rust) et tests Mac/KONKR **après** achèvement du code du lot A.
- Comparaison future : même jeu/core/mode, même résolution/proportions, **8 captures** équivalentes, film HOST et JOIN jusqu'au LINKED puis retour `End Link`/Unplug ; comparer positions, titres, graisse/police, teintes, contours, ordre des draw calls, moments d'apparition du badge et respect du dock sous le port.
- Vérifier les actions réelles `B`, `SELECT`, `Left/Right`, `A`, `HOME+SELECT` et leur affichage fidèle ; ne jamais exposer de hotkey décorative non fonctionnelle.
- Tests matériels/installation d'APK encore **différés sur demande** ; ne pas merger PR #29 et ne pas modifier la RC4 stable.

**Roadmap :** ce document est un appendice du lot A. Le reste de la roadmap a6 (B Share Saves → C QA → D consolidation) reste inchangé.
