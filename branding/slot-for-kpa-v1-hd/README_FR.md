# Slot. for KONKR — v1 HD / ES-DE / Android

**Design figé :** silhouette GBA, logo `slot.`, encoche inférieure et trait courbe de **Slot-for-KPA-Android-Icon-Pack-v1(3).zip**, pas les dessins des packs v2/v3/v4. Couleurs #FCF3DF, #1E2126, blanc. Le design n'a pas été réinterprété.

## Objectif
Rendre le visuel moins pixelisé dans l'écran `Android Apps` d'ES-DE, en conservant la compatibilité Android 12 et les icônes adaptatives.

## Contenu
* `master-svg-v1/slot_v1_foreground.svg` : même vectorisation du pack v1, conservée.
* `master-svg-v1/slot_v1_monochrome.svg` : tracé monochrome d'origine.
* `android/app/src/main/res/drawable-nodpi/ic_launcher_foreground.png` : **1080×1080 RGBA** rendu directement depuis le SVG.
* `android/app/src/main/res/drawable-nodpi/ic_launcher_monochrome.png` : **1080×1080 RGBA** monochrome.
* `android/app/src/main/res/mipmap-anydpi-v26/{ic_launcher,ic_launcher_round}.xml` : icônes adaptatives pour Android 8 à 12.
* `android/app/src/main/res/mipmap-anydpi-v33/*` : idem avec composant monochrome pour Android 13+.
* `android/app/src/main/res/mipmap-*/` : compatibilité legacy, 48/72/96/144/192 px.
* `exports/slot_v1_esde_hd_{1024,2048}.png` : illustration carrée HD de l'icône originale pour tests ou usage ES-DE.
* `exports/slot_v1_google_play_512.png` : 512×512, PNG RGBA sRGB, fond carré sans masque ni ombre.
* `android/app/src/main/res/drawable/*_vector.xml` : VectorDrawable v1 existants conservés comme alternatives, **non activés**.
* `optional-esde-test/` : expérimentation facultative à effectuer seulement si le problème persiste.
* `approved-original-v1/` : fichiers originaux conservés sans modification.

## Intégration dans le dépôt Slot

Cible : `android/app/src/main/res/` sur la branche courante du projet (notamment `feat/konkr-dev23-original-v1-icons`, à confirmer). Ne modifier **aucune logique métier**.

1. Copier les dossiers `android/app/src/main/res/` de ce pack dans les ressources du module Android.
2. **Supprimer les anciens fichiers `drawable-mdpi/ic_launcher_foreground.png`, `drawable-hdpi/...`, `drawable-xhdpi/...`, `drawable-xxhdpi/...`, `drawable-xxxhdpi/...` et les mêmes variantes `ic_launcher_monochrome.png`.** Sinon Android peut préférer ces petites versions aux nouveaux PNG `drawable-nodpi`.
3. Vérifier que les déclarations de `mipmap-anydpi-v26` et `mipmap-anydpi-v33` visent `@drawable/ic_launcher_foreground` et `@drawable/ic_launcher_monochrome`. Ne pas changer `@mipmap/ic_launcher` dans l'AndroidManifest ; garder `@mipmap/ic_launcher_round`.
4. Garder les anciens fichiers `*_vector.xml` hors des déclarations adaptatives par défaut. Ils sont fournis comme référence / solution de repli après comparaison visuelle.
5. Vérifier dans l'APK que `drawable-nodpi` embarque réellement `ic_launcher_foreground.png` **1080×1080** et qu'il n'y a plus de duplicates density-specific pour ce nom.
6. Compiler l'APK, mettre à jour l'application, rafraîchir la liste `Android Apps` d'ES-DE et relancer ES-DE si nécessaire ; comparer une nouvelle capture avec la capture antérieure.
7. Tester aussi l'icône du launcher Android et la forme ronde/masque, ainsi que la mise à jour d'installation.

## Hypothèse et limite
L'écran ES-DE peut demander un petit bitmap via PackageManager, puis l'agrandir indépendamment de la source. Dans ce cas, fournir 1080px ne suffit pas nécessairement à régler la pixellisation. Comparer une capture après rebuild est indispensable pour le savoir. L'option A/B facultative dans `optional-esde-test` permet de diagnostiquer cette cause, sans l'utiliser en production.

## Contrôles produits
`validation.json` liste les dimensions, le mode RGBA, le profil ICC, la taille et la bbox alpha. Les PNG HD proviennent du **SVG de v1**, pas d'un agrandissement de 192 ou 432 px. Les PNG legacy sont rendus séparément depuis le même SVG. La palette et les proportions de la composition originale sont respectées.

## Exemple de comparaison
`previews/v1_old192_nearest_vs_v1_hd376.png` compare à gauche le **legacy 192 px d’origine du ZIP v1** agrandi en 376 px, et à droite le nouveau rendu HD issu du master SVG v1. Ce comparatif est une simulation de redimensionnement, pas une capture de la nouvelle APK dans ES-DE.
