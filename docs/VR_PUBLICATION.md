# Publier un jeu RusteeGear en VR (Meta Quest et autres casques OpenXR)

Phase 9 de [roadmapExportVRQuest24septembre.md](roadmapExportVRQuest24septembre.md).
État au 24 septembre 2026 : **l'APK n'a encore jamais tourné sur un vrai casque**.
Tout ce qui suit suppose que la phase 0 (test « cubes » au casque) est validée.

## 1. Tester sur son propre casque (sideload)

1. Compte développeur Meta (organisation créée sur le tableau de bord développeur),
   puis app **Meta Horizon** sur le téléphone → le casque → **Mode développeur**.
2. Casque branché en USB-C, mis sur la tête : accepter « Autoriser le débogage USB ».
3. Depuis l'éditeur : panneau Export → **Meta Quest · .apk VR** → cocher « Installer et
   lancer sur le casque ». En ligne de commande :

   ```bash
   INSTALL=1 ./packaging/build_quest.sh                 # Rivière
   VR_SCENE=cubes INSTALL=1 ./packaging/build_quest.sh  # test technique (phase 0)
   VR_SCENE=reeduc INSTALL=1 ./packaging/build_quest.sh # rééducation Mouvéo (mains)
   ```

4. Journaux : `~/Library/Android/sdk/platform-tools/adb logcat | grep -E 'motor3derust|OpenXR|panicked'`.
5. Relancer depuis le casque : Bibliothèque → filtre **Sources inconnues**.

Mesurer les performances réelles avec l'**OVR Metrics Tool** (surimpression FPS/GPU
dans le casque) avant tout réglage de `VR_HZ` / `VR_RENDER_SCALE`.

## 2. Build de publication

```bash
RUSTEEGEAR_KEYSTORE_PASS=… APP_VERSION=1.0.0 ./packaging/build_quest.sh --release
```

- Signé avec `packaging/release.keystore` (le même que l'APK Android ; **le secret
  `RUSTEEGEAR_KEYSTORE_PASS` n'est toujours pas configuré dans la CI de release**).
- `APP_VERSION` fixe `versionName` et, via cargo-apk, le `versionCode` : il doit
  **croître à chaque envoi** sur le Store.
- Identifiant : `com.berthod.rusteegear.vr` par défaut, `<bundle>.vr` depuis le
  panneau Export — distinct de l'APK téléphone.

## 3. Liste de contrôle avant soumission au Horizon Store

Les exigences exactes (« VRC ») évoluent : **les relire sur la documentation Meta au
moment de soumettre**. Ce que le moteur fait déjà, et ce qui reste à vérifier au casque :

| Point | État |
|---|---|
| Lancement en immersif (catégorie VR du manifeste), `android.hardware.vr.headtracking` requis | ✅ manifeste |
| Perte de focus (menu système, casque retiré) → jeu en pause, entrées ignorées ; reprise au retour | ✅ `XrContent::set_focused` (test) — **à vérifier au casque** |
| Quitter proprement depuis le jeu (menu → Quitter → fin de session OpenXR) et via le menu système | ✅ code — **à vérifier au casque** |
| Fréquence d'affichage tenue sans saccade (72 Hz minimum) | ⚠️ mesuré sur Mac seulement (~10 ms/image) — **OVR Metrics Tool au casque** |
| Confort : rotation par crans, vignette, pas de secousse de caméra, vue spectateur | ✅ (phase 4) — note de confort à déclarer sur la fiche |
| Suivi des mains : permission `com.oculus.permission.HAND_TRACKING` déclarée, fonctionnement sans manettes | ✅ manifeste, menu au pincement gauche tenu — **à vérifier au casque** |
| Politique de confidentialité (obligatoire dès qu'on utilise le suivi des mains) | ❌ à rédiger (données de mains non transmises par le moteur) |
| Icône, visuels de la fiche Store, vidéo | ❌ à produire (`quest_sim --snapshot` pour des premières captures) |
| Plantages / réseau hors ligne | ⚠️ à tester au casque (le jeu en ligne passe par `wss://ws.loicberthod.ch`) |

## 4. Autres casques OpenXR Android

Le même APK OpenXR, avec des métadonnées adaptées (`VR_TARGET`) :

| Casque | Commande | État |
|---|---|---|
| Meta Quest 2, 3, 3S, Pro | `VR_TARGET=quest` (défaut) | cible principale |
| Pico 4 / 4 Ultra | `VR_TARGET=pico` | manifeste généré (`pvr.app.type`) — **non vérifié**, confirmer avec la doc Pico |
| Android XR (Samsung Galaxy XR…) | `VR_TARGET=androidxr` | manifeste généré (`android.software.xr.api.openxr`) — **non vérifié** |

Tous reçoivent la catégorie d'intent standard `org.khronos.openxr.intent.category.IMMERSIVE_HMD`
et le loader OpenXR Khronos embarqué.

## 5. PC VR (SteamVR, Quest Link) — non fait

Le code OpenXR (`src/xr/hello.rs`) est aujourd'hui réservé à Android (point d'entrée
`android_main`, `AndroidPlatformInfo`). Un exécutable PC VR demanderait : un point
d'entrée desktop, le loader OpenXR du système, le backend Vulkan de wgpu sous
Windows/Linux. Tout le reste (`xr::content`, rendu, entrées, interface) est déjà
partagé. Utile surtout pour itérer vite depuis un PC Windows avec un Quest en Link ;
impossible à valider sur macOS (aucun runtime OpenXR).
