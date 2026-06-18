# Build & signature iOS (`.ipa`)

État : **`.ipa` signé avec votre certificat Apple Development** ✅
Reste **une étape** pour l'installer sur un iPhone : un **profil de provisioning**.

## Ce qui fonctionne
- Cross-compilation `aarch64-apple-ios` (tout le moteur). ✅
- `.ipa` assemblé et **signé** avec `Apple Development: lberthod@gmail.com` (Team `N668CK695Q`). ✅
- Script : `packaging/build_ios.sh`.

```bash
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
./packaging/build_ios.sh           # → target/ios/Motor3DeRust.ipa (signé)
```

## La dernière étape : le profil de provisioning
iOS exige un `embedded.mobileprovision` qui lie **App ID + certificat + UDID de l'appareil**.
Il n'existe pas encore (aucun profil dans `~/Library/MobileDevice/Provisioning Profiles/`).

### Option A — Xcode crée le profil automatiquement (le plus simple)
1. Brancher l'iPhone en USB, le déverrouiller, « Faire confiance ».
2. Ouvrir Xcode ▸ *Settings ▸ Accounts* : vérifier le compte `lberthod@gmail.com`.
3. Créer un petit projet App (ou ouvrir un projet) avec **Bundle Identifier = `com.berthod.motor3derust`**,
   Team = votre équipe, **Automatically manage signing** coché, l'iPhone sélectionné comme cible.
   Xcode enregistre l'appareil et génère le profil de développement.
4. Récupérer le profil généré :
   ```bash
   ls ~/Library/MobileDevice/Provisioning\ Profiles/*.mobileprovision
   ```
5. Re-signer notre `.ipa` avec ce profil :
   ```bash
   PROFILE=~/Library/MobileDevice/Provisioning\ Profiles/XXXX.mobileprovision ./packaging/build_ios.sh
   ```
6. Installer : appareil connecté ▸ `xcrun devicectl device install app target/ios/Motor3DeRust.ipa`
   (ou glisser l'`.ipa`/`.app` dans Xcode ▸ Devices).

### Option B — Portail développeur (compte payant)
developer.apple.com ▸ Certificates, IDs & Profiles :
- enregistrer l'App ID `com.berthod.motor3derust`,
- enregistrer l'UDID de l'appareil,
- créer un profil **iOS App Development**, le télécharger, puis `PROFILE=… ./packaging/build_ios.sh`.

> Sans profil, l'`.ipa` est correctement signé mais refusé au lancement par l'appareil.
> Pour une vraie distribution App Store, il faudra un profil *Distribution* + notarisation.
