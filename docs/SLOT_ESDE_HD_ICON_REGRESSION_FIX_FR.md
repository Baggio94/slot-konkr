# Slot. for KONKR — Régression de netteté ES-DE après correctif splash/Recents

## Diagnostic confirmé dans les sources
- Branche cible : `feat/konkr-dev26-cart-studio-foundation`.
- Après `dev25`, l'icône était nette dans ES-DE : le foreground adaptatif pointait vers `@drawable/ic_launcher_foreground`, image PNG de **1080 × 1080** dans `drawable-nodpi`.
- Le correctif du splash/Recents a changé la source du foreground vers `@drawable/ic_launcher_foreground_vector` (VectorDrawable tracé sur une grille de 108dp).
- ES-DE affiche désormais une icône perçue comme pixellisée. Ce changement est un candidat direct à la régression ; la confirmation finale nécessite un test A/B sur KONKR.

## Correctif appliqué
- Restaurer `@drawable/ic_launcher_foreground` dans **quatre** ressources :
  - `mipmap-anydpi-v26/ic_launcher.xml`
  - `mipmap-anydpi-v26/ic_launcher_round.xml`
  - `mipmap-anydpi-v33/ic_launcher.xml`
  - `mipmap-anydpi-v33/ic_launcher_round.xml`
- Le foreground PNG 1080×1080 **existe déjà**, on ne le redessine pas.
- Conserver le splash système Android 12+ sur `@drawable/ic_launcher_foreground_vector`, sans modifier son thème.
- Conserver le wordmark vectoriel du splash interne Slot. (avec le PNG original en fallback).
- Ne pas toucher aux `mipmap-*` legacy, ressources HD d'ES-DE, couleur de fond, coeurs, saves, navigation ou Cart Studio.

## Ce qui est attendu
- ES-DE retrouve l'icône HD validée en `dev25`.
- Splash système + splash interne conservent les améliorations vectorielles.
- Rendu des Recents à recontrôler indépendamment ; Android ou le launcher KONKR peuvent rasteriser et/ou mettre l'icône en cache. **Ne pas annoncer que Recents est corrigé** sans capture.

## Vérification après compilation
1. Compiler la branche corrigée avec la signature de dev26.
2. Installer l'APK via `adb install -r` ou procédure habituelle.
3. Vérifier la netteté de l'icône dans ES-DE après rafraîchissement de la liste des apps / redémarrage d'ES-DE si nécessaire (sans effacer ses métadonnées).
4. Vérifier séparément splash Android, splash interne et Recents.
5. Si ES-DE reste pixellisé, contrôler l'image réellement retournée par `PackageManager` sur le KONKR : quel drawable et quelle taille rasterisée ? Confronter avec `drawable-nodpi` 1080px et le cache ES-DE.
6. Envoyer des captures avant/après à la discussion de développement.

## Limites
Le diff du correctif ne comporte que les quatre fichiers XML de l'icône et ce rapport. Aucun test matériel n'a encore été effectué.
