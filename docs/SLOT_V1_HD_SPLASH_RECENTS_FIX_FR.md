# Slot. for KONKR — Correctif V1 HD (splash Android, splash interne, Recents)

## Base / périmètre

- Base de travail : `feat/konkr-dev25-v1-hd-icons` (dev25).
- Correctif : `fix/konkr-dev25-v1-hd-splash-recents`.
- Le design **Slot. V1 validé** est conservé ; aucun changement des coeurs d'émulation, ROMs, sauvegardes ou de la navigation.
- La ressource ES-DE HD (déjà nette sur le KONKR) et les PNG `drawable-nodpi` 1080x1080 **ne sont pas supprimés ni remplacés**.

## Diagnostic

Trois chemins d'affichage différents étaient en jeu :

1. **ES-DE** charge déjà correctement la version HD après dev25 : ne rien casser.
2. **Splash système Android 12** : le thème d'application était celui d'Android par défaut, sans configuration de splash. Il utilisait l'icône fournie/traitée par le système.
3. **Splash interne Slot.** : `MainActivity.showSlotStartupSplash()` affichait `slot-bootlogo-kpa-960x640.png`, agrandi depuis le logo firmware Slot v1.5.0 de 196x75 pixels.
4. **Recents / launcher** : les icônes adaptatives pointaient vers `@drawable/ic_launcher_foreground` (PNG), alors que les vecteurs V1 `ic_launcher_foreground_vector` et `ic_launcher_monochrome_vector` étaient déjà présents.

## Modifications

- `mipmap-anydpi-v26/{ic_launcher,ic_launcher_round}.xml` : le foreground est `@drawable/ic_launcher_foreground_vector`.
- `mipmap-anydpi-v33/{ic_launcher,ic_launcher_round}.xml` : foreground V1 vectoriel + monochrome V1 vectoriel.
- `AndroidManifest.xml` : `android:theme="@style/Theme.SlotKonkr"`.
- `values/slot_launch_theme.xml` : conserve le thème natif Material NoActionBar d'origine.
- `values-v31/slot_launch_theme.xml` : définit `android:windowSplashScreenBackground` et `android:windowSplashScreenAnimatedIcon` vers le foreground V1 vectoriel.
- `drawable/slot_boot_wordmark_vector.xml` : vrai mot-symbole `slot.` vectoriel en blanc sur transparence ; contours issus de la version V1 vectorielle validée.
- `MainActivity.kt` : le second splash affiche ce mot-symbole vectoriel via `ImageView` ; le PNG original reste un **fallback** en cas de problème. Chronométrage, couleur de fond et disparition du splash inchangés.

## Ce qui est vérifié et ce qui ne l'est pas

- Ressources et références contrôlées à partir des fichiers GitHub.
- Modification limitée aux XML d'icônes, aux thèmes Android et au rendu du splash interne.
- **Non compilé** sur ce poste et **non testé physiquement** sur le KONKR.
- Le launcher Android/Recents peut mettre en cache une icône rasterisée même lorsqu'elle provient d'un VectorDrawable. Le correctif améliore la source, **sans garantir** qu'un launcher externe invalide immédiatement son cache.

## Tests demandés avant merge/release

1. Faire tourner le workflow Android habituel de la branche de développement, en conservant la signature de l'APK installée.
2. Vérifier la compilation des nouveaux `VectorDrawable` et attributs natifs du thème Android 12 (minSdk 31, targetSdk 32).
3. Installer l'APK sur le KONKR, ouvrir l'app à froid : vérifier **le splash système** (cartouche V1) puis **le splash interne** (mot-symbole Slot. V1, net).
4. Ouvrir les **Recents**, prendre une capture et vérifier l'icône à la taille réellement affichée.
5. Vérifier **ES-DE / Android Apps** : l'icône doit rester aussi nette qu'en dev25.
6. Tester retour depuis Recents, veille/réveil et ouverture du menu Slot. pour confirmer l'absence de régression de lifecycle.
7. Si Recents est toujours pixelisé malgré la source vectorielle, vider uniquement l'entrée de la tâche des Recents et retester après redémarrage du launcher/appareil. Ne pas effacer les données du launcher sans accord.

## Remarques pour la discussion de développement

- Ne pas remplacer ce correctif par le pack V2/V3/V4 : **le design préféré est le V1**.
- Ne pas supprimer les assets firmware `slot-bootlogo.bmp` ou `slot-bootlogo-kpa-960x640.png` ; le second demeure fallback.
- Aucun réglage d'émulateur, sauvegarde, shader ou option UI n'a été modifié.
- La qualité effective de l'icône Recents doit être validée sur l'appareil, pas simplement déduite de la résolution des fichiers.
